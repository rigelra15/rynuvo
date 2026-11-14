//! Tauri commands for voice: thin wrappers over `voice::ctl::VoiceCtl`.
use rynuvo_proto::Id;
use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;

use crate::{
    commands::Core,
    prefs::{Prefs, PrefsStore},
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
pub fn voice_set_volume(core: State<'_, Core>, prefs: State<'_, PrefsStore>, user_id: Id, volume: f32) {
    core.voice.set_volume(user_id, volume);
    prefs.update(|p| {
        p.volumes.insert(user_id, volume.clamp(0.0, 2.0));
    });
}

#[tauri::command]
pub async fn voice_devices() -> Result<DeviceList, String> {
    tauri::async_runtime::spawn_blocking(devices::list).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn voice_set_devices(core: State<'_, Core>, prefs: State<'_, PrefsStore>, input: Option<String>, output: Option<String>) -> Result<(), String> {
    prefs.update(|p| {
        p.input_device = input.clone();
        p.output_device = output.clone();
    });
    core.voice.set_devices(input, output);
    Ok(())
}

#[tauri::command]
pub fn voice_set_threshold(core: State<'_, Core>, prefs: State<'_, PrefsStore>, threshold: f32) {
    core.voice.set_threshold(threshold);
    prefs.update(|p| p.vad_threshold = threshold.clamp(0.0, 1.0));
}

#[tauri::command]
pub fn prefs_get(prefs: State<'_, PrefsStore>) -> Prefs {
    prefs.get()
}

/// Save preferences and apply them: shortcuts, autostart, and the audio settings.
#[tauri::command]
pub fn prefs_set(app: AppHandle, core: State<'_, Core>, prefs: State<'_, PrefsStore>, next: Prefs) -> Result<Prefs, String> {
    let prev = prefs.get();
    if next.shortcut_mute != prev.shortcut_mute || next.shortcut_deafen != prev.shortcut_deafen {
        if let Err(e) = crate::tray::register_shortcuts(&app, &next) {
            let _ = crate::tray::register_shortcuts(&app, &prev);
            return Err(e);
        }
    }
    if next.autostart != prev.autostart {
        let al = app.autolaunch();
        let r = if next.autostart { al.enable() } else { al.disable() };
        r.map_err(|e| format!("Couldn't change start-at-login: {e}"))?;
    }
    core.voice.set_prefs(next.audio());
    Ok(prefs.update(|p| *p = next))
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
