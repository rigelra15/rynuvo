//! Mixes every speaker's decoded audio into one 20 ms output frame.
use std::collections::HashMap;

use rynuvo_proto::Id;

use super::{FRAME, codec::Decoder, jitter::JitterBuffer};

/// RMS above which a decoded frame counts as someone speaking.
pub const SPEAKING_RMS: f32 = 0.003;
/// Frames (20 ms) without audio after which someone no longer counts as speaking (300 ms).
pub const SPEAKING_HOLD: u32 = 15;

struct Peer {
    jitter: JitterBuffer,
    dec: Decoder,
    /// Mix ticks since this peer last produced audible audio.
    quiet: u32,
}

/// Stream audio queued beyond this (200 ms) is dropped from the front: lower delay beats completeness.
pub const STREAM_MAX_BACKLOG: usize = 10 * FRAME;

pub struct Mixer {
    peers: HashMap<Id, Peer>,
    volumes: HashMap<Id, f32>,
    local: std::collections::VecDeque<f32>,
    /// Decoded Go Live audio per streamer (mono, 48 kHz).
    streams: HashMap<Id, std::collections::VecDeque<f32>>,
    stream_volumes: HashMap<Id, f32>,
}

impl Default for Mixer {
    fn default() -> Self {
        Self::new()
    }
}

impl Mixer {
    pub fn new() -> Self {
        Mixer {
            peers: HashMap::new(),
            volumes: HashMap::new(),
            local: Default::default(),
            streams: HashMap::new(),
            stream_volumes: HashMap::new(),
        }
    }

    pub fn push(&mut self, from: Id, seq: u32, payload: Vec<u8>) {
        let peer = match self.peers.entry(from) {
            std::collections::hash_map::Entry::Occupied(o) => o.into_mut(),
            std::collections::hash_map::Entry::Vacant(v) => match Decoder::new() {
                Ok(dec) => v.insert(Peer { jitter: JitterBuffer::new(3), dec, quiet: u32::MAX }),
                Err(e) => {
                    tracing::warn!("{e}");
                    return;
                }
            },
        };
        peer.jitter.push(seq, payload);
    }

    /// Volume for one person, 0.0–2.0 (1.0 = unchanged).
    pub fn set_volume(&mut self, user: Id, v: f32) {
        self.volumes.insert(user, v.clamp(0.0, 2.0));
    }

    pub fn remove(&mut self, user: Id) {
        self.peers.remove(&user);
    }

    pub fn push_stream(&mut self, user: Id, samples: &[f32]) {
        let q = self.streams.entry(user).or_default();
        q.extend(samples);
        if q.len() > STREAM_MAX_BACKLOG {
            let extra = q.len() - STREAM_MAX_BACKLOG / 2;
            q.drain(..extra);
        }
    }

    pub fn set_stream_volume(&mut self, user: Id, v: f32) {
        self.stream_volumes.insert(user, v.clamp(0.0, 2.0));
    }

    pub fn remove_stream(&mut self, user: Id) {
        self.streams.remove(&user);
    }

    pub fn stream_backlog(&self, user: Id) -> usize {
        self.streams.get(&user).map_or(0, |q| q.len())
    }

    /// Queue a locally generated sound (join/leave tones, mic test).
    pub fn play_local(&mut self, samples: Vec<f32>) {
        self.local.extend(samples);
    }

    pub fn mix(&mut self, out: &mut [f32; FRAME]) {
        out.fill(0.0);
        for (id, p) in self.peers.iter_mut() {
            let pcm = match p.jitter.pop() {
                super::jitter::Pop::Idle => {
                    p.quiet = p.quiet.saturating_add(1);
                    continue;
                }
                super::jitter::Pop::Frame(f) => p.dec.decode(f),
            };
            let rms = (pcm.iter().map(|x| x * x).sum::<f32>() / FRAME as f32).sqrt();
            p.quiet = if rms > SPEAKING_RMS { 0 } else { p.quiet.saturating_add(1) };
            let vol = self.volumes.get(id).copied().unwrap_or(1.0);
            for (o, s) in out.iter_mut().zip(pcm) {
                *o += s * vol;
            }
        }
        for (id, q) in self.streams.iter_mut() {
            let vol = self.stream_volumes.get(id).copied().unwrap_or(1.0);
            for o in out.iter_mut() {
                match q.pop_front() {
                    Some(s) => *o += s * vol,
                    None => break,
                }
            }
        }
        for o in out.iter_mut() {
            match self.local.pop_front() {
                Some(s) => *o += s,
                None => break,
            }
        }
        for o in out.iter_mut() {
            *o = o.clamp(-1.0, 1.0);
        }
    }

