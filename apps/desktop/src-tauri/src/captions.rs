//! Closed captions for what you hear in a call (people and shared-screen sound), made on this device
//! with whisper.cpp: the audio never leaves it, so end-to-end encryption stays whole.
//!
//! The mixer hands every 20 ms of what it plays from others (not music, not the app's own sounds) to
//! `feed`; a worker cuts it into utterances at pauses, runs Whisper on each and reports the lines.
//! The model is downloaded once, the first time captions are turned on.
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, Ordering},
        mpsc,
    },
    time::Duration,
};

use rynuvo_proto::Id;
use serde::Serialize;

/// Multilingual "small", 5-bit: good Indonesian and English for its size (~190 MB).
pub const MODEL_FILE: &str = "ggml-small-q5_1.bin";
const MODEL_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q5_1.bin";
/// What the download should come close to (a truncated file is refused).
const MODEL_MIN_BYTES: u64 = 150_000_000;

const RATE: usize = 16_000;
/// 20 ms at 16 kHz.
const STEP: usize = RATE / 50;
/// An utterance ends after this much quiet (frames of 20 ms)...
const END_QUIET: u32 = 35;
/// ...or once it's this long, so lines keep coming during a monologue.
const MAX_LEN: usize = RATE * 8;
/// Kept from just before speech starts, so the first word isn't clipped.
const PREROLL: usize = RATE * 3 / 10;
/// RMS above which a 20 ms frame counts as speech.
const SPEECH_RMS: f32 = 0.006;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Lang {
    /// Write what's said in Indonesian.
    Id,
    /// English, translating whatever language is spoken.
    En,
}

