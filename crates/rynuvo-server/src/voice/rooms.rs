//! Who is in which voice channel, with each member's relay credentials and traffic counters.
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Instant,
};

use rynuvo_proto::{Id, StreamInfo, VoiceMember, e2ee::Scope, voice_wire::VoiceKey};

/// Most people allowed in one voice channel.
pub const MAX_PER_CHANNEL: usize = 10;
/// Most viewers of one stream (the streamer's upload is multiplied by this).
pub const MAX_VIEWERS: usize = 3;

#[derive(Debug, Clone)]
pub struct Viewer {
    pub user_id: Id,
    /// Key for this pair's direct (P2P) packets.
    pub key: String,
    pub started_at: i64,
    pub relay_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct Stream {
    pub codecs: Vec<String>,
    pub preset: String,
    pub viewers: Vec<Viewer>,
}

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
    /// When the call in this channel began (shared by everyone in it).
    pub call_since: i64,
    pub route: Route,
    /// Next sequence number for packets sent to this member.
    pub down_seq: u32,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub stream: Option<Stream>,
    /// The device's public key (base64), used by others to seal media keys to it.
    pub pubkey: String,
}

impl Member {
    pub fn stream_info(&self) -> Option<StreamInfo> {
        self.stream.as_ref().map(|s| StreamInfo {
            user_id: self.user_id,
            channel_id: self.channel_id,
            codecs: s.codecs.clone(),
            preset: s.preset.clone(),
        })
    }

