pub mod hub;
pub use hub::Hub;

use std::time::Duration;

use axum::{
    Router,
    extract::{
        State, WebSocketUpgrade,
        ws::{CloseFrame, Message, WebSocket},
    },
    response::Response,
    routing::get,
};
use hub::{Outgoing, Status};
use rynuvo_proto::{CLOSE_AUTH_REVOKED, CLOSE_PROTOCOL_OLD, LastSeen, PROTOCOL_VERSION, ReadMark, User, WsClientMsg, WsServerMsg};

use crate::{app::AppState, auth, now_ms, routes::channels::list_channels};

const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
const PING_EVERY: Duration = Duration::from_secs(20);
const IDLE_TIMEOUT: Duration = Duration::from_secs(45);
const SEND_TIMEOUT: Duration = Duration::from_secs(10);
/// Clients only send small control messages; anything larger is hostile.
const MAX_MESSAGE: usize = 64 * 1024;

pub fn router() -> Router<AppState> {
    Router::new().route("/ws", get(upgrade))
}

async fn upgrade(State(st): State<AppState>, ws: WebSocketUpgrade) -> Response {
    ws.max_message_size(MAX_MESSAGE).max_frame_size(MAX_MESSAGE).on_upgrade(move |socket| handle(st, socket))
}

async fn close(mut socket: WebSocket, code: u16, reason: &'static str) {
    let _ = socket.send(Message::Close(Some(CloseFrame { code, reason: reason.into() }))).await;
}

async fn send_raw(socket: &mut WebSocket, msg: Message) -> bool {
    matches!(tokio::time::timeout(SEND_TIMEOUT, socket.send(msg)).await, Ok(Ok(())))
}

async fn send(socket: &mut WebSocket, msg: &WsServerMsg) -> bool {
    let txt = serde_json::to_string(msg).expect("serialize");
    send_raw(socket, Message::Text(txt.into())).await
}

async fn read_hello(socket: &mut WebSocket) -> Option<(String, u32, bool)> {
    let msg = tokio::time::timeout(HELLO_TIMEOUT, socket.recv()).await.ok()??.ok()?;
    let Message::Text(t) = msg else { return None };
    match serde_json::from_str::<WsClientMsg>(t.as_str()).ok()? {
        WsClientMsg::Hello { token, protocol, mobile } => Some((token, protocol, mobile)),
        _ => None,
    }
}

