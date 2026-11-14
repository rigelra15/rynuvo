//! Voice: codec, jitter buffering, mixing, the network link, and (later) devices and the engine.
pub mod codec;
pub mod gate;
pub mod jitter;
pub mod link;
pub mod mixer;

/// Samples in one 20 ms Opus frame at 48 kHz mono.
pub const FRAME: usize = 960;
