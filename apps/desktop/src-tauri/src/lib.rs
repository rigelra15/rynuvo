pub mod commands;
pub mod core;
mod diag;
#[cfg(target_os = "macos")]
mod mac_notify;
mod media;
mod notify;
pub mod voice;
mod voice_cmd;

use std::sync::Arc;

use tauri::{Emitter, Manager};

use crate::core::identity::KeyringStore;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let profile = std::env::var("RYNUVO_PROFILE").ok().filter(|p| !p.is_empty());
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .register_asynchronous_uri_scheme_protocol("rynuvo-media", media::protocol)
        .setup(move |app| {
            if let Ok(dir) = app.path().app_log_dir() {
                let _ = core::logs::init(&dir);
            }
            #[cfg(target_os = "macos")]
            mac_notify::request_permission();
            let dir = app.path().app_config_dir()?;
            let file = match &profile {
                Some(p) => format!("state-{p}.json"),
                None => "state.json".into(),
            };
            let store = Arc::new(KeyringStore::new(profile.as_deref()));
            let handle = app.handle().clone();
            let voice = voice::ctl::VoiceCtl::new(
                Arc::new(|| Box::new(voice::devices::CpalBackend::default()) as Box<dyn voice::backend::AudioBackend>),
                Arc::new(move |e| {
                    let _ = handle.emit(commands::VOICE_EVENT, e);
                }),
            );
            app.manage(commands::Core::new(store, dir.join(file), voice));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::join,
            commands::claim_transfer,
            commands::logout,
            commands::rename_me,
            commands::create_channel,
            commands::update_channel,
            commands::delete_channel,
            commands::messages,
            commands::post_message,
            commands::edit_message,
            commands::delete_message,
            commands::send_typing,
            commands::transfer_code,
            commands::admin_rotate_invite,
            commands::admin_kick,
            commands::admin_transfer_code,
            commands::admin_reports,
            commands::admin_report_body,
            media::upload_image,
            media::post_image_message,
            media::save_image,
            diag::run_nettest,
            diag::send_log,
            diag::app_version,
            voice_cmd::voice_state,
            voice_cmd::voice_join,
            voice_cmd::voice_leave,
            voice_cmd::voice_set_muted,
            voice_cmd::voice_set_deafened,
            voice_cmd::voice_set_volume,
            voice_cmd::voice_devices,
            voice_cmd::voice_set_devices,
            voice_cmd::voice_set_threshold,
            voice_cmd::voice_mic_test,
            voice_cmd::voice_kick,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Rynuvo");
}
