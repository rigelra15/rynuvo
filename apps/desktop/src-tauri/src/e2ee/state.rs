//! Pure media-key state for one engine run. Time is passed in so tests control it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rynuvo_proto::e2ee::{
    frame_generation, open_envelope, open_frame, seal_envelope, seal_frame, DeviceKey, MediaKey, PublicKey, Scope,
};
use rynuvo_proto::Id;

/// How long a peer's replaced key still opens frames that were in flight.
const PREVIOUS_KEY_TTL: Duration = Duration::from_secs(10);
/// Delay between sending a new key's envelopes and sealing with it.
const ACTIVATE_DELAY: Duration = Duration::from_millis(300);
/// How long a sender may go without a usable key before asking for one.
const REQUEST_AFTER: Duration = Duration::from_secs(1);
/// Minimum gap between two requests to the same sender.
const REQUEST_EVERY: Duration = Duration::from_secs(2);

/// An envelope to send to `to` over the WebSocket.
pub struct Outgoing {
    pub to: Id,
    pub scope: Scope,
    pub generation: u32,
    pub envelope: Vec<u8>,
}

/// One of my media keys. `next` is the next frame counter; `u32::MAX` means exhausted.
struct MyKey {
    key: MediaKey,
    generation: u32,
    next: u32,
}

impl MyKey {
    fn fresh(generation: u32) -> Self {
        MyKey { key: MediaKey::random(), generation, next: 0 }
    }
}

/// My keys for one scope: the one in use, and a newer one waiting to become active.
struct Mine {
    current: MyKey,
    pending: Option<(MyKey, Instant)>,
}

impl Mine {
    fn latest(&self) -> &MyKey {
        self.pending.as_ref().map(|(k, _)| k).unwrap_or(&self.current)
    }
}

/// What I hold for one sender in one scope.
#[derive(Default)]
struct Peer {
    current: Option<(u32, MediaKey)>,
    previous: Option<(u32, MediaKey, Instant)>,
    missing_since: Option<Instant>,
    last_request: Option<Instant>,
}

/// `a` is strictly newer than `b`, allowing for wrap-around of the u32 generation.
fn newer(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) > 0
}

fn random_generation() -> u32 {
    let k = MediaKey::random();
    u32::from_be_bytes([k.0[0], k.0[1], k.0[2], k.0[3]])
}

/// Media-key bookkeeping for one engine run. A new `KeyState` always starts with fresh keys.
/// Deliberately no `Debug`: it holds the device private key and media keys.
pub struct KeyState {
    me: Id,
    device: DeviceKey,
    pubkeys: HashMap<Id, PublicKey>,
    mine: HashMap<Scope, Mine>,
    peers: HashMap<(Scope, Id), Peer>,
}

pub type SharedKeys = Arc<Mutex<KeyState>>;

impl KeyState {
    pub fn new(me: Id, device: DeviceKey) -> Self {
        KeyState { me, device, pubkeys: HashMap::new(), mine: HashMap::new(), peers: HashMap::new() }
    }

    pub fn set_peer_pubkey(&mut self, user: Id, pk: PublicKey) {
        self.pubkeys.insert(user, pk);
    }

    fn envelopes(&self, scope: &Scope, key: &MyKey, to: &[Id]) -> Vec<Outgoing> {
        to.iter()
            .filter(|&&u| u != self.me)
            .filter_map(|&u| {
                let pk = self.pubkeys.get(&u)?;
                let envelope = seal_envelope(&self.device, self.me, u, pk, scope, key.generation, &key.key)?;
                Some(Outgoing { to: u, scope: scope.clone(), generation: key.generation, envelope })
            })
            .collect()
    }

    /// Send my newest key for `scope` to `to` (used for joiners), making one if I have none.
    pub fn share_current(&mut self, scope: &Scope, to: &[Id], _now: Instant) -> Vec<Outgoing> {
        self.mine
            .entry(scope.clone())
            .or_insert_with(|| Mine { current: MyKey::fresh(random_generation()), pending: None });
        self.envelopes(scope, self.mine[scope].latest(), to)
    }

    /// Make a new key for `scope`, send it to `to`; it is used for sealing from `now + 300ms`.
    pub fn rotate(&mut self, scope: &Scope, to: &[Id], now: Instant) -> Vec<Outgoing> {
        let Some(mine) = self.mine.get_mut(scope) else {
            return self.share_current(scope, to, now);
        };
        let key = MyKey::fresh(mine.latest().generation.wrapping_add(1));
        mine.pending = Some((key, now + ACTIVATE_DELAY));
        self.envelopes(scope, self.mine[scope].latest(), to)
    }