impl Lang {
    pub fn parse(s: &str) -> Lang {
        if s == "en" { Lang::En } else { Lang::Id }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum CaptionEvent {
    /// Getting ready: downloading the model (progress 0..1), or loading it.
    Preparing { step: &'static str, progress: f32 },
    On,
    Off,
    Error { message: String },
    /// One line, from whoever was loudest while it was said (a person or a streamer).
    Line { speaker: Option<Id>, text: String, at: i64 },
}

pub type CaptionSink = Arc<dyn Fn(CaptionEvent) + Send + Sync>;

// --- the tap the mixer feeds -------------------------------------------------------------------

static ON: AtomicBool = AtomicBool::new(false);
static FEED: Mutex<Option<mpsc::SyncSender<(Vec<f32>, Option<Id>)>>> = Mutex::new(None);

/// Whether the mixer should hand over audio at all (cheap, checked every 20 ms).
pub fn active() -> bool {
    ON.load(Ordering::Relaxed)
}

/// 20 ms of what's heard from others at 48 kHz mono, and who was loudest in it.
pub fn feed(frame: &[f32], loudest: Option<Id>) {
    if !active() {
        return;
    }
    let down: Vec<f32> = frame.chunks_exact(3).map(|c| (c[0] + c[1] + c[2]) / 3.0).collect();
    if let Some(tx) = FEED.lock().unwrap().as_ref() {
        // A full queue (the worker is far behind) drops audio rather than growing without bound.
        let _ = tx.try_send((down, loudest));
    }
}

// --- the controller ----------------------------------------------------------------------------

pub struct Captions {
    dir: PathBuf,
    events: CaptionSink,
    lang: Arc<AtomicU8>,
    /// Bumped on every start/stop, so a slow download from an earlier start gives up.
    run: Mutex<u64>,
}

impl Captions {
    pub fn new(dir: PathBuf, events: CaptionSink) -> Arc<Captions> {
        Arc::new(Captions { dir, events, lang: Arc::new(AtomicU8::new(0)), run: Mutex::new(0) })
    }

    pub fn model_path(&self) -> PathBuf {
        self.dir.join(MODEL_FILE)
    }

    pub fn has_model(&self) -> bool {
        std::fs::metadata(self.model_path()).is_ok_and(|m| m.len() >= MODEL_MIN_BYTES)
    }

    pub fn set_lang(&self, lang: Lang) {
        self.lang.store(lang as u8, Ordering::Relaxed);
    }

    /// Turn captions on: fetch the model if needed, load it, and start listening.
    pub async fn start(self: &Arc<Self>, lang: Lang) {
        self.set_lang(lang);
        let run = {
            let mut r = self.run.lock().unwrap();
            *r += 1;
            *r
        };
        if !self.has_model() {
            if let Err(e) = self.download(run).await {
                if *self.run.lock().unwrap() == run {
                    (self.events)(CaptionEvent::Error { message: format!("Couldn't download the captions model: {e}") });
                }
                return;
            }
        }
        if *self.run.lock().unwrap() != run {
            return; // turned off meanwhile
        }
        (self.events)(CaptionEvent::Preparing { step: "loading", progress: 1.0 });
        let path = self.model_path();
        let loaded = tokio::task::spawn_blocking(move || load(&path)).await.map_err(|e| e.to_string()).and_then(|r| r);
        let ctx = match loaded {
            Ok(c) => c,
            Err(e) => {
                (self.events)(CaptionEvent::Error { message: format!("Couldn't load the captions model: {e}") });
                return;
            }
        };
        if *self.run.lock().unwrap() != run {
            return;
        }
        // About 60 s of audio may wait while Whisper works on what came before.
        let (tx, rx) = mpsc::sync_channel(3000);
        *FEED.lock().unwrap() = Some(tx);
        ON.store(true, Ordering::Relaxed);
        let (events, lang) = (self.events.clone(), self.lang.clone());
        let _ = std::thread::Builder::new().name("rynuvo-captions".into()).spawn(move || work(ctx, rx, lang, events));
        (self.events)(CaptionEvent::On);
    }

    pub fn stop(&self) {
        *self.run.lock().unwrap() += 1;
        ON.store(false, Ordering::Relaxed);
        // Dropping the sender ends the worker once it's done with what it has.
        *FEED.lock().unwrap() = None;
        (self.events)(CaptionEvent::Off);
    }

    async fn download(&self, run: u64) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let part = self.dir.join(format!("{MODEL_FILE}.part"));
        let client = crate::core::http::tls_roots(reqwest::Client::builder()).connect_timeout(Duration::from_secs(15)).build().map_err(|e| e.to_string())?;
        let mut resp = client.get(MODEL_URL).send().await.map_err(|e| e.to_string())?.error_for_status().map_err(|e| e.to_string())?;
        let total = resp.content_length().unwrap_or(190_000_000).max(1);
        let mut file = tokio::fs::File::create(&part).await.map_err(|e| e.to_string())?;
        let mut got = 0u64;
        let mut shown = -1i32;
        use tokio::io::AsyncWriteExt;
        while let Some(chunk) = resp.chunk().await.map_err(|e| e.to_string())? {
            if *self.run.lock().unwrap() != run {
                drop(file);
                let _ = tokio::fs::remove_file(&part).await;
                return Err("cancelled".into());
            }
            file.write_all(&chunk).await.map_err(|e| e.to_string())?;
            got += chunk.len() as u64;
            let pct = (got * 100 / total) as i32;
            if pct != shown {
                shown = pct;
                (self.events)(CaptionEvent::Preparing { step: "downloading", progress: got as f32 / total as f32 });
            }
        }
        file.flush().await.map_err(|e| e.to_string())?;
        drop(file);
        if got < MODEL_MIN_BYTES {
            let _ = tokio::fs::remove_file(&part).await;
            return Err("the download was cut short".into());
        }
        tokio::fs::rename(&part, self.model_path()).await.map_err(|e| e.to_string())
    }
}

fn load(path: &std::path::Path) -> Result<whisper_rs::WhisperContext, String> {
    let mut p = whisper_rs::WhisperContextParameters::default();
    p.use_gpu(cfg!(target_os = "macos"));
    whisper_rs::WhisperContext::new_with_params(path, p).map_err(|e| e.to_string())
}

// --- the worker --------------------------------------------------------------------------------

/// Cuts the heard audio into utterances at pauses.
#[derive(Default)]
struct Cutter {
    pre: VecDeque<f32>,
    seg: Vec<f32>,
    talking: bool,
    quiet: u32,
    voiced: u32,
    who: HashMap<Id, u32>,
}

impl Cutter {
    /// Add 20 ms; returns a finished utterance and its main speaker.
    fn push(&mut self, frame: &[f32], loudest: Option<Id>) -> Option<(Vec<f32>, Option<Id>)> {
        let rms = (frame.iter().map(|x| x * x).sum::<f32>() / frame.len().max(1) as f32).sqrt();
        let speech = rms > SPEECH_RMS;
        if !self.talking {
            if !speech {
                self.pre.extend(frame);
                while self.pre.len() > PREROLL {
                    self.pre.pop_front();
                }
                return None;
            }
            self.talking = true;
            self.seg.extend(self.pre.drain(..));
        }
        self.seg.extend_from_slice(frame);
        if speech {
            self.quiet = 0;
            self.voiced += 1;
            if let Some(id) = loudest {
                *self.who.entry(id).or_default() += 1;
            }
        } else {
            self.quiet += 1;
        }
        if self.quiet < END_QUIET && self.seg.len() < MAX_LEN {
            return None;
        }
        let seg = std::mem::take(&mut self.seg);
        let voiced = std::mem::take(&mut self.voiced);
        let who = std::mem::take(&mut self.who).into_iter().max_by_key(|(_, n)| *n).map(|(id, _)| id);
        self.talking = false;
        self.quiet = 0;
        // A cough or a click isn't worth a line.
        (voiced >= 12).then_some((seg, who))
    }
}

/// What Whisper says to silence and noise it was trained on (video outros, subtitle credits).
fn hallucination(t: &str) -> bool {
    let l = t.trim().trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
    if l.is_empty() || t.trim_start().starts_with(['[', '(', '*', '♪']) {
        return true;
    }
    const JUNK: &[&str] = &[
        "terima kasih",
        "terima kasih telah menonton",
        "terima kasih sudah menonton",
        "sampai jumpa di video selanjutnya",
        "thank you",
        "thank you for watching",
        "thanks for watching",
        "you",
        "bye",
        "subtitle by",
        "amara org",
    ];
    JUNK.contains(&l.as_str()) || l.contains("subtitles by") || l.contains("amara.org")
}

/// One utterance (16 kHz mono) to text.
fn transcribe(state: &mut whisper_rs::WhisperState, seg: Vec<f32>, lang: Lang, threads: i32) -> Option<String> {
    let en = lang == Lang::En;
    let mut p = whisper_rs::FullParams::new(whisper_rs::SamplingStrategy::Greedy { best_of: 1 });
    p.set_n_threads(threads);
    // English: translate whatever is said. Indonesian: write it down as Indonesian.
    p.set_language(Some(if en { "en" } else { "id" }));
    p.set_translate(en);
    p.set_no_context(true);
    p.set_single_segment(true);
    p.set_no_timestamps(true);
    p.set_suppress_blank(true);
    p.set_suppress_nst(true);
    p.set_print_special(false);
    p.set_print_progress(false);
    p.set_print_realtime(false);
    p.set_print_timestamps(false);
    // Whisper wants at least a second; pad short utterances with silence.
    let mut audio = seg;
    if audio.len() < RATE + RATE / 10 {
        audio.resize(RATE + RATE / 10, 0.0);
    }
    if let Err(e) = state.full(p, &audio) {
        tracing::warn!("captions: {e}");
        return None;
    }
    let text: String = state
        .as_iter()
        .filter(|s| s.no_speech_probability() < 0.6)
        .filter_map(|s| s.to_str_lossy().ok().map(|t| t.into_owned()))
        .collect::<Vec<_>>()
        .join(" ");
    Some(text.trim().to_string())
}

fn work(ctx: whisper_rs::WhisperContext, rx: mpsc::Receiver<(Vec<f32>, Option<Id>)>, lang: Arc<AtomicU8>, events: CaptionSink) {
    let mut state = match ctx.create_state() {
        Ok(s) => s,
        Err(e) => {
            events(CaptionEvent::Error { message: format!("Captions couldn't start: {e}") });
            return;
        }
    };
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(2, 6) as i32;
    let mut cut = Cutter::default();
    let mut carry: Vec<f32> = Vec::new();
    loop {
        let (pcm, who) = match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(m) => m,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        };
        carry.extend(pcm);
        while carry.len() >= STEP {
            let frame: Vec<f32> = carry.drain(..STEP).collect();
            let Some((seg, speaker)) = cut.push(&frame, who) else { continue };
            let en = lang.load(Ordering::Relaxed) == Lang::En as u8;
            let Some(text) = transcribe(&mut state, seg, if en { Lang::En } else { Lang::Id }, threads) else { continue };
            if hallucination(&text) || !active() {
                continue;
            }
            events(CaptionEvent::Line { speaker, text, at: crate::core::now_ms() });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(n: usize, amp: f32) -> Vec<f32> {
        (0..n).map(|i| amp * (i as f32 * 0.3).sin()).collect()
    }

    #[test]
    fn an_utterance_ends_at_a_pause_and_goes_to_whoever_spoke() {
        let mut c = Cutter::default();
        for _ in 0..10 {
            assert!(c.push(&vec![0.0; STEP], None).is_none(), "silence alone says nothing");
        }
        for _ in 0..40 {
            assert!(c.push(&tone(STEP, 0.2), Some(7)).is_none());
        }
        let mut done = None;
        for _ in 0..END_QUIET {
            done = done.or(c.push(&vec![0.0; STEP], None));
        }
        let (seg, who) = done.expect("ends after the pause");
        assert_eq!(who, Some(7));
        assert!(seg.len() >= 40 * STEP, "keeps the speech (and a bit before it)");
    }

    #[test]
    fn a_long_monologue_is_cut_into_lines() {
        let mut c = Cutter::default();
        let lines = (0..(MAX_LEN / STEP) * 2 + 5).filter_map(|_| c.push(&tone(STEP, 0.2), Some(1))).count();
        assert_eq!(lines, 2);
    }

    #[test]
    fn a_click_is_not_a_line() {
        let mut c = Cutter::default();
        let mut got = None;
        for _ in 0..3 {
            got = got.or(c.push(&tone(STEP, 0.3), Some(1)));
        }
        for _ in 0..END_QUIET + 2 {
            got = got.or(c.push(&vec![0.0; STEP], None));
        }
        assert!(got.is_none());
    }

    /// With a real model: `RYNUVO_WHISPER_MODEL=path RYNUVO_WHISPER_WAV=16k-f32le.raw cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn transcribes_real_speech() {
        let model = std::env::var("RYNUVO_WHISPER_MODEL").expect("model path");
        let raw = std::fs::read(std::env::var("RYNUVO_WHISPER_WAV").expect("audio")).unwrap();
        let audio: Vec<f32> = raw.chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).collect();
        let ctx = load(std::path::Path::new(&model)).unwrap();
        let mut st = ctx.create_state().unwrap();
        for lang in [Lang::Id, Lang::En] {
            let t0 = std::time::Instant::now();
            let text = transcribe(&mut st, audio.clone(), lang, 6).unwrap();
            println!("{lang:?} in {:?}: {text}", t0.elapsed());
        }
    }

    #[test]
    fn whisper_noise_is_dropped() {
        assert!(hallucination("Terima kasih."));
        assert!(hallucination(" [Musik]"));
        assert!(hallucination("Thanks for watching!"));
        assert!(!hallucination("Ayo main lagi nanti malam"));
    }
}
