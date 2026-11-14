//! Per-speaker jitter buffer for 20 ms frames, keyed by the relay's per-listener sequence numbers.
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU32, Ordering},
};

use super::codec::Frame;

/// Most frames held (1 s); a packet further ahead than this resets the buffer.
pub const CAPACITY: u32 = 50;
/// Consecutive empty slots after which the speaker is considered to have stopped talking.
pub const IDLE_AFTER: u32 = 15;

/// A speaker's audio frame counter: one number per 20 ms of time, sent or not. Listeners play by
/// these numbers, so a pause has to show up as a gap; otherwise the speech after it lands behind
/// the listener's playhead and is dropped as late.
#[derive(Default)]
pub struct AudioClock(AtomicU32);

impl AudioClock {
    /// The number for a frame that is being sent now.
    pub fn send(&self) -> u32 {
        self.0.fetch_add(1, Ordering::Relaxed).wrapping_add(1)
    }

    /// A 20 ms frame passed with nothing sent (voice gate closed, muted).
    pub fn skip(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

#[derive(Debug)]
pub enum Pop {
    Frame(Frame),
    /// Nothing to play (prebuffering or the speaker is silent).
    Idle,
}

pub struct JitterBuffer {
    target: usize,
    frames: BTreeMap<u32, Vec<u8>>,
    /// Sequence number of the next slot to play.
    next: Option<u32>,
    playing: bool,
    missing: u32,
}

impl JitterBuffer {
    /// `target` frames are collected before playback starts (3 × 20 ms = 60 ms).
    pub fn new(target: usize) -> Self {
        JitterBuffer { target: target.max(1), frames: BTreeMap::new(), next: None, playing: false, missing: 0 }
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    pub fn push(&mut self, seq: u32, payload: Vec<u8>) {
        if let Some(n) = self.next {
            if seq < n {
                return; // too late
            }
            if seq >= n.saturating_add(CAPACITY) {
                self.frames.clear();
                self.next = None;
                self.playing = false;
            }
        }
        self.frames.entry(seq).or_insert(payload);
        while self.frames.len() > CAPACITY as usize {
            self.frames.pop_first();
        }
    }

    pub fn pop(&mut self) -> Pop {
        if !self.playing {
            if self.frames.len() < self.target {
                return Pop::Idle;
            }
            self.playing = true;
            self.missing = 0;
            self.next = self.frames.keys().next().copied();
        }
        let n = self.next.expect("playing implies a playhead");
        self.next = Some(n.wrapping_add(1));
        if let Some(d) = self.frames.remove(&n) {
            self.missing = 0;
            return Pop::Frame(Frame::Data(d));
        }
        self.missing += 1;
        if self.missing >= IDLE_AFTER {
            // The talk spurt is over; the next one starts from whatever number it brings.
            self.playing = false;
            self.next = None;
            return Pop::Idle;
        }
        match self.frames.get(&n.wrapping_add(1)) {
            Some(d) => Pop::Frame(Frame::Fec(d.clone())),
            None => Pop::Frame(Frame::Lost),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(p: Pop) -> Vec<u8> {
        match p {
            Pop::Frame(Frame::Data(d)) => d,
            other => panic!("expected data, got {other:?}"),
        }
    }

    fn filled(seqs: &[u32]) -> JitterBuffer {
        let mut j = JitterBuffer::new(3);
        for &s in seqs {
            j.push(s, vec![s as u8]);
        }
        j
    }

    #[test]
    fn in_order() {
        let mut j = JitterBuffer::new(3);
        j.push(1, vec![1]);
        j.push(2, vec![2]);
        assert!(matches!(j.pop(), Pop::Idle), "prebuffering");
        j.push(3, vec![3]);
        assert_eq!(data(j.pop()), [1]);
        assert_eq!(data(j.pop()), [2]);
        assert_eq!(data(j.pop()), [3]);
    }

    #[test]
    fn reorder_within_window() {
        let mut j = filled(&[1, 3, 2]);
        assert_eq!(data(j.pop()), [1]);
        assert_eq!(data(j.pop()), [2]);
        assert_eq!(data(j.pop()), [3]);
    }

    #[test]
    fn duplicate_ignored() {
        let mut j = filled(&[1, 2, 2, 3]);
        assert_eq!(data(j.pop()), [1]);
        assert_eq!(data(j.pop()), [2]);
        assert_eq!(data(j.pop()), [3]);
    }

    #[test]
    fn gap_with_next_uses_fec() {
        let mut j = filled(&[1, 3, 4]);
        assert_eq!(data(j.pop()), [1]);
        assert!(matches!(j.pop(), Pop::Frame(Frame::Fec(d)) if d == [3]));
        assert_eq!(data(j.pop()), [3]);
    }

    #[test]
    fn gap_without_next_is_lost() {
        let mut j = filled(&[1, 2, 3]);
        for _ in 0..3 {
            j.pop();
        }
        j.push(6, vec![6]);
        assert!(matches!(j.pop(), Pop::Frame(Frame::Lost)), "4 missing, 5 missing too");
        assert!(matches!(j.pop(), Pop::Frame(Frame::Fec(_))), "5 missing, 6 present");
        assert_eq!(data(j.pop()), [6]);
    }

    #[test]
    fn long_silence_goes_idle_and_rebuffers() {
        let mut j = filled(&[1, 2, 3]);
        for _ in 0..3 {
            j.pop();
        }
        let mut idle = false;
        for _ in 0..20 {
            if matches!(j.pop(), Pop::Idle) {
                idle = true;
                break;
            }
        }
        assert!(idle, "goes idle after a talk spurt ends");
        j.push(40, vec![40]);
        assert!(matches!(j.pop(), Pop::Idle), "rebuffers before playing again");
        j.push(41, vec![41]);
        j.push(42, vec![42]);
        assert_eq!(data(j.pop()), [40]);
    }

    #[test]
    fn late_packet_dropped() {
        let mut j = filled(&[1, 2, 3]);
        assert_eq!(data(j.pop()), [1]);
        assert_eq!(data(j.pop()), [2]);
        j.push(1, vec![1]);
        assert_eq!(data(j.pop()), [3]);
        assert_eq!(j.len(), 0);
    }

    /// Real time, one 20 ms tick per step: the sender talks, pauses briefly (voice gate closed),
    /// talks again; the listener pops one frame per tick. Every spoken frame must be played.
    fn heard_after_pause(pause: usize) -> (usize, usize) {
        let clock = AudioClock::default();
        let mut j = JitterBuffer::new(3);
        let talk = |n: usize| std::iter::repeat_n(true, n);
        let ticks: Vec<bool> = talk(20).chain(std::iter::repeat_n(false, pause)).chain(talk(20)).chain(std::iter::repeat_n(false, 40)).collect();
        let (mut sent, mut played) = (0, 0);
        for speak in ticks {
            if speak {
                let n = clock.send();
                j.push(n, vec![1]);
                sent += 1;
            } else {
                clock.skip();
            }
            if matches!(j.pop(), Pop::Frame(Frame::Data(_))) {
                played += 1;
            }
        }
        (sent, played)
    }

    #[test]
    fn speech_after_a_short_pause_is_not_dropped() {
        for pause in [1, 5, 10, 14, 15, 30, 60] {
            let (sent, played) = heard_after_pause(pause);
            assert_eq!(played, sent, "pause of {pause} frames: played {played} of {sent}");
        }
    }

    #[test]
    fn a_new_talk_spurt_after_idle_plays_even_if_numbered_behind_the_playhead() {
        // An older client numbers only the frames it sends, so after a pause its next frame is
        // behind where this listener's playhead ran to while waiting.
        let mut j = filled(&[1, 2, 3]);
        while !matches!(j.pop(), Pop::Idle) {}
        for s in 4..=6 {
            j.push(s, vec![s as u8]);
        }
        assert_eq!(data(j.pop()), [4]);
    }

    #[test]
    fn far_future_resets() {
        let mut j = filled(&[1, 2, 3]);
        j.pop();
        j.push(1000, vec![9]);
        assert!(j.len() <= 50);
        // Playback resumes from the new position after prebuffering.
        j.push(1001, vec![10]);
        j.push(1002, vec![11]);
        assert_eq!(data(j.pop()), [9]);
    }
}
