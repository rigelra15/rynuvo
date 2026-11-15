//! Splits encoded frames into datagram-sized fragments and puts them back together.
//!
//! Fragment header (12 bytes, big endian): frame u32, idx u16, count u16, flags u8, codec u8, ts_ms u16.
//!
//! Spares: every `GROUP` fragments of a frame get one more packet, the XOR of their payloads, so a
//! single lost fragment in a group is rebuilt on arrival instead of breaking the picture until the
//! next key frame. Spare header (15 bytes): frame u32, first u16, count u16, flags u8, codec u8,
//! ts_ms u16, n u8, length XOR u16; then the XOR of the group's payloads (zero-padded).
use std::collections::BTreeMap;

pub const HEADER: usize = 12;
/// Largest fragment payload: keeps header + crypto overhead inside one 1200-byte packet.
pub const MAX_FRAG: usize = 1100;

/// Fragments covered by one spare: about 12% more traffic for repairing one loss in eight.
pub const GROUP: usize = 8;
/// Key frames are big and rare, and losing one costs the most: they get a spare per four.
pub const KEY_GROUP: usize = 4;
pub const SPARE_HEADER: usize = 15;

pub const KEY: u8 = 1;
pub const AUDIO: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub frame: u32,
    pub idx: u16,
    pub count: u16,
    pub flags: u8,
    pub codec: u8,
    pub ts_ms: u16,
}

impl Header {
    pub fn parse(b: &[u8]) -> Option<(Header, &[u8])> {
        if b.len() < HEADER {
            return None;
        }
        let u16_at = |i: usize| u16::from_be_bytes([b[i], b[i + 1]]);
        let h = Header {
            frame: u32::from_be_bytes(b[0..4].try_into().unwrap()),
            idx: u16_at(4),
            count: u16_at(6),
            flags: b[8],
            codec: b[9],
            ts_ms: u16_at(10),
        };
        if h.count == 0 || h.idx >= h.count || b.len() - HEADER > MAX_FRAG {
            return None;
        }
        Some((h, &b[HEADER..]))
    }

    pub fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.frame.to_be_bytes());
        out.extend_from_slice(&self.idx.to_be_bytes());
        out.extend_from_slice(&self.count.to_be_bytes());
        out.push(self.flags);
        out.push(self.codec);
        out.extend_from_slice(&self.ts_ms.to_be_bytes());
    }
}

fn chunks(data: &[u8]) -> Vec<&[u8]> {
    if data.is_empty() { vec![&[][..]] } else { data.chunks(MAX_FRAG).collect() }
}

/// The spares for a frame `split` cut up the same way: one per `GROUP` fragments.
pub fn fec(frame: u32, data: &[u8], flags: u8, codec: u8, ts_ms: u16) -> Vec<Vec<u8>> {
    let chunks = chunks(data);
    let count = chunks.len() as u16;
    let size = if flags & KEY != 0 { KEY_GROUP } else { GROUP };
    chunks
        .chunks(size)
        .enumerate()
        .map(|(g, group)| {
            let width = group.iter().map(|c| c.len()).max().unwrap_or(0);
            let mut parity = vec![0u8; width];
            let mut len_xor = 0u16;
            for c in group {
                parity.iter_mut().zip(c.iter()).for_each(|(p, b)| *p ^= b);
                len_xor ^= c.len() as u16;
            }
            let mut out = Vec::with_capacity(SPARE_HEADER + width);
            out.extend_from_slice(&frame.to_be_bytes());
            out.extend_from_slice(&((g * size) as u16).to_be_bytes());
            out.extend_from_slice(&count.to_be_bytes());
            out.push(flags);
            out.push(codec);
            out.extend_from_slice(&ts_ms.to_be_bytes());
            out.push(group.len() as u8);
            out.extend_from_slice(&len_xor.to_be_bytes());
            out.extend_from_slice(&parity);
            out
        })
        .collect()
}

