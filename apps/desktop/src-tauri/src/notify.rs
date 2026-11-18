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
    /// Who is in which voice room, to tell "came in" from a mute change.
    in_voice: HashMap<Id, Id>,
    /// When each room last emptied, so a hangout that blinks out and back isn't a new one.
    emptied: HashMap<Id, i64>,
}

/// A room empty for less than this and filled again is the same hangout (a dropped connection).
const SAME_HANGOUT_MS: i64 = 2 * 60_000;

fn show<R: Runtime>(app: &AppHandle<R>, title: String, body: String) {
    #[cfg(target_os = "macos")]
    {
        let _ = app;
        crate::mac_notify::show(&title, &body);
    }
    #[cfg(not(target_os = "macos"))]
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        tracing::warn!("notification failed: {e}");
    }
}

static NAMES: Mutex<Option<Names>> = Mutex::new(None);
/// Catch-up after a reconnect replays older messages; don't notify for those.
const FRESH_MS: i64 = 60_000;

pub use rynuvo_proto::mentions_me;

/// "#channel · nickname: first 80 chars". Pure, for testing.
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
        WsServerMsg::Ready { me, users, channels, voice, .. } => {
            // Who's already in a room when I connect isn't news.
            n.in_voice = voice.iter().map(|v| (v.user_id, v.channel_id)).collect();
            n.me = Some(me.id);
            n.users = users.iter().map(|u| (u.id, u.nickname.clone())).collect();
            n.channels = channels.iter().map(|c| (c.id, c.name.clone())).collect();
        }
        WsServerMsg::VoiceMemberUpdated(v) => {
            // A hangout starting: someone comes into an empty room. (Desktop notifications can't be
            // updated in place, so only the start says something; the sidebar shows who comes after.)
            let was_empty = !n.in_voice.values().any(|c| *c == v.channel_id);
            let came_in = n.in_voice.insert(v.user_id, v.channel_id) != Some(v.channel_id);
            if !came_in || !was_empty || Some(v.user_id) == n.me {
                return;
            }
            let now = crate::core::now_ms();
            if n.emptied.get(&v.channel_id).is_some_and(|at| now - at < SAME_HANGOUT_MS) {
                return;
            }
            if app.webview_windows().values().any(|w| w.is_focused().unwrap_or(false)) {
                return; // the sidebar shows it
            }
            let room = n.channels.get(&v.channel_id).map_or("a room", String::as_str);
            let nick = n.users.get(&v.user_id).map_or("Someone", String::as_str);
            show(app, format!("{nick} is in {room}"), "Hop in and hang out?".into());
        }
        WsServerMsg::VoiceMemberLeft { user_id, channel_id } => {
            n.in_voice.remove(user_id);
            if !n.in_voice.values().any(|c| c == channel_id) {
                n.emptied.insert(*channel_id, crate::core::now_ms());
            }
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
        WsServerMsg::VoiceInvited { channel_id, from } => {
            let focused = app.webview_windows().values().any(|w| w.is_focused().unwrap_or(false));
            if focused {
                return; // the card in the app says it
            }
            let room = n.channels.get(channel_id).map_or("a room", String::as_str);
            let nick = n.users.get(from).map_or("Someone", String::as_str);
            show(app, format!("{nick} wants you in {room}"), "Open Rynuvo to join them.".into());
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
