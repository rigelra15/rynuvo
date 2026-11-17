use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use rynuvo_proto::{CLOSE_AUTH_REVOKED, WsServerMsg};
use tokio::sync::{mpsc, watch};

/// Messages a connection may have queued before it is considered stuck and cut off.
pub const QUEUE_DEPTH: usize = 256;
/// Queue slots media packets may not use, so chat events still fit behind a media backlog.
const MEDIA_RESERVE: usize = 64;

/// What the hub asks a connection task to do.
#[derive(Debug, Clone)]
pub enum Outgoing {
    Msg(WsServerMsg),
    /// Voice packet for a client on the WebSocket path.
    Bin(Vec<u8>),
    Close(u16, &'static str),
}

struct Conn {
    id: u64,
    user_id: i64,
    is_admin: bool,
    mobile: bool,
    tx: mpsc::Sender<Outgoing>,
    kill: watch::Sender<bool>,
}

/// How a user shows to others: offline, or online and whether that is only from phones/tablets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Offline,
    Online { mobile: bool },
}

/// A registered connection: its queue, a flag raised when it must close, and the user's status
/// before and after it joined.
pub struct Registered {
    pub id: u64,
    pub rx: mpsc::Receiver<Outgoing>,
    pub kill: watch::Receiver<bool>,
    pub before: Status,
    pub after: Status,
}

fn status_of(conns: &[Conn], user_id: i64) -> Status {
    let mut it = conns.iter().filter(|c| c.user_id == user_id).peekable();
    if it.peek().is_none() {
        return Status::Offline;
    }
    Status::Online { mobile: it.all(|c| c.mobile) }
}

#[derive(Default)]
struct Inner {
    next_id: u64,
    conns: Vec<Conn>,
    typing: HashMap<(i64, i64), Instant>,
}

/// Realtime fan-out hub shared by all WebSocket connections.
#[derive(Clone, Default)]
pub struct Hub {
    inner: Arc<Mutex<Inner>>,
}

pub const TYPING_INTERVAL: Duration = Duration::from_secs(3);

impl Hub {
    /// Register a connection.
    pub fn register(&self, user_id: i64, is_admin: bool, mobile: bool) -> Registered {
        let (tx, rx) = mpsc::channel(QUEUE_DEPTH);
        let (kill_tx, kill) = watch::channel(false);
        let mut g = self.inner.lock().unwrap();
        let before = status_of(&g.conns, user_id);
        g.next_id += 1;
        let id = g.next_id;
        g.conns.push(Conn { id, user_id, is_admin, mobile, tx, kill: kill_tx });
        let after = status_of(&g.conns, user_id);
        Registered { id, rx, kill, before, after }
    }

    /// Remove a connection. Returns its user and that user's status before and after.
    pub fn unregister(&self, conn_id: u64) -> Option<(i64, Status, Status)> {
        let mut g = self.inner.lock().unwrap();
        let pos = g.conns.iter().position(|c| c.id == conn_id)?;
        let before = status_of(&g.conns, g.conns[pos].user_id);
        let c = g.conns.remove(pos);
        Some((c.user_id, before, status_of(&g.conns, c.user_id)))
    }

