//! Tauri commands for voice: thin wrappers over `voice::ctl::VoiceCtl`.
use rynuvo_proto::Id;
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, State};

use crate::{
    commands::Core,
    e2ee::trust::{KeyStatus, Trust},
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
    #[cfg(desktop)]
    apply_desktop(&app, &prev, &next)?;
    #[cfg(mobile)]
    let _ = (&app, &prev);
    core.voice.set_prefs(next.audio());
    Ok(prefs.update(|p| *p = next))
}

/// Global shortcuts and start-at-login (desktop only).
#[cfg(desktop)]
fn apply_desktop(app: &AppHandle, prev: &Prefs, next: &Prefs) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    if next.shortcut_mute != prev.shortcut_mute || next.shortcut_deafen != prev.shortcut_deafen {
        if let Err(e) = crate::tray::register_shortcuts(app, next) {
            let _ = crate::tray::register_shortcuts(app, prev);
            return Err(e);
        }
    }
    if next.autostart != prev.autostart {
        let al = app.autolaunch();
        let r = if next.autostart { al.enable() } else { al.disable() };
        r.map_err(|e| format!("Couldn't change start-at-login: {e}"))?;
    }
    Ok(())
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

/// Tauri event the UI listens to for "re-read the call's verification data".
pub const E2EE_EVENT: &str = "e2ee-changed";

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemberSecurity {
    pub user_id: Id,
    pub status: KeyStatus,
    /// Their current device key (base64): pass it back to `e2ee_verify`.
    pub pubkey: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallSecurity {
    /// The 30-digit code over everyone in the call, me included.
    pub code: String,
    /// Everyone else in the call. Reading this records their keys as seen.
    pub members: Vec<MemberSecurity>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonCode {
    pub code: String,
    /// The key of theirs the code covers (base64): pass it to `e2ee_verify` once compared.
    pub pubkey: String,
}

/// The call's verification code and each other member's key status; None when not in voice.
#[tauri::command]
pub fn e2ee_call(core: State<'_, Core>, trust: State<'_, Mutex<Trust>>) -> Option<CallSecurity> {
    let keys = core.voice.call_keys()?;
    let mut all = vec![keys.me.clone()];
    all.extend(keys.others.iter().cloned());
    let statuses = trust.lock().unwrap().statuses(&keys.others);
    let members = keys.others.iter().zip(statuses).map(|((u, pk), status)| MemberSecurity { user_id: *u, status, pubkey: pk.to_b64() }).collect();
    Some(CallSecurity { code: rynuvo_proto::e2ee::call_code(&all), members })
}

/// The 30-digit code for me and `user_id`, to compare in person; None if they are not in my call.
#[tauri::command]
pub fn e2ee_person_code(core: State<'_, Core>, user_id: Id) -> Option<PersonCode> {
    let keys = core.voice.call_keys()?;
    let (_, pk) = keys.others.iter().find(|(u, _)| *u == user_id)?;
    Some(PersonCode { code: rynuvo_proto::e2ee::person_code((keys.me.0, &keys.me.1), (user_id, pk)), pubkey: pk.to_b64() })
}

/// Mark `user_id`'s device key verified, but only if `pubkey` (what the UI showed) is still their key.
#[tauri::command]
pub fn e2ee_verify(app: AppHandle, core: State<'_, Core>, trust: State<'_, Mutex<Trust>>, user_id: Id, pubkey: String) -> Result<(), String> {
    let keys = core.voice.call_keys().ok_or("not in a call")?;
    let pk = keys.verify_target(user_id, &pubkey)?;
    trust.lock().unwrap().verify(user_id, &pk);
    let _ = app.emit(E2EE_EVENT, ());
    Ok(())
}
