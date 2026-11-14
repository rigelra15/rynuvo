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
            let (title, body) = format(ch, nick, &m.content, m.image.is_some());
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
    use super::format;

    #[test]
    fn formats_and_truncates() {
        let long = "a".repeat(100);
        let (t, b) = format("umum", "Ayu", &long, false);
        assert_eq!(t, "#umum");
        assert_eq!(b, format!("Ayu: {}…", "a".repeat(80)));
        assert_eq!(format("x", "Bayu", "", true).1, "Bayu: sent an image");
    }
}