async fn snapshot(st: &AppState, me: User) -> anyhow::Result<WsServerMsg> {
    let users: Vec<(i64, String, bool, Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT id, nickname, is_admin, avatar_icon, avatar_color, avatar_image FROM users WHERE removed_at IS NULL ORDER BY id").fetch_all(&st.db).await?;
    let channels = list_channels(&st.db).await.map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let reads = sqlx::query_as::<_, (i64, Option<i64>, i64)>(
        "SELECT c.id, (SELECT MAX(m.id) FROM messages m WHERE m.channel_id = c.id), COALESCE(r.last_read, 0)
         FROM channels c LEFT JOIN read_state r ON r.channel_id = c.id AND r.user_id = ?
         WHERE c.kind = 'text'",
    )
    .bind(me.id)
    .fetch_all(&st.db)
    .await?
    .into_iter()
    .map(|(channel_id, last_message, last_read)| ReadMark { channel_id, last_message, last_read })
    .collect();
    let alerts = if me.is_admin { st.disk.alert().into_iter().chain(st.relay.quota.alert()).collect() } else { Vec::new() };
    Ok(WsServerMsg::Ready {
        me,
        users: users.into_iter().map(|(id, nickname, is_admin, avatar_icon, avatar_color, avatar_image)| User { id, nickname, is_admin, avatar_icon, avatar_color, avatar_image }).collect(),
        channels,
        online: st.hub.online_ids(),
        mobile: st.hub.mobile_ids(),
        last_seen: sqlx::query_as::<_, (i64, i64)>("SELECT user_id, MAX(last_seen) FROM devices WHERE last_seen IS NOT NULL GROUP BY user_id")
            .fetch_all(&st.db)
            .await?
            .into_iter()
            .map(|(user_id, at)| LastSeen { user_id, at })
            .collect(),
        alerts,
        voice: st.voice.members(),
        streams: st.voice.streams(),
        music: st.music.states(crate::now_ms()),
        music_bot: crate::music::wire::bot_id(st).await,
        reads,
        activities: st.hub.activities(),
    })
}

/// Move my read mark in a text channel forward (never back, never past its newest message).
/// The mark it ends at, if it moved.
pub async fn mark_read(st: &AppState, user_id: i64, channel_id: i64, message_id: i64) -> Option<i64> {
    let r = sqlx::query_scalar::<_, i64>(
        "INSERT INTO read_state (user_id, channel_id, last_read)
         SELECT ?, c.id, MIN(?, m.last) FROM channels c
           JOIN (SELECT MAX(id) AS last FROM messages WHERE channel_id = ?3) m ON m.last IS NOT NULL
           WHERE c.id = ?3 AND c.kind = 'text'
         ON CONFLICT (user_id, channel_id) DO UPDATE SET last_read = excluded.last_read
           WHERE excluded.last_read > read_state.last_read
         RETURNING last_read",
    )
    .bind(user_id)
    .bind(message_id)
    .bind(channel_id)
    .fetch_optional(&st.db)
    .await;
    r.ok().flatten()
}

async fn handle(st: AppState, mut socket: WebSocket) {
    let Some((token, protocol, mobile)) = read_hello(&mut socket).await else {
        return close(socket, 1008, "expected hello").await;
    };
    if protocol < PROTOCOL_VERSION {
        return close(socket, CLOSE_PROTOCOL_OLD, "update required").await;
    }
    let au = match auth::user_by_token(&st.db, &token).await {
        Ok(Some(au)) => au,
        _ => return close(socket, CLOSE_AUTH_REVOKED, "invalid token").await,
    };
    let _ = sqlx::query("UPDATE devices SET last_seen = ? WHERE id = ?").bind(now_ms()).bind(au.device_id).execute(&st.db).await;

    let user_id = au.user.id;
    let is_admin = au.user.is_admin;
    let device_id = au.device_id;
    let reg = st.hub.register(user_id, au.user.is_admin, mobile);
    let (conn_id, mut rx, mut kill) = (reg.id, reg.rx, reg.kill);
    st.hub.set_device(conn_id, device_id);
    let ready = match snapshot(&st, au.user).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("snapshot failed: {e:#}");
            st.hub.unregister(conn_id);
            return close(socket, 1011, "server error").await;
        }
    };
    if send(&mut socket, &ready).await {
        if let (true, Status::Online { mobile }) = (reg.before != reg.after, reg.after) {
            st.hub.broadcast_except(user_id, WsServerMsg::Presence { user_id, online: true, mobile, last_seen: None });
        }
        run(&st, &mut socket, &mut rx, &mut kill, user_id, is_admin, conn_id).await;
    }
    crate::voice::conn_closed(&st, conn_id);
    // Last seen is when the device let go, not when it connected.
    let now = now_ms();
    let _ = sqlx::query("UPDATE devices SET last_seen = ? WHERE id = ?").bind(now).bind(device_id).execute(&st.db).await;
    if let Some((uid, before, after, game)) = st.hub.unregister(conn_id) {
        if let Some(game) = game {
            st.hub.broadcast_except(uid, WsServerMsg::Activity(rynuvo_proto::Activity { user_id: uid, game }));
        }
        if before != after {
            let msg = match after {
                Status::Offline => WsServerMsg::Presence { user_id: uid, online: false, mobile: false, last_seen: Some(now) },
                Status::Online { mobile } => WsServerMsg::Presence { user_id: uid, online: true, mobile, last_seen: None },
            };
            st.hub.broadcast_except(uid, msg);
        }
    }
}