    pub fn public(&self) -> VoiceMember {
        VoiceMember { user_id: self.user_id, channel_id: self.channel_id, muted: self.muted, deafened: self.deafened, joined_at: self.joined_at, call_since: self.call_since, pubkey: self.pubkey.clone() }
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

fn in_scope(g: &[Member], a: Id, b: Id, scope: &Scope) -> bool {
    match *scope {
        Scope::Voice(ch) => [a, b].iter().all(|u| g.iter().any(|m| m.user_id == *u && m.channel_id == ch)),
        Scope::Stream(streamer) => {
            let other = if a == streamer { b } else if b == streamer { a } else { return false };
            other != streamer && g.iter().any(|m| m.user_id == streamer && m.stream.as_ref().is_some_and(|s| s.viewers.iter().any(|v| v.user_id == other)))
        }
    }
}

impl VoiceRooms {
    pub fn join(&self, user_id: Id, conn_id: u64, channel_id: Id, pubkey: String, now_ms: i64) -> Result<Joined, JoinError> {
        let mut g = self.inner.lock().unwrap();
        let in_channel = g.iter().filter(|m| m.channel_id == channel_id && m.user_id != user_id).count();
        if in_channel >= MAX_PER_CHANNEL {
            return Err(JoinError::Full);
        }
        let previous = g.iter().position(|m| m.user_id == user_id).map(|i| g.remove(i));
        let (muted, deafened) = previous.as_ref().map_or((false, false), |p| (p.muted, p.deafened));
        let call_since = g.iter().filter(|m| m.channel_id == channel_id).map(|m| m.call_since).min().unwrap_or(now_ms);
        let member = Member {
            user_id,
            conn_id,
            channel_id,
            session: new_session(&g),
            key: VoiceKey::random(),
            muted,
            deafened,
            joined_at: now_ms,
            call_since,
            route: Route::None,
            down_seq: 0,
            bytes_in: 0,
            bytes_out: 0,
            stream: None,
            pubkey,
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

    pub fn start_stream(&self, user_id: Id, codecs: Vec<String>, preset: String) -> Option<StreamInfo> {
        let mut g = self.inner.lock().unwrap();
        let m = g.iter_mut().find(|m| m.user_id == user_id)?;
        match &mut m.stream {
            Some(s) => {
                s.codecs = codecs;
                s.preset = preset;
            }
            None => m.stream = Some(Stream { codecs, preset, viewers: Vec::new() }),
        }
        m.stream_info()
    }

    pub fn stop_stream(&self, user_id: Id) -> Option<(StreamInfo, Vec<Viewer>)> {
        let mut g = self.inner.lock().unwrap();
        let m = g.iter_mut().find(|m| m.user_id == user_id)?;
        let info = m.stream_info()?;
        let s = m.stream.take()?;
        Some((info, s.viewers))
    }

    pub fn streams(&self) -> Vec<StreamInfo> {
        self.inner.lock().unwrap().iter().filter_map(Member::stream_info).collect()
    }

    /// Add `viewer` to `streamer`'s stream. Returns the pair key and both connections.
    pub fn watch(&self, viewer: Id, streamer: Id, now_ms: i64) -> Option<(String, u64, u64)> {
        let mut g = self.inner.lock().unwrap();
        if viewer == streamer {
            return None;
        }
        let v = g.iter().find(|m| m.user_id == viewer)?;
        let (v_conn, v_channel) = (v.conn_id, v.channel_id);
        let s = g.iter_mut().find(|m| m.user_id == streamer && m.channel_id == v_channel)?;
        let s_conn = s.conn_id;
        let st = s.stream.as_mut()?;
        if let Some(existing) = st.viewers.iter().find(|x| x.user_id == viewer) {
            return Some((existing.key.clone(), s_conn, v_conn));
        }
        if st.viewers.len() >= MAX_VIEWERS {
            return None;
        }
        let key = VoiceKey::random().to_b64();
        st.viewers.push(Viewer { user_id: viewer, key: key.clone(), started_at: now_ms, relay_bytes: 0 });
        Some((key, s_conn, v_conn))
    }

    /// Remove `viewer` from `streamer`'s stream. Returns the viewer entry and the streamer's connection.
    pub fn unwatch(&self, viewer: Id, streamer: Id) -> Option<(Viewer, u64)> {
        let mut g = self.inner.lock().unwrap();
        let s = g.iter_mut().find(|m| m.user_id == streamer)?;
        let st = s.stream.as_mut()?;
        let i = st.viewers.iter().position(|x| x.user_id == viewer)?;
        Some((st.viewers.remove(i), s.conn_id))
    }

    /// `viewer` is gone: remove them from every stream. Returns (streamer, streamer conn, entry).
    pub fn drop_viewer(&self, viewer: Id) -> Vec<(Id, u64, Viewer)> {
        let mut g = self.inner.lock().unwrap();
        let mut out = Vec::new();
        for m in g.iter_mut() {
            if let Some(st) = m.stream.as_mut() {
                if let Some(i) = st.viewers.iter().position(|x| x.user_id == viewer) {
                    out.push((m.user_id, m.conn_id, st.viewers.remove(i)));
                }
            }
        }
        out
    }

    /// The connection of `to` if `from` and `to` are a streamer–viewer pair (either direction).
    pub fn pair_conn(&self, from: Id, to: Id) -> Option<u64> {
        let g = self.inner.lock().unwrap();
        let watches = |streamer: Id, viewer: Id| {
            g.iter().any(|m| m.user_id == streamer && m.stream.as_ref().is_some_and(|s| s.viewers.iter().any(|v| v.user_id == viewer)))
        };
        if watches(from, to) || watches(to, from) { g.iter().find(|m| m.user_id == to).map(|m| m.conn_id) } else { None }
    }

    /// Whether `a` and `b` both belong to `scope`: members of the same voice channel, or a
    /// streamer and one of their viewers.
    pub fn shares_scope(&self, a: Id, b: Id, scope: &Scope) -> bool {
        in_scope(&self.inner.lock().unwrap(), a, b, scope)
    }

    /// The connection of `to`, if `sender` is speaking from `sender_conn` (their current voice
    /// connection) and shares `scope` with `to`. Checked and looked up under one lock.
    pub fn scope_conn(&self, sender: Id, sender_conn: u64, to: Id, scope: &Scope) -> Option<u64> {
        let g = self.inner.lock().unwrap();
        if sender == to || !g.iter().any(|m| m.user_id == sender && m.conn_id == sender_conn) || !in_scope(&g, sender, to, scope) {
            return None;
        }
        g.iter().find(|m| m.user_id == to).map(|m| m.conn_id)
    }

    /// Run `f` with the member list locked (used by the relay hot path).
    pub fn with<R>(&self, f: impl FnOnce(&mut Vec<Member>) -> R) -> R {
        f(&mut self.inner.lock().unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_call_keeps_its_start_while_anyone_stays() {
        let r = VoiceRooms::default();
        let a = r.join(1, 1, 7, String::new(), 1_000).unwrap().member;
        assert_eq!(a.call_since, 1_000, "the first one in starts the call");
        let b = r.join(2, 2, 7, String::new(), 5_000).unwrap().member;
        assert_eq!(b.call_since, 1_000, "later joiners share the call's start");
        r.leave_user(1);
        let c = r.join(3, 3, 7, String::new(), 9_000).unwrap().member;
        assert_eq!(c.call_since, 1_000, "the first one leaving doesn't restart it");
        r.leave_user(2);
        r.leave_user(3);
        let d = r.join(4, 4, 7, String::new(), 20_000).unwrap().member;
        assert_eq!(d.call_since, 20_000, "an empty channel starts a new call");
    }
}
