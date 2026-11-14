//! Tauri commands for Go Live: thin wrappers over `stream::ctl::StreamCtl`.
use std::sync::Arc;

use rynuvo_proto::Id;
use tauri::{
    State,
    ipc::{Channel, InvokeResponseBody},
};

use crate::{
    commands::Core,
    stream::{capture, encode},
};

pub const STREAM_EVENT: &str = "rynuvo://stream";

#[tauri::command]
pub async fn stream_sources() -> Result<Vec<capture::Source>, String> {
    tauri::async_runtime::spawn_blocking(capture::sources).await.map_err(|e| e.to_string())?
}

/// Codecs this machine can encode, best first (the UI shows them; the stream negotiates per viewer).
#[tauri::command]
pub fn stream_encoders() -> Vec<&'static str> {
    encode::available().iter().map(|c| c.name()).collect()
}

#[tauri::command]
pub async fn stream_go_live(core: State<'_, Core>, source_id: String, preset: String) -> Result<(), String> {
    let ctl = core.stream.clone();
    // Opening capture can wait on the OS (permission checks, ScreenCaptureKit setup).
    tauri::async_runtime::spawn_blocking(move || ctl.go_live(&source_id, &preset)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn stream_stop(core: State<'_, Core>) {
    core.stream.stop_live();
}

#[tauri::command]
pub fn stream_is_live(core: State<'_, Core>) -> bool {
    core.stream.is_live()
}

/// Access units arrive on `channel` as raw bytes: [codec u8][key u8][ts u32 BE][Annex B AU].
#[tauri::command]
pub fn stream_watch(core: State<'_, Core>, user_id: Id, codecs: Vec<String>, channel: Channel<InvokeResponseBody>) -> Result<(), String> {
    core.stream.watch(user_id, codecs, Arc::new(move |b| {
        let _ = channel.send(InvokeResponseBody::Raw(b));
    }))
}

#[tauri::command]
pub fn stream_unwatch(core: State<'_, Core>, user_id: Id) {
    core.stream.unwatch(user_id);
}

#[tauri::command]
pub fn stream_set_volume(core: State<'_, Core>, user_id: Id, volume: f32) {
    core.stream.set_stream_volume(user_id, volume);
}
