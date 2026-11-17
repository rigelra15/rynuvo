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
use hub::Outgoing;
use rynuvo_proto::{CLOSE_AUTH_REVOKED, CLOSE_PROTOCOL_OLD, PROTOCOL_VERSION, User, WsClientMsg, WsServerMsg};

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

async fn read_hello(socket: &mut WebSocket) -> Option<(String, u32)> {
    let msg = tokio::time::timeout(HELLO_TIMEOUT, socket.recv()).await.ok()??.ok()?;
    let Message::Text(t) = msg else { return None };
    match serde_json::from_str::<WsClientMsg>(t.as_str()).ok()? {
        WsClientMsg::Hello { token, protocol } => Some((token, protocol)),
        _ => None,
    }
}

async fn snapshot(st: &AppState, me: User) -> anyhow::Result<WsServerMsg> {
    let users: Vec<(i64, String, bool, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT id, nickname, is_admin, avatar_icon, avatar_color FROM users WHERE removed_at IS NULL ORDER BY id").fetch_all(&st.db).await?;
    let channels = list_channels(&st.db).await.map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let alerts = if me.is_admin { st.disk.alert().into_iter().chain(st.relay.quota.alert()).collect() } else { Vec::new() };
    Ok(WsServerMsg::Ready {
        me,
        users: users.into_iter().map(|(id, nickname, is_admin, avatar_icon, avatar_color)| User { id, nickname, is_admin, avatar_icon, avatar_color }).collect(),
        channels,
        online: st.hub.online_ids(),
        alerts,
        voice: st.voice.members(),
        streams: st.voice.streams(),
        music: st.music.states(crate::now_ms()),
        music_bot: crate::music::wire::bot_id(st).await,
    })
}

async fn handle(st: AppState, mut socket: WebSocket) {
    let Some((token, protocol)) = read_hello(&mut socket).await else {
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
    let reg = st.hub.register(user_id, au.user.is_admin);
    let (conn_id, mut rx, mut kill, first) = (reg.id, reg.rx, reg.kill, reg.first);
    let ready = match snapshot(&st, au.user).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("snapshot failed: {e:#}");
            st.hub.unregister(conn_id);
            return close(socket, 1011, "server error").await;
        }
    };
    if send(&mut socket, &ready).await {
        if first {
            st.hub.broadcast_except(user_id, WsServerMsg::Presence { user_id, online: true });
        }
        run(&st, &mut socket, &mut rx, &mut kill, user_id, is_admin, conn_id).await;
    }
    crate::voice::conn_closed(&st, conn_id);
    if let Some(uid) = st.hub.unregister(conn_id) {
        st.hub.broadcast(WsServerMsg::Presence { user_id: uid, online: false });
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
                        Ok(WsClientMsg::Ping) => if !send(socket, &WsServerMsg::Pong).await { return },
                        Ok(WsClientMsg::VoiceJoin { channel_id, pubkey }) => crate::voice::join(st, user_id, conn_id, channel_id, pubkey).await,
                        Ok(WsClientMsg::E2eeKey { to, scope, generation, envelope }) => crate::voice::forward_key(st, user_id, to, scope, generation, envelope),
                        Ok(WsClientMsg::E2eeKeyRequest { from, scope }) => crate::voice::forward_key_request(st, user_id, from, scope),
                        Ok(WsClientMsg::VoiceLeave) => crate::voice::leave(st, user_id),
                        Ok(WsClientMsg::VoiceState { muted, deafened }) => crate::voice::set_state(st, user_id, muted, deafened),
                        Ok(WsClientMsg::VoiceKick { user_id: target }) => crate::voice::kick(st, user_id, is_admin, target).await,
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
