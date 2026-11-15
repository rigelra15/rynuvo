//! Plays the listen-together song inside the voice mixer, so the echo canceller hears it like voice
//! and deafen silences it. Kept in time by `set`, which the music controller calls a few times a
//! second with where the song should be now.
use std::sync::Arc;

use super::FRAME;
use crate::music::webm::Song;

/// Off by more than this, playback jumps to where it should be.
pub const DRIFT_MS: u32 = 250;
/// Music level while someone is talking (≈ −40 %).
pub const DUCKED: f32 = 0.6;

/// Packets decoded before the one a jump lands in, so the decoder has settled (Opus needs ~80 ms).
const PREROLL_PACKETS: usize = 4;
const SR_MS: u64 = 48;

struct Playing {
    entry: u64,
    song: Arc<Song>,
    dec: opus::Decoder,
    /// Next packet to decode.
    next: usize,
    /// Decoded mono samples not played yet.
    buf: std::collections::VecDeque<f32>,
    /// Song position of the next sample out, in samples.
    pos: u64,
    paused: bool,
}

impl Playing {
    fn new(entry: u64, song: Arc<Song>) -> Option<Self> {
        let ch = if song.channels >= 2 { opus::Channels::Stereo } else { opus::Channels::Mono };
        let dec = opus::Decoder::new(48_000, ch).map_err(|e| tracing::warn!("music decoder: {e}")).ok()?;
        Some(Playing { entry, song, dec, next: 0, buf: Default::default(), pos: 0, paused: false })
    }

    fn seek(&mut self, ms: u32) {
        let p = &self.song.packets;
        let hit = p.partition_point(|(t, _)| *t <= ms).saturating_sub(1);
        let from = hit.saturating_sub(PREROLL_PACKETS);
        let _ = self.dec.reset_state();
        self.buf.clear();
        self.next = from;
        self.pos = ms as u64 * SR_MS;
        // Decode up to the target and drop what comes before it.
        let start = p.get(from).map_or(0, |x| x.0 as u64 * SR_MS);
        let mut skip = (self.pos.saturating_sub(start) + if from == 0 { self.song.pre_skip as u64 } else { 0 }) as usize;
        while skip > 0 && self.decode_next() {
            let n = skip.min(self.buf.len());
            self.buf.drain(..n);
            skip -= n;
        }
    }

    /// Decode one packet into `buf`; false at the end of the song.
    fn decode_next(&mut self) -> bool {
        let Some((_, pkt)) = self.song.packets.get(self.next) else { return false };
        self.next += 1;
        let ch = self.song.channels.max(1) as usize;
        let mut pcm = [0f32; 5760 * 2];
        match self.dec.decode_float(pkt, &mut pcm, false) {
            Ok(n) if ch >= 2 => self.buf.extend(pcm[..n * 2].chunks_exact(2).map(|s| (s[0] + s[1]) * 0.5)),
            Ok(n) => self.buf.extend(&pcm[..n]),
            Err(e) => {
                tracing::debug!("music packet {}: {e}", self.next - 1);
                self.buf.extend(std::iter::repeat_n(0.0, FRAME));
            }
        }
        true
    }
}

pub struct MusicPlayer {
    cur: Option<Playing>,
    volume: f32,
    duck: f32,
}

impl Default for MusicPlayer {
    fn default() -> Self {
        Self::new()
    }
}

impl MusicPlayer {
    pub fn new() -> Self {
        MusicPlayer { cur: None, volume: 0.5, duck: 1.0 }
    }

    /// Play `entry` (its decoded `song`) from `at_ms`, or keep playing it if it already is and is
    /// no more than `DRIFT_MS` off.
    pub fn set(&mut self, entry: u64, song: Arc<Song>, at_ms: u32, paused: bool) {
        if self.cur.as_ref().is_none_or(|c| c.entry != entry) {
            self.cur = Playing::new(entry, song);
            if let Some(c) = self.cur.as_mut() {
                c.seek(at_ms);
            }
        } else if let Some(c) = self.cur.as_mut() {
            let now = (c.pos / SR_MS) as u32;
            if now.abs_diff(at_ms) > DRIFT_MS {
                c.seek(at_ms);
            }
        }
        if let Some(c) = self.cur.as_mut() {
            c.paused = paused;
        }
    }

    pub fn clear(&mut self) {
        self.cur = None;
    }

