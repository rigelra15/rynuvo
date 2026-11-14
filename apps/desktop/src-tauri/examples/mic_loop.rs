//! Manual check: mic → DSP → your own speakers, with the level meter printed.
//! `cargo run --example mic_loop [seconds]`. Use headphones to hear yourself without feedback.
use std::{collections::HashMap, sync::Arc, time::Duration};

use rynuvo_desktop_lib::voice::{
    devices::{self, CpalBackend},
    engine::{EngineOpts, VoiceEngine, VoiceEvent},
};

#[tokio::main]
async fn main() {
    let secs: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(10);
    println!("{:#?}", devices::list());
    let opts = EngineOpts { me: 1, input: None, output: None, threshold: 0.5, volumes: HashMap::new() };
    let engine = VoiceEngine::start(
        Box::new(CpalBackend::default()),
        opts,
        None,
        Arc::new(|_| {}),
        Arc::new(|e| match e {
            VoiceEvent::Level { db, open } => println!("level {db:6.1} dB {}", if open { "OPEN" } else { "" }),
            other => println!("{other:?}"),
        }),
    );
    engine.mic_test(true);
    tokio::time::sleep(Duration::from_secs(secs)).await;
    engine.stop();
}
