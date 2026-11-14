//! Desktop notifications for messages from others while the window is unfocused.
use std::{collections::HashMap, sync::Mutex};

use rynuvo_proto::{Id, WsServerMsg};
use tauri::{AppHandle, Manager, Runtime};
#[cfg(not(target_os = "macos"))]
use tauri_plugin_notification::NotificationExt;

use crate::core::session::SessionEvent;

#[derive(Default)]
struct Names {
    me: Option<Id>,
    users: HashMap<Id, String>,
    channels: HashMap<Id, String>,
}

static NAMES: Mutex<Option<Names>> = Mutex::new(None);
/// Catch-up after a reconnect replays older messages; don't notify for those.
const FRESH_MS: i64 = 60_000;

/// "#channel · nickname: first 80 chars". Pure, for testing.
/// Does `content` @mention `me` (or @everyone)? Same rules as the app's `findMentions`: case-insensitive,
/// word boundaries on both sides, and the longest matching name wins ("@Budi Santoso" is not "@Budi").
pub fn mentions_me(content: &str, me: &str, names: &[&str]) -> bool {
    let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    let mut names: Vec<String> = names.iter().map(|n| n.to_lowercase()).chain(["everyone".to_string()]).collect();
    names.sort_by_key(|n| std::cmp::Reverse(n.chars().count()));
    let me = me.to_lowercase();
    let chars: Vec<char> = content.chars().collect();
    let lower: Vec<char> = chars.iter().map(|c| c.to_lowercase().next().unwrap_or(*c)).collect();
    for i in 0..chars.len() {
        if chars[i] != '@' || (i > 0 && word(Some(chars[i - 1]))) {
            continue;
        }
        let hit = names.iter().find(|n| {
            let n: Vec<char> = n.chars().collect();
            let end = i + 1 + n.len();
            end <= lower.len() && lower[i + 1..end] == n[..] && !word(chars.get(end).copied())
        });
        if let Some(n) = hit {
            if *n == me || n == "everyone" {
                return true;
            }
        }
    }
    false
}

pub fn format(channel: &str, nick: &str, content: &str, has_image: bool) -> (String, String) {
    let mut text: String = content.chars().take(80).collect();
    if content.chars().count() > 80 {
        text.push('…');
    }
    if text.is_empty() && has_image {
        text = "sent an image".into();
    }
    (format!("#{channel}"), format!("{nick}: {text}"))
}

pub fn on_event<R: Runtime>(app: &AppHandle<R>, e: &SessionEvent) {
    let SessionEvent::Server(msg) = e else { return };
    let mut g = NAMES.lock().unwrap();
    let n = g.get_or_insert_with(Names::default);
    match msg {
        WsServerMsg::Ready { me, users, channels, .. } => {
            n.me = Some(me.id);
            n.users = users.iter().map(|u| (u.id, u.nickname.clone())).collect();
            n.channels = channels.iter().map(|c| (c.id, c.name.clone())).collect();
        }
        WsServerMsg::UserUpserted(u) => {
            n.users.insert(u.id, u.nickname.clone());
        }
        WsServerMsg::ChannelCreated(c) | WsServerMsg::ChannelUpdated(c) => {
            n.channels.insert(c.id, c.name.clone());
        }
        WsServerMsg::ChannelDeleted { id } => {
            n.channels.remove(id);
        }
        WsServerMsg::MessageCreated(m) => {
            if Some(m.author_id) == n.me || crate::core::now_ms() - m.created_at > FRESH_MS {
                return;
            }
            let focused = app.webview_windows().values().any(|w| w.is_focused().unwrap_or(false));
            tracing::debug!(focused, "message from other user");
            if focused {
                return;
            }
            let ch = n.channels.get(&m.channel_id).map_or("channel", String::as_str);
            let nick = n.users.get(&m.author_id).map_or("someone", String::as_str);
            let (mut title, body) = format(ch, nick, &m.content, m.image.is_some());
            let names: Vec<&str> = n.users.values().map(String::as_str).collect();
            let me_nick = n.me.and_then(|id| n.users.get(&id)).map_or("", String::as_str);
            if !me_nick.is_empty() && mentions_me(&m.content, me_nick, &names) {
                title = format!("{nick} mentioned you in #{ch}");
            }
            #[cfg(target_os = "macos")]
            crate::mac_notify::show(&title, &body);
            #[cfg(not(target_os = "macos"))]
            if let Err(e) = app.notification().builder().title(title).body(body).show() {
                tracing::warn!("notification failed: {e}");
            }
        }
        _ => {}
    }
}

/// Forget names on logout so a different account doesn't see stale data.
pub fn reset() {
    *NAMES.lock().unwrap() = None;
}

#[cfg(test)]
mod tests {
    use super::{format, mentions_me};

    #[test]
    fn mention_matches_the_same_way_as_the_app() {
        let names = ["Rigel", "Budi", "Budi Santoso"];
        assert!(mentions_me("yo @rigel!", "Rigel", &names));
        assert!(mentions_me("@everyone game?", "Rigel", &names));
        assert!(!mentions_me("mail@rigel.dev", "Rigel", &names), "inside a word");
        assert!(!mentions_me("@rigelx", "Rigel", &names), "longer word");
        assert!(!mentions_me("hi @Budi Santoso", "Budi", &names), "longest name wins");
        assert!(mentions_me("hi @budi, ok", "Budi", &names));
    }

    #[test]
    fn formats_and_truncates() {
        let long = "a".repeat(100);
        let (t, b) = format("umum", "Ayu", &long, false);
        assert_eq!(t, "#umum");
        assert_eq!(b, format!("Ayu: {}…", "a".repeat(80)));
        assert_eq!(format("x", "Bayu", "", true).1, "Bayu: sent an image");
    }
}
