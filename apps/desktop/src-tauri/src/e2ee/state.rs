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
/// A generation up to this far behind the one held counts as older (a late envelope or stale frame).
/// Anything further away is a new sender session (engine restart starts at a random generation).
const GEN_WINDOW: u32 = 1 << 16;
/// How many replaced keys per sender are kept for in-flight frames.
const MAX_PREVIOUS: usize = 3;

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

    /// Switch to the pending key once its activation time has come.
    fn promote(&mut self, now: Instant) {
        if self.pending.as_ref().is_some_and(|(_, at)| now >= *at)
            && let Some((k, _)) = self.pending.take()
        {
            self.current = k;
        }
    }
}

/// What I hold for one sender in one scope.
#[derive(Default)]
struct Peer {
    current: Option<(u32, MediaKey)>,
    /// Replaced keys, newest first, each with its own expiry.
    previous: Vec<(u32, MediaKey, Instant)>,
    missing_since: Option<Instant>,
    last_request: Option<Instant>,
}

/// `generation` is the held one or up to `GEN_WINDOW` behind it (u32 wrap-around allowed).
fn not_newer(generation: u32, held: u32) -> bool {
    let d = generation.wrapping_sub(held);
    d == 0 || d > u32::MAX - GEN_WINDOW
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

    /// Record `user`'s device key. A different key than before is a new device: drop all keys from them.
    pub fn set_peer_pubkey(&mut self, user: Id, pk: PublicKey) {
        if self.pubkeys.get(&user).is_some_and(|old| *old != pk) {
            self.peers.retain(|(_, u), _| *u != user);
        }
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

    /// Send my key for `scope` to `to` (joiners), making one if I have none. Call it with `&[]` (or
    /// the members) when joining a scope: `seal` returns None until a key exists. Inside a rotation's
    /// 300 ms window both the active and the pending key are sent, active first; deliver them in order.
    pub fn share_current(&mut self, scope: &Scope, to: &[Id], now: Instant) -> Vec<Outgoing> {
        let mine = self
            .mine
            .entry(scope.clone())
            .or_insert_with(|| Mine { current: MyKey::fresh(random_generation()), pending: None });
        mine.promote(now);
        let mine = &self.mine[scope];
        let mut out = self.envelopes(scope, &mine.current, to);
        if let Some((pending, _)) = &mine.pending {
            out.extend(self.envelopes(scope, pending, to));
        }
        out
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

    /// Take a key `from` sent me. False if `from`'s pubkey is unknown, it can't be opened, it duplicates
    /// the held generation, or it is slightly older (a late envelope). A far jump is a new session: taken.
    pub fn on_envelope(&mut self, from: Id, scope: &Scope, generation: u32, env: &[u8], now: Instant) -> bool {
        let Some(from_pub) = self.pubkeys.get(&from) else { return false };
        let Some(key) = open_envelope(&self.device, from, from_pub, self.me, scope, generation, env) else {
            return false;
        };
        let peer = self.peers.entry((scope.clone(), from)).or_default();
        if peer.current.as_ref().is_some_and(|(cur, _)| not_newer(generation, *cur)) {
            return false;
        }
        if let Some((g, k)) = peer.current.take() {
            peer.previous.insert(0, (g, k, now + PREVIOUS_KEY_TTL));
            peer.previous.truncate(MAX_PREVIOUS);
        }
        peer.current = Some((generation, key));
        peer.missing_since = None;
        peer.last_request = None;
        true
    }

    /// Encrypt one frame with my active key. None if I have no key or its counter is used up.
    pub fn seal(&mut self, scope: &Scope, kind: u8, plain: &[u8], now: Instant) -> Option<Vec<u8>> {
        let mine = self.mine.get_mut(scope)?;
        mine.promote(now);
        let k = &mut mine.current;
        if k.next == u32::MAX {
            return None;
        }
        let ct = seal_frame(&k.key, k.generation, k.next, kind, self.me, plain);
        k.next += 1;
        Some(ct)
    }

    /// Decrypt a frame from `from`. None means drop it (lost frame). Only pass members of the scope:
    /// a frame newer than any key held marks the sender as missing a key, so a request becomes due;
    /// a stale frame (older than the held key) is just loss.
    pub fn open(&mut self, scope: &Scope, from: Id, kind: u8, ct: &[u8], now: Instant) -> Option<Vec<u8>> {
        let generation = frame_generation(ct)?;
        let peer = self.peers.entry((scope.clone(), from)).or_default();
        peer.previous.retain(|(_, _, until)| now < *until);
        let key = peer
            .current
            .iter()
            .map(|(g, k)| (*g, k))
            .chain(peer.previous.iter().map(|(g, k, _)| (*g, k)))
            .find(|(g, _)| *g == generation)
            .map(|(_, k)| k);
        let Some(key) = key else {
            let stale = peer.current.as_ref().is_some_and(|(cur, _)| not_newer(generation, *cur));
            if !stale {
                peer.missing_since.get_or_insert(now);
            }
            return None;
        };
        let plain = open_frame(key, kind, from, ct)?;
        peer.missing_since = None;
        peer.last_request = None;
        Some(plain)
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

    #[cfg(test)]
    fn set_next_counter(&mut self, scope: &Scope, n: u32) {
        self.mine.get_mut(scope).expect("key").current.next = n;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rynuvo_proto::e2ee::{frame_generation, seal_envelope, seal_frame, MediaKey, KIND_VOICE};
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

    fn pair() -> (DeviceKey, KeyState, KeyState) {
        let (da, db) = (DeviceKey::generate(), DeviceKey::generate());
        let mut a = KeyState::new(A, da.clone());
        let mut b = KeyState::new(B, db.clone());
        a.set_peer_pubkey(B, db.public());
        b.set_peer_pubkey(A, da.public());
        (da, a, b)
    }

    /// An envelope from A to B for any generation, as a restarted A (same device) would send.
    fn env_from_a(da: &DeviceKey, b: &KeyState, generation: u32, key: &MediaKey) -> Vec<u8> {
        seal_envelope(da, A, B, &b.device.public(), &SCOPE, generation, key).unwrap()
    }

    #[test]
    fn late_rotation_envelope_is_rejected() {
        let (da, mut a, mut b) = pair();
        let t0 = Instant::now();
        let out = a.share_current(&SCOPE, &[B], t0);
        let g = out[0].generation;
        deliver(out, A, B, &mut b, t0);
        let late = g.wrapping_sub(1000);
        assert!(!b.on_envelope(A, &SCOPE, late, &env_from_a(&da, &b, late, &MediaKey::random()), t0));
    }

    #[test]
    fn restarted_peer_far_generation_is_accepted() {
        let (da, mut a, mut b) = pair();
        let t0 = Instant::now();
        let out = a.share_current(&SCOPE, &[B], t0);
        let g = out[0].generation;
        deliver(out, A, B, &mut b, t0);
        let far = g.wrapping_add(1 << 31);
        let k = MediaKey::random();
        assert!(b.on_envelope(A, &SCOPE, far, &env_from_a(&da, &b, far, &k), t0));
        let ct = seal_frame(&k, far, 0, KIND_VOICE, A, b"hi");
        assert_eq!(b.open(&SCOPE, A, KIND_VOICE, &ct, t0).as_deref(), Some(&b"hi"[..]));
    }

    #[test]
    fn pubkey_change_wipes_old_keys() {
        let (_, mut a, mut b) = pair();
        let t0 = Instant::now();
        deliver(a.share_current(&SCOPE, &[B], t0), A, B, &mut b, t0);
        let ct = a.seal(&SCOPE, KIND_VOICE, b"x", t0).unwrap();
        let same = b.pubkeys[&A].clone();
        b.set_peer_pubkey(A, same);
        assert!(b.open(&SCOPE, A, KIND_VOICE, &ct, t0).is_some());
        b.set_peer_pubkey(A, DeviceKey::generate().public());
        assert!(b.open(&SCOPE, A, KIND_VOICE, &ct, t0).is_none());
    }

    #[test]
    fn joiner_during_rotation_window_does_not_request() {
        let (mut a, mut b, mut c) = trio();
        let t0 = Instant::now();
        deliver(a.share_current(&SCOPE, &[B], t0), A, B, &mut b, t0);
        let next = a.rotate(&SCOPE, &[B], t0)[0].generation;
        // C joins in the window and only holds the new key.
        let out: Vec<Outgoing> =
            a.share_current(&SCOPE, &[C], t0 + ms(50)).into_iter().filter(|o| o.generation == next).collect();
        deliver(out, A, C, &mut c, t0 + ms(50));
        let old = a.seal(&SCOPE, KIND_VOICE, b"x", t0 + ms(100)).unwrap();
        assert!(c.open(&SCOPE, A, KIND_VOICE, &old, t0 + ms(100)).is_none());
        let new = a.seal(&SCOPE, KIND_VOICE, b"x", t0 + ms(400)).unwrap();
        assert!(c.open(&SCOPE, A, KIND_VOICE, &new, t0 + ms(400)).is_some());
        assert!(c.due_requests(t0 + Duration::from_secs(5)).is_empty());
    }

    #[test]
    fn double_rotation_keeps_original_key() {
        let (mut a, mut b, _) = trio();
        let t0 = Instant::now();
        deliver(a.share_current(&SCOPE, &[B], t0), A, B, &mut b, t0);
        let orig = a.seal(&SCOPE, KIND_VOICE, b"x", t0).unwrap();
        deliver(a.rotate(&SCOPE, &[B], t0), A, B, &mut b, t0);
        deliver(a.rotate(&SCOPE, &[B], t0 + ms(100)), A, B, &mut b, t0 + ms(100));
        assert!(b.open(&SCOPE, A, KIND_VOICE, &orig, t0 + ms(350)).is_some());
    }

    #[test]
    fn share_during_window_sends_active_and_pending() {
        let (mut a, mut b, mut c) = trio();
        let t0 = Instant::now();
        let out = a.share_current(&SCOPE, &[B], t0);
        let g = out[0].generation;
        deliver(out, A, B, &mut b, t0);
        let next = a.rotate(&SCOPE, &[B], t0)[0].generation;
        let out = a.share_current(&SCOPE, &[C], t0 + ms(100));
        assert_eq!(out.iter().map(|o| o.generation).collect::<Vec<_>>(), vec![g, next]);
        deliver(out, A, C, &mut c, t0 + ms(100));
        let ct = a.seal(&SCOPE, KIND_VOICE, b"x", t0 + ms(100)).unwrap();
        assert!(c.open(&SCOPE, A, KIND_VOICE, &ct, t0 + ms(100)).is_some());
        let ct = a.seal(&SCOPE, KIND_VOICE, b"x", t0 + ms(400)).unwrap();
        assert!(c.open(&SCOPE, A, KIND_VOICE, &ct, t0 + ms(400)).is_some());
        assert_eq!(a.share_current(&SCOPE, &[C], t0 + ms(500)).len(), 1);
    }

    #[test]
    fn envelope_from_unknown_pubkey_is_rejected() {
        let (_, mut a, _) = pair();
        let mut stranger = KeyState::new(B, DeviceKey::generate());
        let t0 = Instant::now();
        let o = a.share_current(&SCOPE, &[B], t0).remove(0);
        assert!(!stranger.on_envelope(A, &o.scope, o.generation, &o.envelope, t0));
    }

    #[test]
    fn duplicate_envelope_is_rejected() {
        let (_, mut a, mut b) = pair();
        let t0 = Instant::now();
        let o = a.share_current(&SCOPE, &[B], t0).remove(0);
        assert!(b.on_envelope(A, &o.scope, o.generation, &o.envelope, t0));
        assert!(!b.on_envelope(A, &o.scope, o.generation, &o.envelope, t0));
    }

    #[test]
    fn counter_exhaustion_stops_sealing() {
        let (_, mut a, _) = pair();
        let t0 = Instant::now();
        a.share_current(&SCOPE, &[B], t0);
        a.set_next_counter(&SCOPE, u32::MAX - 1);
        assert!(a.seal(&SCOPE, KIND_VOICE, b"x", t0).is_some());
        assert!(a.seal(&SCOPE, KIND_VOICE, b"x", t0).is_none());
    }
}
