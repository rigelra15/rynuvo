//! Just enough of a WebM (Matroska) reader to pull the Opus packets out of a YouTube audio stream.
use std::sync::Arc;

/// A song's Opus packets, ready to decode from any position.
#[derive(Debug)]
pub struct Song {
    pub channels: u8,
    /// Samples (48 kHz) the decoder outputs before the real start of the audio.
    pub pre_skip: u32,
    /// (start time in ms, packet), in order.
    pub packets: Vec<(u32, Vec<u8>)>,
}

impl Song {
    pub fn duration_ms(&self) -> u32 {
        self.packets.last().map_or(0, |p| p.0 + 20)
    }
}

const SEGMENT: u32 = 0x1853_8067;
const INFO: u32 = 0x1549_A966;
const TIMECODE_SCALE: u32 = 0x2A_D7B1;
const TRACKS: u32 = 0x1654_AE6B;
const TRACK_ENTRY: u32 = 0xAE;
const TRACK_NUMBER: u32 = 0xD7;
const CODEC_ID: u32 = 0x86;
const CODEC_PRIVATE: u32 = 0x63A2;
const CLUSTER: u32 = 0x1F43_B675;
const CLUSTER_TIME: u32 = 0xE7;
const SIMPLE_BLOCK: u32 = 0xA3;
const BLOCK_GROUP: u32 = 0xA0;
const BLOCK: u32 = 0xA1;

/// A variable-length integer: (value, length). `keep_marker` for element IDs.
fn vint(b: &[u8], keep_marker: bool) -> Option<(u64, usize)> {
    let first = *b.first()?;
    let len = first.leading_zeros() as usize + 1;
    if len > 8 || b.len() < len {
        return None;
    }
    let mut v = if keep_marker { first as u64 } else { (first as u64) & (0xFF >> len) };
    for x in &b[1..len] {
        v = (v << 8) | *x as u64;
    }
    Some((v, len))
}

fn uint(b: &[u8]) -> u64 {
    b.iter().fold(0, |v, x| (v << 8) | *x as u64)
}

#[derive(Default)]
struct Reader {
    scale_ns: u64,
    opus_track: Option<u64>,
    channels: u8,
    pre_skip: u32,
    cluster_time: u64,
    packets: Vec<(u32, Vec<u8>)>,
    // The track entry being read.
    entry_number: u64,
    entry_opus: bool,
    entry_private: Vec<u8>,
}

impl Reader {
    /// Walk the elements in `b`, descending into the ones that hold what we need.
    fn walk(&mut self, mut b: &[u8]) {
        while let Some((id, il)) = vint(b, true) {
            let Some((size, sl)) = vint(&b[il..], false) else { return };
            let start = il + sl;
            let unknown = size == (1u64 << (7 * sl)) - 1;
            let end = if unknown { b.len() } else { (start as u64 + size).min(b.len() as u64) as usize };
            let body = &b[start..end];
            match id as u32 {
                SEGMENT | INFO | TRACKS | BLOCK_GROUP => self.walk(body),
                CLUSTER => {
                    self.cluster_time = 0;
                    self.walk(body);
                }
                TRACK_ENTRY => {
                    self.entry_number = 0;
                    self.entry_opus = false;
                    self.entry_private.clear();
                    self.walk(body);
                    if self.entry_opus && self.opus_track.is_none() {
                        self.opus_track = Some(self.entry_number);
                        let p = &self.entry_private;
                        if p.len() >= 12 && &p[..8] == b"OpusHead" {
                            self.channels = p[9];
                            self.pre_skip = u16::from_le_bytes([p[10], p[11]]) as u32;
                        }
                    }
                }
                TIMECODE_SCALE => self.scale_ns = uint(body),
                TRACK_NUMBER => self.entry_number = uint(body),
                CODEC_ID => self.entry_opus = body == b"A_OPUS",
                CODEC_PRIVATE => self.entry_private = body.to_vec(),
                CLUSTER_TIME => self.cluster_time = uint(body),
                SIMPLE_BLOCK | BLOCK => self.block(body),
                _ => {}
            }
            if unknown || end >= b.len() {
                return;
            }
            b = &b[end..];
        }
    }

    fn block(&mut self, b: &[u8]) {
        let Some((track, tl)) = vint(b, false) else { return };
        if Some(track) != self.opus_track || b.len() < tl + 3 {
            return;
        }
        let rel = i16::from_be_bytes([b[tl], b[tl + 1]]) as i64;
        let flags = b[tl + 2];
        if flags & 0x06 != 0 {
            return; // laced blocks don't occur in these streams
        }
        let t = (self.cluster_time as i64 + rel).max(0) as u64;
        let ms = t * self.scale_ns.max(1) / 1_000_000;
        self.packets.push((ms as u32, b[tl + 3..].to_vec()));
    }
}

pub fn demux(data: &[u8]) -> Result<Arc<Song>, String> {
    if vint(data, true).map(|v| v.0) != Some(0x1A45_DFA3) {
        return Err("Not a WebM file".into());
    }
    let mut r = Reader { scale_ns: 1_000_000, ..Default::default() };
    r.walk(data);
    if r.opus_track.is_none() {
        return Err("No Opus audio in this stream".into());
    }
    if r.packets.is_empty() {
        return Err("The stream has no audio".into());
    }
    Ok(Arc::new(Song { channels: r.channels.max(1), pre_skip: r.pre_skip, packets: r.packets }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TONE: &[u8] = include_bytes!("../../tests/fixtures/tone.webm");

    #[test]
    fn reads_opus_packets_from_webm() {
        let s = demux(TONE).unwrap();
        assert_eq!(s.channels, 2);
        assert_eq!(s.pre_skip, 312);
        assert!((1400..=1560).contains(&s.duration_ms()), "{}", s.duration_ms());
        assert!(s.packets.windows(2).all(|w| w[0].0 < w[1].0), "in time order");
        let mut d = opus::Decoder::new(48_000, opus::Channels::Stereo).unwrap();
        let mut out = vec![0f32; 5760 * 2];
        let n = d.decode_float(&s.packets[10].1, &mut out, false).unwrap();
        assert_eq!(n, 960, "20 ms packets");
    }

    #[test]
    fn rejects_garbage() {
        assert!(demux(b"not a webm file at all").is_err());
        assert!(demux(&TONE[..200]).is_err(), "cut before any audio");
    }
}

#[cfg(test)]
mod real {
    /// `RYNUVO_WEBM=/path/to/song.webm cargo test --lib music::webm::real -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn reads_a_downloaded_song() {
        let data = std::fs::read(std::env::var("RYNUVO_WEBM").unwrap()).unwrap();
        let s = super::demux(&data).unwrap();
        println!("channels {} pre_skip {} packets {} duration {} ms", s.channels, s.pre_skip, s.packets.len(), s.duration_ms());
        assert!(s.duration_ms() > 60_000);
    }
}
