//! Who is in which voice channel, with each member's relay credentials and traffic counters.
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Instant,
};

use rynuvo_proto::{Id, VoiceMember, voice_wire::VoiceKey};

/// Most people allowed in one voice channel.
pub const MAX_PER_CHANNEL: usize = 10;

/// Where the relay last heard from a member, which is also where it sends to them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    None,
    Udp(SocketAddr, Instant),
    Ws(Instant),
}

#[derive(Debug, Clone)]
pub struct Member {
    pub user_id: Id,
    pub conn_id: u64,
    pub channel_id: Id,
    pub session: u32,
    pub key: VoiceKey,
    pub muted: bool,
    pub deafened: bool,
    pub joined_at: i64,
    pub route: Route,
    /// Next sequence number for packets sent to this member.
    pub down_seq: u32,
    pub bytes_in: u64,
    pub bytes_out: u64,
}

impl Member {
    pub fn public(&self) -> VoiceMember {
        VoiceMember { user_id: self.user_id, channel_id: self.channel_id, muted: self.muted, deafened: self.deafened }
    }
}

#[derive(Debug)]
pub enum JoinError {
    Full,
}

pub struct Joined {
    pub member: Member,
    /// The user's previous membership, if this join moved them.
    pub previous: Option<Member>,
}

#[derive(Clone, Default)]
pub struct VoiceRooms {
    inner: Arc<Mutex<Vec<Member>>>,
}

fn new_session(taken: &[Member]) -> u32 {
    loop {
        let mut b = [0u8; 4];
        getrandom::fill(&mut b).expect("system randomness");
        let s = u32::from_be_bytes(b);
        if s != 0 && !taken.iter().any(|m| m.session == s) {
            return s;
        }
    }
}

impl VoiceRooms {
    pub fn join(&self, user_id: Id, conn_id: u64, channel_id: Id, now_ms: i64) -> Result<Joined, JoinError> {
        let mut g = self.inner.lock().unwrap();
        let in_channel = g.iter().filter(|m| m.channel_id == channel_id && m.user_id != user_id).count();
        if in_channel >= MAX_PER_CHANNEL {
            return Err(JoinError::Full);
        }
        let previous = g.iter().position(|m| m.user_id == user_id).map(|i| g.remove(i));
        let (muted, deafened) = previous.as_ref().map_or((false, false), |p| (p.muted, p.deafened));
        let member = Member {
            user_id,
            conn_id,
            channel_id,
            session: new_session(&g),
            key: VoiceKey::random(),
            muted,
            deafened,
            joined_at: now_ms,
            route: Route::None,
            down_seq: 0,
            bytes_in: 0,
            bytes_out: 0,
        };
        g.push(member.clone());
        Ok(Joined { member, previous })
    }

    pub fn leave_user(&self, user_id: Id) -> Option<Member> {
        let mut g = self.inner.lock().unwrap();
        g.iter().position(|m| m.user_id == user_id).map(|i| g.remove(i))
    }

    pub fn leave_conn(&self, conn_id: u64) -> Option<Member> {
        let mut g = self.inner.lock().unwrap();
        g.iter().position(|m| m.conn_id == conn_id).map(|i| g.remove(i))
    }

    pub fn set_state(&self, user_id: Id, muted: bool, deafened: bool) -> Option<VoiceMember> {
        let mut g = self.inner.lock().unwrap();
        let m = g.iter_mut().find(|m| m.user_id == user_id)?;
        m.muted = muted || deafened;
        m.deafened = deafened;
        Some(m.public())
    }

    pub fn members(&self) -> Vec<VoiceMember> {
        self.inner.lock().unwrap().iter().map(Member::public).collect()
    }

    pub fn member(&self, user_id: Id) -> Option<Member> {
        self.inner.lock().unwrap().iter().find(|m| m.user_id == user_id).cloned()
    }

    pub fn end_channel(&self, channel_id: Id) -> Vec<Member> {
        let mut g = self.inner.lock().unwrap();
        let (gone, keep): (Vec<_>, Vec<_>) = g.drain(..).partition(|m| m.channel_id == channel_id);
        *g = keep;
        gone
    }

    /// Run `f` with the member list locked (used by the relay hot path).
    pub fn with<R>(&self, f: impl FnOnce(&mut Vec<Member>) -> R) -> R {
        f(&mut self.inner.lock().unwrap())
    }
}