async fn run(
    st: &AppState,
    socket: &mut WebSocket,
    rx: &mut tokio::sync::mpsc::Receiver<Outgoing>,
    kill: &mut tokio::sync::watch::Receiver<bool>,
    user_id: i64,
    is_admin: bool,
    conn_id: u64,
) {
    let mut ping = tokio::time::interval(PING_EVERY);
    ping.tick().await;
    let mut last_rx = tokio::time::Instant::now();
    loop {
        tokio::select! {
            _ = kill.changed() => return,
            out = rx.recv() => match out {
                Some(Outgoing::Msg(m)) => if !send(socket, &m).await { return },
                Some(Outgoing::Bin(b)) => if !send_raw(socket, Message::Binary(b.into())).await { return },
                Some(Outgoing::Close(code, reason)) => {
                    let _ = send_raw(socket, Message::Close(Some(CloseFrame { code, reason: reason.into() }))).await;
                    return;
                }
                None => return,
            },
            inc = socket.recv() => {
                let Some(Ok(msg)) = inc else { return };
                last_rx = tokio::time::Instant::now();
                match msg {
                    Message::Text(t) => match serde_json::from_str::<WsClientMsg>(t.as_str()) {
                        Ok(WsClientMsg::Typing { channel_id }) => {
                            if st.hub.allow_typing(user_id, channel_id) {
                                st.hub.broadcast_except(user_id, WsServerMsg::Typing { channel_id, user_id });
                            }
                        }
                        Ok(WsClientMsg::Read { channel_id, message_id }) => {
                            if let Some(last_read) = mark_read(st, user_id, channel_id, message_id).await {
                                st.hub.send_to_user_except(user_id, conn_id, WsServerMsg::ReadUpdated { channel_id, last_read });
                            }
                        }
                        Ok(WsClientMsg::Ping) => if !send(socket, &WsServerMsg::Pong).await { return },
                        Ok(WsClientMsg::VoiceJoin { channel_id, pubkey }) => crate::voice::join(st, user_id, conn_id, channel_id, pubkey).await,
                        Ok(WsClientMsg::E2eeKey { to, scope, generation, envelope }) => crate::voice::forward_key(st, conn_id, user_id, to, scope, generation, envelope),
                        Ok(WsClientMsg::E2eeKeyRequest { from, scope }) => crate::voice::forward_key_request(st, conn_id, user_id, from, scope),
                        Ok(WsClientMsg::VoiceLeave) => crate::voice::leave(st, user_id),
                        Ok(WsClientMsg::VoiceState { muted, deafened }) => crate::voice::set_state(st, user_id, muted, deafened),
                        Ok(WsClientMsg::VoiceKick { user_id: target }) => crate::voice::kick(st, user_id, is_admin, target).await,
                        Ok(WsClientMsg::VoiceInvite { to }) => crate::voice::invite(st, user_id, to),
                        Ok(WsClientMsg::Away { away }) => st.hub.set_away(conn_id, away),
                        Ok(WsClientMsg::Activity { game }) => {
                            // A name, not an essay: trimmed and cut short.
                            let game = game.map(|g| g.trim().chars().take(64).collect::<String>()).filter(|g| !g.is_empty());
                            if let Some((uid, game)) = st.hub.set_game(conn_id, game) {
                                st.hub.broadcast(WsServerMsg::Activity(rynuvo_proto::Activity { user_id: uid, game }));
                            }
                        }
                        Ok(WsClientMsg::StreamStart { codecs, preset }) => crate::voice::stream_start(st, user_id, codecs, preset),
                        Ok(WsClientMsg::StreamStop) => crate::voice::stream_stop(st, user_id),
                        Ok(WsClientMsg::StreamWatch { user_id: streamer, codecs }) => crate::voice::stream_watch(st, user_id, streamer, codecs),
                        Ok(WsClientMsg::StreamUnwatch { user_id: streamer }) => crate::voice::stream_unwatch(st, user_id, streamer),
                        Ok(WsClientMsg::StreamSignal { to, data }) => crate::voice::stream_signal(st, user_id, to, data),
                        Ok(WsClientMsg::Music { cmd, text_channel_id }) => crate::music::wire::command(st, user_id, is_admin, cmd, text_channel_id).await,
                        _ => {}
                    },
                    Message::Binary(b) => st.relay.from_ws(conn_id, &b).await,
                    Message::Close(_) => return,
                    _ => {}
                }
            }
            _ = ping.tick() => {
                if last_rx.elapsed() > IDLE_TIMEOUT { return }
                if !send_raw(socket, Message::Ping(Vec::new().into())).await { return }
            }
        }
    }
}