    /// Take a key `from` sent me. False if it can't be opened or isn't newer than what I hold.
    pub fn on_envelope(&mut self, from: Id, scope: &Scope, generation: u32, env: &[u8], now: Instant) -> bool {
        let Some(from_pub) = self.pubkeys.get(&from) else { return false };
        let Some(key) = open_envelope(&self.device, from, from_pub, self.me, scope, generation, env) else {
            return false;
        };
        let peer = self.peers.entry((scope.clone(), from)).or_default();
        if let Some((cur, _)) = &peer.current {
            if !newer(generation, *cur) {
                return false;
            }
        }
        if let Some((g, k)) = peer.current.take() {
            peer.previous = Some((g, k, now + PREVIOUS_KEY_TTL));
        }
        peer.current = Some((generation, key));
        peer.missing_since = None;
        true
    }

    /// Encrypt one frame with my active key. None if I have no key or its counter is used up.
    pub fn seal(&mut self, scope: &Scope, kind: u8, plain: &[u8], now: Instant) -> Option<Vec<u8>> {
        let mine = self.mine.get_mut(scope)?;
        if mine.pending.as_ref().is_some_and(|(_, at)| now >= *at) {
            mine.current = mine.pending.take()?.0;
        }
        let k = &mut mine.current;
        if k.next == u32::MAX {
            return None;
        }
        let ct = seal_frame(&k.key, k.generation, k.next, kind, self.me, plain);
        k.next += 1;
        Some(ct)
    }

    /// Decrypt a frame from `from`. None (frame dropped) if no usable key; then a key request becomes due.
    pub fn open(&mut self, scope: &Scope, from: Id, kind: u8, ct: &[u8], now: Instant) -> Option<Vec<u8>> {
        let generation = frame_generation(ct)?;
        let peer = self.peers.entry((scope.clone(), from)).or_default();
        if peer.previous.as_ref().is_some_and(|(_, _, until)| now >= *until) {
            peer.previous = None;
        }
        let key = match (&peer.current, &peer.previous) {
            (Some((g, k)), _) if *g == generation => Some(k),
            (_, Some((g, k, _))) if *g == generation => Some(k),
            _ => None,
        };
        let Some(key) = key else {
            peer.missing_since.get_or_insert(now);
            return None;
        };
        open_frame(key, kind, from, ct)
    }

    /// Senders I have had no usable key from for at least 1 s; each returned at most once per 2 s.
    pub fn due_requests(&mut self, now: Instant) -> Vec<(Scope, Id)> {
        let mut due = Vec::new();
        for ((scope, from), peer) in &mut self.peers {
            let Some(since) = peer.missing_since else { continue };
            if now.saturating_duration_since(since) < REQUEST_AFTER {
                continue;
            }
            if peer.last_request.is_some_and(|t| now.saturating_duration_since(t) < REQUEST_EVERY) {
                continue;
            }
            peer.last_request = Some(now);
            due.push((scope.clone(), *from));
        }
        due
    }

    /// Drop my keys and every peer key for `scope` (left the call / stopped the stream).
    pub fn forget_scope(&mut self, scope: &Scope) {
        self.mine.remove(scope);
        self.peers.retain(|(s, _), _| s != scope);
    }

