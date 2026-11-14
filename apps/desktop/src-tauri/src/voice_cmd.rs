//! Tauri commands for voice: thin wrappers over `voice::ctl::VoiceCtl`.
use rynuvo_proto::Id;
use tauri::State;

use crate::{
    commands::Core,
    voice::{
        ctl::MyVoice,
        devices::{self, DeviceList},
    },
};

#[tauri::command]
pub fn voice_state(core: State<'_, Core>) -> MyVoice {
    core.voice.state()
}

#[tauri::command]
pub fn voice_join(core: State<'_, Core>, channel_id: Id) -> MyVoice {
    core.voice.join(channel_id);
    core.voice.state()
}

#[tauri::command]
pub async fn voice_leave(core: State<'_, Core>) -> Result<MyVoice, String> {
    core.voice.leave();
    Ok(core.voice.state())
}

#[tauri::command]
pub fn voice_set_muted(core: State<'_, Core>, muted: bool) -> MyVoice {
    core.voice.set_muted(muted)
}

#[tauri::command]
pub fn voice_set_deafened(core: State<'_, Core>, deafened: bool) -> MyVoice {
    core.voice.set_deafened(deafened)
}

#[tauri::command]
pub fn voice_set_volume(core: State<'_, Core>, user_id: Id, volume: f32) {
    core.voice.set_volume(user_id, volume);
}

#[tauri::command]
pub async fn voice_devices() -> Result<DeviceList, String> {
    tauri::async_runtime::spawn_blocking(devices::list).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn voice_set_devices(core: State<'_, Core>, input: Option<String>, output: Option<String>) -> Result<(), String> {
    core.voice.set_devices(input, output);
    Ok(())
}

#[tauri::command]
pub fn voice_set_threshold(core: State<'_, Core>, threshold: f32) {
    core.voice.set_threshold(threshold);
}

#[tauri::command]
pub async fn voice_mic_test(core: State<'_, Core>, on: bool) -> Result<(), String> {
    core.voice.mic_test(on);
    Ok(())
}

#[tauri::command]
pub fn voice_kick(core: State<'_, Core>, user_id: Id) {
    core.voice.kick(user_id);
}