struct Spare {
    first: usize,
    n: usize,
    len_xor: u16,
    parity: Vec<u8>,
}

pub fn split(frame: u32, data: &[u8], flags: u8, codec: u8, ts_ms: u16) -> Vec<Vec<u8>> {
    let chunks = chunks(data);
    let count = chunks.len() as u16;
    chunks
        .into_iter()
        .enumerate()
        .map(|(i, c)| {
            let mut out = Vec::with_capacity(HEADER + c.len());
            Header { frame, idx: i as u16, count, flags, codec, ts_ms }.write(&mut out);
            out.extend_from_slice(c);
            out
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assembled {
    pub frame: u32,
    pub key: bool,
    pub codec: u8,
    pub ts_ms: u16,
    pub data: Vec<u8>,
}

/// Frames completed out of order wait this many newer frames for the missing one.
const REORDER: usize = 2;
/// Largest frame accepted: 4096 fragments ≈ 4.5 MB.
const MAX_COUNT: u16 = 4096;

struct Partial {
    head: Header,
    parts: Vec<Option<Vec<u8>>>,
    got: usize,
}

pub struct Reassembler {
    partial: BTreeMap<u32, Partial>,
    spares: BTreeMap<u32, Vec<Spare>>,
    done: BTreeMap<u32, Assembled>,
    /// Last frame handed to the decoder.
    last: Option<u32>,
    need_key: bool,
    received: u64,
    lost: u64,
}

impl Default for Reassembler {
    fn default() -> Self {
        Reassembler { partial: BTreeMap::new(), spares: BTreeMap::new(), done: BTreeMap::new(), last: None, need_key: true, received: 0, lost: 0 }
    }
}

impl Reassembler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one video fragment; returns frames ready for the decoder, in order.
    pub fn push(&mut self, pkt: &[u8]) -> Vec<Assembled> {
        let Some((h, body)) = Header::parse(pkt) else { return Vec::new() };
        if h.count > MAX_COUNT || self.last.is_some_and(|l| h.frame <= l) || self.done.contains_key(&h.frame) {
            return Vec::new();
        }
        let p = self.partial.entry(h.frame).or_insert_with(|| Partial { head: h, parts: vec![None; h.count as usize], got: 0 });
        if p.head.count != h.count || p.parts[h.idx as usize].is_some() {
            return Vec::new();
        }
        p.parts[h.idx as usize] = Some(body.to_vec());
        p.got += 1;
        self.received += 1;
        self.settle(h.frame);
        self.drain()
    }

    /// Feed one spare; returns frames ready for the decoder, in order.
    pub fn push_fec(&mut self, pkt: &[u8]) -> Vec<Assembled> {
        if pkt.len() < SPARE_HEADER {
            return Vec::new();
        }
        let u16_at = |i: usize| u16::from_be_bytes([pkt[i], pkt[i + 1]]);
        let h = Header { frame: u32::from_be_bytes(pkt[0..4].try_into().unwrap()), idx: 0, count: u16_at(6), flags: pkt[8], codec: pkt[9], ts_ms: u16_at(10) };
        let (first, n, len_xor) = (u16_at(4) as usize, pkt[12] as usize, u16_at(13));
        let parity = &pkt[SPARE_HEADER..];
        let bad = h.count == 0 || h.count > MAX_COUNT || n == 0 || first + n > h.count as usize || parity.len() > MAX_FRAG;
        if bad || self.last.is_some_and(|l| h.frame <= l) || self.done.contains_key(&h.frame) {
            return Vec::new();
        }
        let p = self.partial.entry(h.frame).or_insert_with(|| Partial { head: h, parts: vec![None; h.count as usize], got: 0 });
        if p.head.count != h.count {
            return Vec::new();
        }
        let list = self.spares.entry(h.frame).or_default();
        if list.iter().all(|s| s.first != first) {
            list.push(Spare { first, n, len_xor, parity: parity.to_vec() });
        }
        self.settle(h.frame);
        self.drain()
    }

    /// Rebuild what the spares allow, then hand the frame on once every fragment is in.
    fn settle(&mut self, frame: u32) {
        let Some(p) = self.partial.get_mut(&frame) else { return };
        for s in self.spares.get(&frame).into_iter().flatten() {
            let missing: Vec<usize> = (s.first..s.first + s.n).filter(|&i| p.parts[i].is_none()).collect();
            let [m] = missing[..] else { continue };
            let mut body = s.parity.clone();
            let mut len = s.len_xor;
            for i in (s.first..s.first + s.n).filter(|&i| i != m) {
                let part = p.parts[i].as_ref().unwrap();
                body.iter_mut().zip(part.iter()).for_each(|(b, x)| *b ^= x);
                len ^= part.len() as u16;
            }
            if len as usize > body.len() {
                continue; // inconsistent spare: leave the hole
            }
            body.truncate(len as usize);
            p.parts[m] = Some(body);
            p.got += 1;
        }
        if p.got == p.parts.len() {
            let p = self.partial.remove(&frame).unwrap();
            self.spares.remove(&frame);
            let data = p.parts.into_iter().flatten().flatten().collect();
            let a = Assembled { frame, key: p.head.flags & KEY != 0, codec: p.head.codec, ts_ms: p.head.ts_ms, data };
            self.done.insert(frame, a);
        }
    }

    fn drain(&mut self) -> Vec<Assembled> {
        let mut out = Vec::new();
        loop {
            let Some((&n, a)) = self.done.first_key_value() else { break };
            let in_order = self.last.map_or(false, |l| n == l.wrapping_add(1));
            if a.key || (in_order && !self.need_key) {
                self.advance_to(n);
                let a = self.done.remove(&n).unwrap();
                if a.key {
                    self.need_key = false;
                }
                out.push(a);
            } else if in_order {
                // Decodable order, but the reference chain is broken: skip it.
                self.advance_to(n);
                self.done.remove(&n);
            } else if self.done.len() > REORDER || self.last.is_none() {
                // The missing frame is not coming: drop it and everything until a key frame.
                // (`a` is not a key frame here, or the first branch would have taken it.)
                self.need_key = true;
                self.advance_to(n);
                self.done.remove(&n);
            } else {
                break;
            }
        }
        out
    }

    /// Mark everything up to `n` as consumed, counting the fragments of abandoned frames as lost.
    fn advance_to(&mut self, n: u32) {
        let stale: Vec<u32> = self.partial.range(..n).map(|(k, _)| *k).collect();
        for k in stale {
            let p = self.partial.remove(&k).unwrap();
            self.lost += (p.parts.len() - p.got) as u64;
        }
        self.spares = self.spares.split_off(&n.wrapping_add(1));
        if let Some(l) = self.last {
            // Frames that never sent a single fragment count as one lost fragment each.
            let gap = n.wrapping_sub(l).saturating_sub(1) as u64;
            self.lost += gap.min(1000);
        }
        self.last = Some(n);
    }

    /// True while the decoder is missing a reference and must wait for a key frame.
    pub fn needs_keyframe(&self) -> bool {
        self.need_key
    }

    /// Fraction of fragments lost since the last call (0.0 when nothing arrived).
    pub fn take_loss(&mut self) -> f32 {
        let total = self.received + self.lost;
        let loss = if total == 0 { 0.0 } else { self.lost as f32 / total as f32 };
        self.received = 0;
        self.lost = 0;
        loss
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(n: usize, seed: u8) -> Vec<u8> {
        (0..n).map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed)).collect()
    }

    fn feed(r: &mut Reassembler, frags: &[Vec<u8>]) -> Vec<Assembled> {
        frags.iter().flat_map(|f| r.push(f)).collect()
    }

    #[test]
    fn split_reassemble_roundtrip() {
        let data = frame(5000, 1);
        let frags = split(0, &data, KEY, 1, 77);
        assert_eq!(frags.len(), 5);
        assert!(frags.iter().all(|f| f.len() <= HEADER + MAX_FRAG));
        let mut r = Reassembler::new();
        let out = feed(&mut r, &frags);
        assert_eq!(out, vec![Assembled { frame: 0, key: true, codec: 1, ts_ms: 77, data }]);
        assert!(!r.needs_keyframe());
    }

    #[test]
    fn empty_frame_is_one_fragment() {
        assert_eq!(split(3, &[], 0, 0, 0).len(), 1);
    }

    #[test]
    fn out_of_order_ok() {
        let mut r = Reassembler::new();
        let k = split(0, &frame(3000, 1), KEY, 1, 0);
        let f1 = split(1, &frame(2500, 2), 0, 1, 16);
        let f2 = split(2, &frame(10, 3), 0, 1, 33);
        assert_eq!(feed(&mut r, &k).len(), 1);
        // frame 2 arrives first, then frame 1 with fragments reversed
        assert!(feed(&mut r, &f2).is_empty(), "held until frame 1 completes");
        let rev: Vec<_> = f1.iter().rev().cloned().collect();
        let out = feed(&mut r, &rev);
        assert_eq!(out.iter().map(|a| a.frame).collect::<Vec<_>>(), vec![1, 2]);
        assert!(!r.needs_keyframe());
        assert_eq!(r.take_loss(), 0.0);
    }

    #[test]
    fn missing_fragment_drops_frame_and_requests_key() {
        let mut r = Reassembler::new();
        feed(&mut r, &split(0, &frame(100, 1), KEY, 1, 0));
        let mut f1 = split(1, &frame(3000, 2), 0, 1, 0);
        f1.remove(1);
        feed(&mut r, &f1);
        let mut later = Vec::new();
        for n in 2..6 {
            later.extend(feed(&mut r, &split(n, &frame(50, n as u8), 0, 1, 0)));
        }
        assert!(later.is_empty(), "delta frames after a gap are not decodable");
        assert!(r.needs_keyframe());
        assert!(r.take_loss() > 0.0);
        let out = feed(&mut r, &split(6, &frame(50, 9), KEY, 1, 0));
        assert_eq!(out.len(), 1);
        assert!(out[0].key);
        assert!(!r.needs_keyframe());
    }

    #[test]
    fn old_frame_ignored() {
        let mut r = Reassembler::new();
        feed(&mut r, &split(5, &frame(10, 1), KEY, 1, 0));
        assert!(feed(&mut r, &split(4, &frame(10, 1), 0, 1, 0)).is_empty());
    }

    #[test]
    fn starts_needing_keyframe() {
        let mut r = Reassembler::new();
        assert!(r.needs_keyframe());
        assert!(feed(&mut r, &split(9, &frame(10, 1), 0, 1, 0)).is_empty());
        assert_eq!(feed(&mut r, &split(10, &frame(10, 1), KEY, 1, 0)).len(), 1);
    }

    #[test]
    fn garbage_ignored() {
        let mut r = Reassembler::new();
        assert!(r.push(&[1, 2, 3]).is_empty());
        let mut bad = split(0, &frame(10, 1), KEY, 1, 0).remove(0);
        bad[6] = 0; // count = 0
        bad[7] = 0;
        assert!(r.push(&bad).is_empty());
    }

    fn with_spares(n: u32, data: &[u8], flags: u8) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
        (split(n, data, flags, 2, 0), fec(n, data, flags, 2, 0))
    }

    fn feed_all(r: &mut Reassembler, frags: &[Vec<u8>], spares: &[Vec<u8>]) -> Vec<Assembled> {
        let mut out = feed(r, frags);
        for s in spares {
            out.extend(r.push_fec(s));
        }
        out
    }

    #[test]
    fn one_spare_per_eight_fragments_or_four_in_a_key_frame() {
        assert_eq!(fec(0, &frame(MAX_FRAG * 20, 1), 0, 2, 0).len(), 3);
        assert_eq!(fec(0, &frame(MAX_FRAG * 20, 1), KEY, 2, 0).len(), 5);
        assert_eq!(fec(0, &frame(10, 1), 0, 2, 0).len(), 1);
    }

    #[test]
    fn a_lost_fragment_is_rebuilt_from_its_spare() {
        let mut r = Reassembler::new();
        let data = frame(MAX_FRAG * 19 + 345, 4);
        let (mut frags, spares) = with_spares(0, &data, KEY);
        frags.remove(18); // the short last fragment's group
        frags.remove(9);
        frags.remove(3);
        let out = feed_all(&mut r, &frags, &spares);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].data, data);
        assert!(out[0].key);
    }

    #[test]
    fn the_spare_can_arrive_before_the_fragments() {
        let mut r = Reassembler::new();
        let data = frame(MAX_FRAG * 5, 6);
        let (mut frags, spares) = with_spares(0, &data, KEY);
        frags.remove(2);
        for s in &spares {
            assert!(r.push_fec(s).is_empty());
        }
        let out = feed(&mut r, &frags);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].data, data);
    }

    #[test]
    fn a_frame_whose_only_fragment_is_lost_comes_back() {
        let mut r = Reassembler::new();
        feed(&mut r, &split(0, &frame(100, 1), KEY, 2, 0));
        let data = frame(700, 3);
        let out = feed_all(&mut r, &[], &fec(1, &data, 0, 2, 16));
        assert_eq!(out, vec![Assembled { frame: 1, key: false, codec: 2, ts_ms: 16, data }]);
        assert!(!r.needs_keyframe());
    }

    #[test]
    fn two_losses_in_one_group_still_need_a_key_frame() {
        let mut r = Reassembler::new();
        feed(&mut r, &split(0, &frame(100, 1), KEY, 2, 0));
        let (mut frags, spares) = with_spares(1, &frame(MAX_FRAG * 6, 2), 0);
        frags.remove(4);
        frags.remove(1);
        assert!(feed_all(&mut r, &frags, &spares).is_empty());
        for n in 2..6 {
            let (f, s) = with_spares(n, &frame(50, n as u8), 0);
            assert!(feed_all(&mut r, &f, &s).is_empty());
        }
        assert!(r.needs_keyframe());
    }

    /// Screen share at 1080p60 over a line that drops 1 packet in 200: with spares nearly every
    /// frame still plays; without them most frames are thrown away waiting for a key frame.
    #[test]
    fn half_a_percent_loss_barely_shows() {
        let run = |spares: bool| {
            let mut r = Reassembler::new();
            let mut seed: u32 = 12345;
            let mut lost = move || {
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                (seed >> 16).is_multiple_of(200)
            };
            let mut shown = 0;
            // A broken picture asks for a key frame, which comes ~100 ms (6 frames) later.
            let mut key_due: Option<u32> = None;
            for n in 0..1200u32 {
                let key = n.is_multiple_of(240) || key_due == Some(n);
                if key {
                    key_due = None;
                }
                let data = frame(if key { 150_000 } else { 20_000 }, n as u8);
                let frags: Vec<_> = split(n, &data, if key { KEY } else { 0 }, 2, 0).into_iter().filter(|_| !lost()).collect();
                let sp: Vec<_> = if spares { fec(n, &data, if key { KEY } else { 0 }, 2, 0).into_iter().filter(|_| !lost()).collect() } else { Vec::new() };
                shown += feed_all(&mut r, &frags, &sp).len();
                if r.needs_keyframe() && key_due.is_none() {
                    key_due = Some(n + 6);
                }
            }
            shown as f32 / 1200.0
        };
        let (with, without) = (run(true), run(false));
        eprintln!("frames shown: with spares {with}, without {without}");
        assert!(with >= 0.95, "with spares {with}");
        assert!(without < 0.5, "without spares {without}");
    }
}
