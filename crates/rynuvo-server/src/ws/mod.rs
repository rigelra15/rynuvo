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

pub fn router() -> Router<AppState> {
    Router::new().route("/ws", get(upgrade))
}

async fn upgrade(State(st): State<AppState>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| handle(st, socket))
}

async fn close(mut socket: WebSocket, code: u16, reason: &'static str) {
    let _ = socket.send(Message::Close(Some(CloseFrame { code, reason: reason.into() }))).await;
}

async fn send(socket: &mut WebSocket, msg: &WsServerMsg) -> bool {
    let txt = serde_json::to_string(msg).expect("serialize");
    socket.send(Message::Text(txt.into())).await.is_ok()
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
    let users: Vec<(i64, String, bool)> =
        sqlx::query_as("SELECT id, nickname, is_admin FROM users WHERE removed_at IS NULL ORDER BY id").fetch_all(&st.db).await?;
    let channels = list_channels(&st.db).await.map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let alerts = if me.is_admin { st.disk.alert().into_iter().collect() } else { Vec::new() };
    Ok(WsServerMsg::Ready {
        me,
        users: users.into_iter().map(|(id, nickname, is_admin)| User { id, nickname, is_admin }).collect(),
        channels,
        online: st.hub.online_ids(),
        alerts,
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
    let (conn_id, mut rx, first) = st.hub.register(user_id, au.user.is_admin);
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
        run(&st, &mut socket, &mut rx, user_id).await;
    }
    if let Some(uid) = st.hub.unregister(conn_id) {
        st.hub.broadcast(WsServerMsg::Presence { user_id: uid, online: false });
    }
}

async fn run(st: &AppState, socket: &mut WebSocket, rx: &mut tokio::sync::mpsc::UnboundedReceiver<Outgoing>, user_id: i64) {
    let mut ping = tokio::time::interval(PING_EVERY);
    ping.tick().await;
    let mut last_rx = tokio::time::Instant::now();
    loop {
        tokio::select! {
            out = rx.recv() => match out {
                Some(Outgoing::Msg(m)) => if !send(socket, &m).await { return },
                Some(Outgoing::Close(code, reason)) => {
                    let _ = socket.send(Message::Close(Some(CloseFrame { code, reason: reason.into() }))).await;
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
                        _ => {}
                    },
                    Message::Close(_) => return,
                    _ => {}
                }
            }
            _ = ping.tick() => {
                if last_rx.elapsed() > IDLE_TIMEOUT { return }
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() { return }
            }
        }
    }
}