    /// Users online only from phones or tablets.
    pub fn mobile_ids(&self) -> Vec<i64> {
        let g = self.inner.lock().unwrap();
        let mut ids: Vec<i64> = g.conns.iter().map(|c| c.user_id).filter(|&u| status_of(&g.conns, u) == Status::Online { mobile: true }).collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    fn send_where(&self, out: Outgoing, pred: impl Fn(&Conn) -> bool) {
        let g = self.inner.lock().unwrap();
        for c in g.conns.iter().filter(|c| pred(c)) {
            if matches!(out, Outgoing::Bin(_)) && c.tx.capacity() <= MEDIA_RESERVE {
                // Real-time media is worthless late: drop it rather than disconnect a slow link.
                continue;
            }
            if let Err(mpsc::error::TrySendError::Full(_)) = c.tx.try_send(out.clone()) {
                tracing::warn!(conn = c.id, "connection is not reading; closing it");
                let _ = c.kill.send(true);
            }
        }
    }

    /// Queue a message for one connection.
    pub fn send_to_conn(&self, conn_id: u64, out: Outgoing) {
        self.send_where(out, |c| c.id == conn_id);
    }

    pub fn broadcast(&self, msg: WsServerMsg) {
        self.send_where(Outgoing::Msg(msg), |_| true);
    }

    pub fn broadcast_except(&self, user_id: i64, msg: WsServerMsg) {
        self.send_where(Outgoing::Msg(msg), |c| c.user_id != user_id);
    }

    /// To my other connections (another device of mine), not the one that caused it.
    pub fn send_to_user_except(&self, user_id: i64, conn_id: u64, msg: WsServerMsg) {
        self.send_where(Outgoing::Msg(msg), |c| c.user_id == user_id && c.id != conn_id);
    }

    pub fn send_to_admins(&self, msg: WsServerMsg) {
        self.send_where(Outgoing::Msg(msg), |c| c.is_admin);
    }

    /// Close every connection of `user_id` with 4001.
    pub fn kick_user(&self, user_id: i64) {
        self.send_where(Outgoing::Close(CLOSE_AUTH_REVOKED, "removed"), |c| c.user_id == user_id);
    }

    /// Close every connection (server restart semantics, code 1012).
    pub fn disconnect_all(&self) {
        self.send_where(Outgoing::Close(1012, "restart"), |_| true);
    }

    pub fn online_ids(&self) -> Vec<i64> {
        let g = self.inner.lock().unwrap();
        let mut ids: Vec<i64> = g.conns.iter().map(|c| c.user_id).collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// Rate limit: true if this typing event should be relayed.
    pub fn allow_typing(&self, user_id: i64, channel_id: i64) -> bool {
        let mut g = self.inner.lock().unwrap();
        let now = Instant::now();
        match g.typing.get(&(user_id, channel_id)) {
            Some(t) if now.duration_since(*t) < TYPING_INTERVAL => false,
            _ => {
                g.typing.retain(|_, t| now.duration_since(*t) < TYPING_INTERVAL);
                g.typing.insert((user_id, channel_id), now);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_backlog_drops_packets_but_keeps_connection() {
        let hub = Hub::default();
        let mut r = hub.register(1, false, false);
        for _ in 0..QUEUE_DEPTH * 2 {
            hub.send_to_conn(r.id, Outgoing::Bin(vec![0; 10]));
        }
        assert!(!*r.kill.borrow(), "a slow reader of media must not be disconnected");
        hub.send_to_conn(r.id, Outgoing::Msg(WsServerMsg::Pong));
        assert!(!*r.kill.borrow(), "chat messages still fit behind the media backlog");
        let mut last = None;
        while let Ok(o) = r.rx.try_recv() {
            last = Some(o);
        }
        assert!(matches!(last, Some(Outgoing::Msg(WsServerMsg::Pong))));
    }

    #[test]
    fn mobile_only_while_no_computer_is_connected() {
        let hub = Hub::default();
        let phone = hub.register(1, false, true);
        assert_eq!((phone.before, phone.after), (Status::Offline, Status::Online { mobile: true }));
        assert_eq!(hub.mobile_ids(), vec![1]);
        let mac = hub.register(1, false, false);
        assert_eq!(mac.after, Status::Online { mobile: false });
        assert!(hub.mobile_ids().is_empty());
        assert_eq!(hub.unregister(mac.id), Some((1, Status::Online { mobile: false }, Status::Online { mobile: true })));
        assert_eq!(hub.unregister(phone.id), Some((1, Status::Online { mobile: true }, Status::Offline)));
    }

    #[test]
    fn slow_consumer_is_cut_off() {
        let hub = Hub::default();
        let mut r = hub.register(1, false, false);
        for _ in 0..(QUEUE_DEPTH + 50) {
            hub.broadcast(WsServerMsg::Pong);
        }
        assert!(*r.kill.borrow(), "connection asked to close once its queue is full");
        let mut n = 0;
        while r.rx.try_recv().is_ok() {
            n += 1;
        }
        assert!(n <= QUEUE_DEPTH, "queue bounded, got {n}");
    }
}
