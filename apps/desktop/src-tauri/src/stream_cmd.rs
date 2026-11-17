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

/// A still of a screen or window for the Go Live picker, as a `data:image/jpeg` URL.
#[tauri::command]
pub async fn stream_thumbnail(source_id: String, width: u32, height: u32) -> Result<String, String> {
    use base64::Engine;
    let (w, h) = (width.min(1280), height.min(720));
    let jpeg = tauri::async_runtime::spawn_blocking(move || capture::thumbnail(&source_id, w, h)).await.map_err(|e| e.to_string())??;
    Ok(format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(jpeg)))
}

/// Codecs this machine can encode, best first (the UI shows them; the stream negotiates per viewer).
#[tauri::command]
pub fn stream_encoders() -> Vec<&'static str> {
    encode::available().iter().map(|c| c.name()).collect()
}

#[derive(serde::Serialize)]
pub struct SelfCheck {
    encoders: Vec<encode::EncoderInfo>,
    capture: &'static str,
}

/// Settings self-check: which encoders really open, and whether capture is allowed.
#[tauri::command]
pub async fn stream_self_check() -> Result<SelfCheck, String> {
    tauri::async_runtime::spawn_blocking(|| SelfCheck { encoders: encode::probe(), capture: crate::stream::capture::permission() })
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn stream_go_live(core: State<'_, Core>, source_id: String, preset: String, cursor: Option<bool>) -> Result<(), String> {
    let ctl = core.stream.clone();
    // Opening capture can wait on the OS (permission checks, ScreenCaptureKit setup).
    tauri::async_runtime::spawn_blocking(move || ctl.go_live(&source_id, &preset, cursor.unwrap_or(true))).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn stream_stop(core: State<'_, Core>) -> Result<(), String> {
    // Stopping capture waits on the OS; keep it off the UI thread.
    let ctl = core.stream.clone();
    tauri::async_runtime::spawn_blocking(move || ctl.stop_live()).await.map_err(|e| e.to_string())
}

/// The webview decoder lost its reference chain: ask the streamer for a key frame.
#[tauri::command]
pub fn stream_request_key(core: State<'_, Core>, user_id: Id) {
    core.stream.request_key(user_id);
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

/// macOS: open System Settings at Screen Recording so the user can allow Rynuvo.
#[tauri::command]
pub fn open_capture_settings() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
            .spawn();
    }
}
