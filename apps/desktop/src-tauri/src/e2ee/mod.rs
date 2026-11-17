//! End-to-end encryption for voice and Go Live: key bookkeeping on top of `rynuvo_proto::e2ee`.

pub mod state;

pub use state::{KeyState, Outgoing, SharedKeys};
