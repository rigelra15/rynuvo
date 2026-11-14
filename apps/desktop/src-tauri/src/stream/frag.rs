//! Splits encoded frames into datagram-sized fragments and puts them back together.
//!
//! Fragment header (12 bytes, big endian): frame u32, idx u16, count u16, flags u8, codec u8, ts_ms u16.
use std::collections::BTreeMap;

pub const HEADER: usize = 12;
/// Largest fragment payload: keeps header + crypto overhead inside one 1200-byte packet.
pub const MAX_FRAG: usize = 1100;

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

pub fn split(frame: u32, data: &[u8], flags: u8, codec: u8, ts_ms: u16) -> Vec<Vec<u8>> {
    let chunks: Vec<&[u8]> = if data.is_empty() { vec![&[][..]] } else { data.chunks(MAX_FRAG).collect() };
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
    done: BTreeMap<u32, Assembled>,
    /// Last frame handed to the decoder.
    last: Option<u32>,
    need_key: bool,
    received: u64,
    lost: u64,
}

impl Default for Reassembler {
    fn default() -> Self {
        Reassembler { partial: BTreeMap::new(), done: BTreeMap::new(), last: None, need_key: true, received: 0, lost: 0 }
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
        if p.got == p.parts.len() {
            let p = self.partial.remove(&h.frame).unwrap();
            let data = p.parts.into_iter().flatten().flatten().collect();
            let a = Assembled { frame: h.frame, key: p.head.flags & KEY != 0, codec: p.head.codec, ts_ms: p.head.ts_ms, data };
            self.done.insert(h.frame, a);
        }
        self.drain()
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
}
