//! Realtime session: WebSocket with Hello/Ready, reconnect with backoff, catch-up of missed messages.
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use futures_util::{SinkExt, StreamExt};
use rynuvo_proto::{CLOSE_AUTH_REVOKED, CLOSE_PROTOCOL_OLD, Id, PROTOCOL_VERSION, WsClientMsg, WsServerMsg};
use serde::Serialize;
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::Message as WsMsg;

use super::{http::Api, identity::SecretStore};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum ConnState {
    Connecting,
    Online,
    #[serde(rename_all = "camelCase")]
    Reconnecting { in_ms: u64 },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "camelCase")]
pub enum SessionEvent {
    Server(WsServerMsg),
    Conn(ConnState),
    Revoked,
    UpdateRequired,
}

pub type Emit = Arc<dyn Fn(SessionEvent) + Send + Sync>;

/// Reconnect delay: min(500·2^attempt, 15000) ms with ±20% jitter.
pub fn backoff_ms(attempt: u32) -> u64 {
    let base = 500u64.saturating_mul(1u64 << attempt.min(16)).min(15_000);
    let mut r = [0u8; 2];
    let _ = getrandom::fill(&mut r);
    let unit = u16::from_le_bytes(r) as f64 / u16::MAX as f64; // 0..1
    (base as f64 * (0.8 + 0.4 * unit)) as u64
}

const PING_EVERY: Duration = Duration::from_secs(20);
const READ_TIMEOUT: Duration = Duration::from_secs(45);

#[derive(Default)]
struct Shared {
    /// channel id -> highest message id the UI has seen
    last_seen: HashMap<Id, Id>,
}

/// Handle to a running session.
#[derive(Clone)]
pub struct SessionHandle {
    shared: Arc<Mutex<Shared>>,
    out: mpsc::UnboundedSender<WsClientMsg>,
    stop: watch::Sender<bool>,
}

impl SessionHandle {
    /// The UI loaded `channel` up to `last_id`; future gaps in it are caught up on reconnect.
    pub fn mark_loaded(&self, channel: Id, last_id: Id) {
        let mut g = self.shared.lock().unwrap();
        let e = g.last_seen.entry(channel).or_insert(last_id);
        *e = (*e).max(last_id);
    }

    pub fn send_typing(&self, channel_id: Id) {
        let _ = self.out.send(WsClientMsg::Typing { channel_id });
    }

    pub fn stop(&self) {
        let _ = self.stop.send(true);
    }
}

pub struct Session;

enum End {
    Retry,
    Stop,
    Revoked,
    UpdateRequired,
}

impl Session {
    pub fn start(api: Api, store: Arc<dyn SecretStore>, server: String, emit: Emit) -> SessionHandle {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let (out_tx, out_rx) = mpsc::unbounded_channel();
        let (stop_tx, stop_rx) = watch::channel(false);
        let h = SessionHandle { shared: shared.clone(), out: out_tx, stop: stop_tx };
        tokio::spawn(run(api, store, server, emit, shared, out_rx, stop_rx));
        h
    }
}

async fn run(
    api: Api,
    store: Arc<dyn SecretStore>,
    server: String,
    emit: Emit,
    shared: Arc<Mutex<Shared>>,
    mut out_rx: mpsc::UnboundedReceiver<WsClientMsg>,
    mut stop: watch::Receiver<bool>,
) {
    let mut attempt = 0u32;
    loop {
        if *stop.borrow() {
            return;
        }
        emit(SessionEvent::Conn(ConnState::Connecting));
        let end = connect_once(&api, &emit, &shared, &mut out_rx, &mut stop, &mut attempt)
            .await
            .unwrap_or_else(|e| {
                tracing::debug!("ws connect failed: {e}");
                End::Retry
            });
        match end {
            End::Stop => return,
            End::Revoked => {
                store.clear(&server);
                emit(SessionEvent::Revoked);
                return;
            }
            End::UpdateRequired => {
                emit(SessionEvent::UpdateRequired);
                return;
            }
            End::Retry => {}
        }
        let wait = backoff_ms(attempt);
        attempt = attempt.saturating_add(1);
        emit(SessionEvent::Conn(ConnState::Reconnecting { in_ms: wait }));
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(wait)) => {}
            _ = stop.changed() => return,
        }
    }
}