    pub fn entry(&self) -> Option<u64> {
        self.cur.as_ref().map(|c| c.entry)
    }

    /// 0.0–1.0.
    pub fn set_volume(&mut self, v: f32) {
        self.volume = v.clamp(0.0, 1.0);
    }

    pub fn position_ms(&self) -> Option<u32> {
        self.cur.as_ref().map(|c| (c.pos / SR_MS) as u32)
    }

    /// Add the next 20 ms of the song to `out`; `ducked` while someone is talking.
    pub fn mix(&mut self, out: &mut [f32; FRAME], ducked: bool) {
        let Some(c) = self.cur.as_mut() else { return };
        if c.paused {
            return;
        }
        while c.buf.len() < FRAME && c.decode_next() {}
        let target = if ducked { DUCKED } else { 1.0 };
        // Glide towards the target over ~100 ms so the change isn't heard as a click.
        let step = (target - self.duck).clamp(-0.2, 0.2) / FRAME as f32;
        for o in out.iter_mut() {
            let Some(s) = c.buf.pop_front() else { break };
            self.duck += step;
            *o += s * self.volume * self.duck;
            c.pos += 1;
        }
        if (self.duck - target).abs() < 1e-3 {
            self.duck = target;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TONE: &[u8] = include_bytes!("../../tests/fixtures/tone.webm");

    fn song() -> Arc<Song> {
        crate::music::webm::demux(TONE).unwrap()
    }

    fn energy(p: &mut MusicPlayer, frames: usize, ducked: bool) -> f32 {
        let mut e = 0.0;
        for _ in 0..frames {
            let mut out = [0f32; FRAME];
            p.mix(&mut out, ducked);
            e += out.iter().map(|x| x * x).sum::<f32>();
        }
        e
    }

    #[test]
    fn starts_where_the_song_is_and_moves_on() {
        let mut p = MusicPlayer::new();
        assert_eq!(p.position_ms(), None);
        p.set(1, song(), 500, false);
        assert_eq!(p.position_ms(), Some(500));
        assert!(energy(&mut p, 10, false) > 1.0, "audible");
        assert_eq!(p.position_ms(), Some(700));
    }

    #[test]
    fn small_drift_is_left_alone_big_drift_jumps() {
        let mut p = MusicPlayer::new();
        p.set(1, song(), 0, false);
        energy(&mut p, 10, false);
        p.set(1, song(), 200 + DRIFT_MS - 50, false);
        assert_eq!(p.position_ms(), Some(200), "within the threshold: no audible jump");
        p.set(1, song(), 1000, false);
        assert_eq!(p.position_ms(), Some(1000));
        assert!(energy(&mut p, 5, false) > 0.1, "plays right after the jump");
    }

    #[test]
    fn paused_is_silent_and_holds_its_place() {
        let mut p = MusicPlayer::new();
        p.set(1, song(), 300, true);
        assert_eq!(energy(&mut p, 10, false), 0.0);
        assert_eq!(p.position_ms(), Some(300));
        p.set(1, song(), 300, false);
        assert!(energy(&mut p, 5, false) > 0.1);
    }

    #[test]
    fn a_new_entry_starts_from_its_own_position() {
        let mut p = MusicPlayer::new();
        p.set(1, song(), 900, false);
        p.set(2, song(), 0, false);
        assert_eq!(p.position_ms(), Some(0));
    }

    #[test]
    fn volume_and_ducking() {
        let mut p = MusicPlayer::new();
        p.set(1, song(), 200, false);
        let full = energy(&mut p, 10, false);
        p.set(1, song(), 200, false);
        p.set(1, song(), 1000, false);
        p.set(1, song(), 200, false);
        energy(&mut p, 15, true); // let the duck settle
        let ducked = energy(&mut p, 10, true);
        let ratio = ducked / full;
        assert!((ratio - DUCKED * DUCKED).abs() < 0.08, "ducked to {ratio}");
        p.set_volume(0.0);
        assert_eq!(energy(&mut p, 3, false), 0.0);
    }

    #[test]
    fn the_end_is_silence() {
        let mut p = MusicPlayer::new();
        p.set(1, song(), 1400, false);
        energy(&mut p, 20, false);
        assert_eq!(energy(&mut p, 5, false), 0.0);
        p.clear();
        assert_eq!(p.position_ms(), None);
    }
}
