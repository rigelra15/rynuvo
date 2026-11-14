pub mod commands;
pub mod core;
mod diag;
#[cfg(target_os = "macos")]
mod mac_notify;
mod media;
mod notify;
pub mod prefs;
pub mod stream;
mod tray;
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
        .plugin(autostart_plugin())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, s, e| tray::on_shortcut(app, s, e.state()))
                .build(),
        )
        .manage(tray::Shortcuts::default())
        .on_window_event(|w, e| {
            // Closing the window keeps Rynuvo (and voice) running in the tray.
            if let tauri::WindowEvent::CloseRequested { api, .. } = e {
                api.prevent_close();
                let _ = w.hide();
            }
        })
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
                    if let voice::engine::VoiceEvent::State { muted, deafened, .. } = &e {
                        tray::update(&handle, *muted, *deafened);
                    }
                    let _ = handle.emit(commands::VOICE_EVENT, e);
                }),
            );
            let prefs_file = match &profile {
                Some(p) => format!("prefs-{p}.json"),
                None => "prefs.json".into(),
            };
            let prefs = prefs::PrefsStore::open(dir.join(prefs_file));
            let p = prefs.get();
            voice.set_prefs(p.audio());
            app.manage(prefs);
            app.manage(commands::Core::new(store, dir.join(file), voice));
            tray::build(app.handle())?;
            if let Err(e) = tray::register_shortcuts(app.handle(), &p) {
                tracing::warn!("shortcuts: {e}");
            }
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
            voice_cmd::prefs_get,
            voice_cmd::prefs_set,
        ])
        .build(tauri::generate_context!())
        .expect("error while running Rynuvo")
        .run(|app, e| {
            // macOS: clicking the Dock icon brings the hidden window back.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = e {
                tray::show_main(app);
            }
            let _ = (app, e);
        });
}

fn autostart_plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    let b = tauri_plugin_autostart::Builder::new();
    #[cfg(target_os = "macos")]
    let b = b.macos_launcher(tauri_plugin_autostart::MacosLauncher::LaunchAgent);
    b.build()
}