    /// People heard in the last 300 ms, sorted.
    pub fn speaking(&self) -> Vec<Id> {
        let mut v: Vec<Id> = self.peers.iter().filter(|(_, p)| p.quiet < SPEAKING_HOLD).map(|(id, _)| *id).collect();
        v.sort_unstable();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voice::codec::Encoder;

    fn loud() -> [f32; FRAME] {
        std::array::from_fn(|i| 0.3 * (2.0 * std::f32::consts::PI * 300.0 * i as f32 / 48_000.0).sin())
    }

    fn feed(m: &mut Mixer, enc: &mut Encoder, from: Id, frames: u32) {
        for s in 1..=frames {
            m.push(from, s, enc.encode(&loud()));
        }
    }

    fn energy(x: &[f32]) -> f32 {
        x.iter().map(|v| v * v).sum::<f32>()
    }

    #[test]
    fn sums_with_volume() {
        let mut enc = Encoder::new().unwrap();
        let mut m = Mixer::new();
        feed(&mut m, &mut enc, 7, 10);
        let mut out = [0f32; FRAME];
        for _ in 0..6 {
            m.mix(&mut out);
        }
        let full = energy(&out);
        assert!(full > 1.0, "audible: {full}");
        m.set_volume(7, 0.0);
        m.mix(&mut out);
        assert!(energy(&out) < 1e-6, "volume 0 silences that speaker");
        assert!(out.iter().all(|x| x.abs() <= 1.0));
    }

    #[test]
    fn speaking_reports_recent_talkers() {
        let mut enc = Encoder::new().unwrap();
        let mut m = Mixer::new();
        feed(&mut m, &mut enc, 7, 6);
        let mut out = [0f32; FRAME];
        for _ in 0..5 {
            m.mix(&mut out);
        }
        assert_eq!(m.speaking(), vec![7]);
        for _ in 0..40 {
            m.mix(&mut out);
        }
        assert!(m.speaking().is_empty(), "silent after 300 ms without audio");
    }

    #[test]
    fn stream_audio_is_mixed_with_its_own_volume_and_bounded_backlog() {
        let mut m = Mixer::new();
        m.push_stream(9, &vec![0.25; FRAME]);
        let mut out = [0f32; FRAME];
        m.mix(&mut out);
        assert!((out[10] - 0.25).abs() < 1e-6);
        m.set_stream_volume(9, 0.0);
        m.push_stream(9, &vec![0.25; FRAME]);
        m.mix(&mut out);
        assert_eq!(out[10], 0.0, "stream volume is separate from the voice volume");
        m.set_stream_volume(9, 1.0);
        m.push_stream(9, &vec![0.1; FRAME * 50]);
        assert!(m.stream_backlog(9) <= STREAM_MAX_BACKLOG, "a late stream never builds up seconds of delay");
        m.remove_stream(9);
        m.mix(&mut out);
        assert_eq!(out[10], 0.0);
    }

    #[test]
    fn local_sound_is_mixed() {
        let mut m = Mixer::new();
        m.play_local(vec![0.5; FRAME * 2]);
        let mut out = [0f32; FRAME];
        m.mix(&mut out);
        assert!((out[10] - 0.5).abs() < 1e-6);
        m.mix(&mut out);
        m.mix(&mut out);
        assert_eq!(out[10], 0.0, "local sound finished");
    }
}
