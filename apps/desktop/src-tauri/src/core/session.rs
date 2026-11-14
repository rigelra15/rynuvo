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

/// Keepalive cadence and how long silence from the server is tolerated before reconnecting.
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    pub ping_every: Duration,
    pub read_timeout: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing { ping_every: Duration::from_secs(20), read_timeout: Duration::from_secs(45) }
    }
}

/// Recent ids remembered per channel for de-duplication.
const RECENT_IDS: usize = 512;

#[derive(Default)]
struct Shared {
    /// channel id -> highest message id seen (the `after=` cursor for catch-up)
    last_seen: HashMap<Id, Id>,
    /// channel id -> every id at or below this was in a page the UI loaded
    floor: HashMap<Id, Id>,
    /// channel id -> recently delivered ids, so out-of-order broadcasts are not mistaken for duplicates
    recent: HashMap<Id, std::collections::VecDeque<Id>>,
}

impl Shared {
    fn track(&mut self, channel: Id, loaded_up_to: Id) {
        let f = self.floor.entry(channel).or_insert(loaded_up_to);
        *f = (*f).max(loaded_up_to);
        let l = self.last_seen.entry(channel).or_insert(loaded_up_to);
        *l = (*l).max(loaded_up_to);
    }

    fn advance(&mut self, channel: Id, id: Id) -> bool {
        let Some(&floor) = self.floor.get(&channel) else { return true };
        if id <= floor {
            return false;
        }
        let recent = self.recent.entry(channel).or_default();
        if recent.contains(&id) {
            return false;
        }
        recent.push_back(id);
        if recent.len() > RECENT_IDS {
            recent.pop_front();
        }
        let l = self.last_seen.entry(channel).or_insert(id);
        *l = (*l).max(id);
        true
    }
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
        self.shared.lock().unwrap().track(channel, last_id);
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
        Self::start_with(api, store, server, emit, Timing::default())
    }

    pub fn start_with(api: Api, store: Arc<dyn SecretStore>, server: String, emit: Emit, timing: Timing) -> SessionHandle {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let (out_tx, out_rx) = mpsc::unbounded_channel();
        let (stop_tx, stop_rx) = watch::channel(false);
        let h = SessionHandle { shared: shared.clone(), out: out_tx, stop: stop_tx };
        tokio::spawn(run(api, store, server, emit, shared, out_rx, stop_rx, timing));
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
    timing: Timing,
) {
    let mut attempt = 0u32;
    loop {
        if *stop.borrow() {
            return;
        }
        emit(SessionEvent::Conn(ConnState::Connecting));
        let end = connect_once(&api, &emit, &shared, &mut out_rx, &mut stop, &mut attempt, timing)
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
    timing: Timing,
) -> Result<End, String> {
    let token = api.token().ok_or("no token")?.to_string();
    let (mut ws, _) = tokio::time::timeout(Duration::from_secs(10), tokio_tungstenite::connect_async(api.ws_url()))
        .await
        .map_err(|_| "connect timeout")?
        .map_err(|e| e.to_string())?;
    let hello = serde_json::to_string(&WsClientMsg::Hello { token, protocol: PROTOCOL_VERSION }).unwrap();
    ws.send(WsMsg::text(hello)).await.map_err(|e| e.to_string())?;

    let mut ready_seen = false;
    let mut ping = tokio::time::interval(timing.ping_every);
    ping.tick().await;
    // Deadline is pushed forward only by frames from the server, so a half-open socket is detected.
    let deadline = tokio::time::sleep(timing.read_timeout);
    tokio::pin!(deadline);
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
            _ = &mut deadline => {
                tracing::info!("no data from server for {:?}, reconnecting", timing.read_timeout);
                return Ok(End::Retry);
            }
            msg = ws.next() => {
                let msg = match msg {
                    None | Some(Err(_)) => return Ok(End::Retry),
                    Some(Ok(m)) => m,
                };
                deadline.as_mut().reset(tokio::time::Instant::now() + timing.read_timeout);
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

/// Record `id` as delivered in `channel`; true if the UI has not seen it yet (or channel not tracked).
fn advance(shared: &Arc<Mutex<Shared>>, channel: Id, id: Id) -> bool {
    shared.lock().unwrap().advance(channel, id)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn out_of_order_messages_are_not_dropped() {
        let s = Arc::new(Mutex::new(Shared::default()));
        s.lock().unwrap().track(1, 0);
        assert!(advance(&s, 1, 11));
        assert!(advance(&s, 1, 10), "older id that was never seen must still be delivered");
        assert!(!advance(&s, 1, 10), "duplicate");
        assert!(!advance(&s, 1, 11), "duplicate");
        assert_eq!(s.lock().unwrap().last_seen[&1], 11, "cursor is the max id");
    }
}