    /// Drop what I hold from `user` in `scope` (they left).
    pub fn forget_peer(&mut self, scope: &Scope, user: Id) {
        self.peers.remove(&(scope.clone(), user));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rynuvo_proto::e2ee::{frame_generation, KIND_VOICE};
    use std::collections::HashSet;
    use std::time::Duration;

    const A: Id = 1;
    const B: Id = 2;
    const C: Id = 3;
    const SCOPE: Scope = Scope::Voice(10);

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// Three users who all know each other's device public keys.
    fn trio() -> (KeyState, KeyState, KeyState) {
        let keys: Vec<(Id, DeviceKey)> = [A, B, C].iter().map(|&id| (id, DeviceKey::generate())).collect();
        let mut states: Vec<KeyState> = keys
            .iter()
            .map(|(id, dk)| {
                let mut s = KeyState::new(*id, dk.clone());
                for (other, odk) in &keys {
                    if other != id {
                        s.set_peer_pubkey(*other, odk.public());
                    }
                }
                s
            })
            .collect();
        let c = states.pop().unwrap();
        let b = states.pop().unwrap();
        let a = states.pop().unwrap();
        (a, b, c)
    }

    fn deliver(out: Vec<Outgoing>, from: Id, to: Id, rx: &mut KeyState, now: Instant) {
        let mut hit = false;
        for o in out.into_iter().filter(|o| o.to == to) {
            assert!(rx.on_envelope(from, &o.scope, o.generation, &o.envelope, now));
            hit = true;
        }
        assert!(hit, "no envelope for {to}");
    }

    #[test]
    fn two_parties_exchange_and_talk() {
        let (mut a, mut b, _) = trio();
        let t0 = Instant::now();
        let out = a.share_current(&SCOPE, &[B], t0);
        deliver(out, A, B, &mut b, t0);
        let ct = a.seal(&SCOPE, KIND_VOICE, b"hello", t0).unwrap();
        assert_eq!(b.open(&SCOPE, A, KIND_VOICE, &ct, t0).as_deref(), Some(&b"hello"[..]));
    }

    #[test]
    fn previous_key_still_opens_for_ten_seconds() {
        let (mut a, mut b, _) = trio();
        let t0 = Instant::now();
        deliver(a.share_current(&SCOPE, &[B], t0), A, B, &mut b, t0);
        let old = a.seal(&SCOPE, KIND_VOICE, b"old", t0).unwrap();
        deliver(a.rotate(&SCOPE, &[B], t0), A, B, &mut b, t0);
        assert!(b.open(&SCOPE, A, KIND_VOICE, &old, t0 + Duration::from_secs(9)).is_some());
        assert!(b.open(&SCOPE, A, KIND_VOICE, &old, t0 + Duration::from_secs(11)).is_none());
    }

    #[test]
    fn rotation_switches_after_300ms() {
        let (mut a, _, _) = trio();
        let t0 = Instant::now();
        let first = a.share_current(&SCOPE, &[B], t0)[0].generation;
        let next = a.rotate(&SCOPE, &[B], t0)[0].generation;
        assert_eq!(next, first.wrapping_add(1));
        let ct = a.seal(&SCOPE, KIND_VOICE, b"x", t0 + ms(100)).unwrap();
        assert_eq!(frame_generation(&ct), Some(first));
        let ct = a.seal(&SCOPE, KIND_VOICE, b"x", t0 + ms(400)).unwrap();
        assert_eq!(frame_generation(&ct), Some(next));
    }

    #[test]
    fn missing_key_requests_after_one_second() {
        let (mut a, mut b, _) = trio();
        let t0 = Instant::now();
        a.share_current(&SCOPE, &[B], t0);
        let ct = a.seal(&SCOPE, KIND_VOICE, b"x", t0).unwrap();
        assert!(b.open(&SCOPE, A, KIND_VOICE, &ct, t0).is_none());
        assert!(b.due_requests(t0 + ms(500)).is_empty());
        assert_eq!(b.due_requests(t0 + ms(1100)), vec![(SCOPE, A)]);
        assert!(b.due_requests(t0 + ms(1500)).is_empty());
    }

    #[test]
    fn counters_never_repeat() {
        let (mut a, _, _) = trio();
        let t0 = Instant::now();
        a.share_current(&SCOPE, &[B], t0);
        let mut seen = HashSet::new();
        for i in 0..1000u64 {
            if i % 250 == 0 {
                a.rotate(&SCOPE, &[B], t0 + ms(i));
            }
            let ct = a.seal(&SCOPE, KIND_VOICE, b"x", t0 + ms(i)).unwrap();
            assert!(seen.insert(<[u8; 8]>::try_from(&ct[..8]).unwrap()), "repeat at {i}");
        }
    }

    #[test]
    fn leaver_cannot_open_after_rotation() {
        let (mut a, mut b, mut c) = trio();
        let t0 = Instant::now();
        let out = a.share_current(&SCOPE, &[B, C], t0);
        assert_eq!(out.len(), 2);
        for o in out {
            let rx = if o.to == B { &mut b } else { &mut c };
            assert!(rx.on_envelope(A, &o.scope, o.generation, &o.envelope, t0));
        }
        let out = a.rotate(&SCOPE, &[B], t0);
        assert!(out.iter().all(|o| o.to == B));
        deliver(out, A, B, &mut b, t0);
        let ct = a.seal(&SCOPE, KIND_VOICE, b"secret", t0 + Duration::from_secs(1)).unwrap();
        assert!(c.open(&SCOPE, A, KIND_VOICE, &ct, t0 + Duration::from_secs(1)).is_none());
        assert!(b.open(&SCOPE, A, KIND_VOICE, &ct, t0 + Duration::from_secs(1)).is_some());
    }
}