async fn connect_once(
    api: &Api,
    emit: &Emit,
    shared: &Arc<Mutex<Shared>>,
    out_rx: &mut mpsc::UnboundedReceiver<WsClientMsg>,
    stop: &mut watch::Receiver<bool>,
    attempt: &mut u32,
) -> Result<End, String> {
    let token = api.token().ok_or("no token")?.to_string();
    let (mut ws, _) = tokio::time::timeout(Duration::from_secs(10), tokio_tungstenite::connect_async(api.ws_url()))
        .await
        .map_err(|_| "connect timeout")?
        .map_err(|e| e.to_string())?;
    let hello = serde_json::to_string(&WsClientMsg::Hello { token, protocol: PROTOCOL_VERSION }).unwrap();
    ws.send(WsMsg::text(hello)).await.map_err(|e| e.to_string())?;

    let mut ready_seen = false;
    let mut ping = tokio::time::interval(PING_EVERY);
    ping.tick().await;
    loop {
        tokio::select! {
            _ = stop.changed() => {
                let _ = ws.close(None).await;
                return Ok(End::Stop);
            }
            out = out_rx.recv() => {
                if let Some(m) = out {
                    if ready_seen {
                        let _ = ws.send(WsMsg::text(serde_json::to_string(&m).unwrap())).await;
                    }
                }
            }
            _ = ping.tick() => {
                if ws.send(WsMsg::text(serde_json::to_string(&WsClientMsg::Ping).unwrap())).await.is_err() {
                    return Ok(End::Retry);
                }
            }
            msg = tokio::time::timeout(READ_TIMEOUT, ws.next()) => {
                let msg = match msg {
                    Err(_) => return Ok(End::Retry),
                    Ok(None) => return Ok(End::Retry),
                    Ok(Some(Err(_))) => return Ok(End::Retry),
                    Ok(Some(Ok(m))) => m,
                };
                match msg {
                    WsMsg::Text(t) => {
                        let Ok(m) = serde_json::from_str::<WsServerMsg>(t.as_str()) else { continue };
                        match m {
                            WsServerMsg::Ready { .. } => {
                                ready_seen = true;
                                *attempt = 0;
                                emit(SessionEvent::Server(m));
                                emit(SessionEvent::Conn(ConnState::Online));
                                catch_up(api, emit, shared).await;
                            }
                            WsServerMsg::MessageCreated(ref msg) => {
                                if advance(shared, msg.channel_id, msg.id) {
                                    emit(SessionEvent::Server(m));
                                }
                            }
                            WsServerMsg::Pong => {}
                            other => emit(SessionEvent::Server(other)),
                        }
                    }
                    WsMsg::Close(f) => {
                        let code = f.map(|f| u16::from(f.code)).unwrap_or(1005);
                        return Ok(match code {
                            CLOSE_AUTH_REVOKED => End::Revoked,
                            CLOSE_PROTOCOL_OLD => End::UpdateRequired,
                            _ => End::Retry,
                        });
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Record `id` as seen in `channel`; true if it is new (or channel not tracked).
fn advance(shared: &Arc<Mutex<Shared>>, channel: Id, id: Id) -> bool {
    let mut g = shared.lock().unwrap();
    match g.last_seen.get_mut(&channel) {
        Some(last) if id <= *last => false,
        Some(last) => {
            *last = id;
            true
        }
        None => true,
    }
}

async fn catch_up(api: &Api, emit: &Emit, shared: &Arc<Mutex<Shared>>) {
    let tracked: Vec<(Id, Id)> = shared.lock().unwrap().last_seen.iter().map(|(c, l)| (*c, *l)).collect();
    for (channel, mut after) in tracked {
        loop {
            let batch = match api.messages(channel, None, Some(after), Some(100)).await {
                Ok(b) => b,
                Err(_) => break, // channel deleted or network: live events will continue
            };
            let n = batch.len();
            for m in batch {
                after = after.max(m.id);
                if advance(shared, channel, m.id) {
                    emit(SessionEvent::Server(WsServerMsg::MessageCreated(m)));
                }
            }
            if n < 100 {
                break;
            }
        }
    }
}
