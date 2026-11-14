use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use rynuvo_proto::{CLOSE_AUTH_REVOKED, WsServerMsg};
use tokio::sync::mpsc;

/// What the hub asks a connection task to do.
#[derive(Debug, Clone)]
pub enum Outgoing {
    Msg(WsServerMsg),
    Close(u16, &'static str),
}

struct Conn {
    id: u64,
    user_id: i64,
    is_admin: bool,
    tx: mpsc::UnboundedSender<Outgoing>,
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
    /// Register a connection. Returns (conn id, receiver, whether this is the user's first connection).
    pub fn register(&self, user_id: i64, is_admin: bool) -> (u64, mpsc::UnboundedReceiver<Outgoing>, bool) {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut g = self.inner.lock().unwrap();
        let first = !g.conns.iter().any(|c| c.user_id == user_id);
        g.next_id += 1;
        let id = g.next_id;
        g.conns.push(Conn { id, user_id, is_admin, tx });
        (id, rx, first)
    }

    /// Remove a connection. Returns the user id if that was the user's last connection.
    pub fn unregister(&self, conn_id: u64) -> Option<i64> {
        let mut g = self.inner.lock().unwrap();
        let pos = g.conns.iter().position(|c| c.id == conn_id)?;
        let c = g.conns.remove(pos);
        (!g.conns.iter().any(|o| o.user_id == c.user_id)).then_some(c.user_id)
    }

    fn send_where(&self, out: Outgoing, pred: impl Fn(&Conn) -> bool) {
        let g = self.inner.lock().unwrap();
        for c in g.conns.iter().filter(|c| pred(c)) {
            let _ = c.tx.send(out.clone());
        }
    }

    pub fn broadcast(&self, msg: WsServerMsg) {
        self.send_where(Outgoing::Msg(msg), |_| true);
    }

    pub fn broadcast_except(&self, user_id: i64, msg: WsServerMsg) {
        self.send_where(Outgoing::Msg(msg), |c| c.user_id != user_id);
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
