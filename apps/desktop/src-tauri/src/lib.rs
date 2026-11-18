mod apk_update;
pub mod captions;
mod captions_cmd;
pub mod commands;
pub mod core;
mod diag;
pub mod e2ee;
#[cfg(target_os = "macos")]
mod mac_notify;
mod media;
pub mod music;
mod notify;
pub mod prefs;
pub mod stream;
#[cfg(desktop)]
mod tray;
pub mod voice;
mod voice_cmd;
mod stream_cmd;
mod music_cmd;

use std::sync::Arc;

use tauri::{Emitter, Manager};


#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Two crypto backends are linked (see Cargo.toml): say which one TLS uses, before any connection.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let profile = std::env::var("RYNUVO_PROFILE").ok().filter(|p| !p.is_empty());
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_process::init());
    #[cfg(mobile)]
    let builder = builder.plugin(tauri_plugin_barcode_scanner::init());
    #[cfg(desktop)]
    let builder = builder
        .plugin(tauri_plugin_updater::Builder::new().build())
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
        });
    builder
        .register_asynchronous_uri_scheme_protocol("rynuvo-media", media::protocol)
        .register_asynchronous_uri_scheme_protocol("rynuvo-video", music::vproxy::protocol)
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
            #[cfg(not(target_os = "android"))]
            let store = Arc::new(core::identity::KeyringStore::new(profile.as_deref()));
            #[cfg(target_os = "android")]
            let store = Arc::new(core::identity::FileStore::open(dir.join("secrets.json")));
            let handle = app.handle().clone();
            let voice = voice::ctl::VoiceCtl::new(
                Arc::new(|| Box::new(voice::devices::CpalBackend::default()) as Box<dyn voice::backend::AudioBackend>),
                Arc::new(move |e| {
                    #[cfg(desktop)]
                    if let voice::engine::VoiceEvent::State { muted, deafened, .. } = &e {
                        tray::update(&handle, *muted, *deafened);
                    }
                    if let voice::engine::VoiceEvent::State { channel_id: None, .. } = &e {
                        // Out of voice: keys first seen in that call stop counting as new.
                        if let Some(t) = handle.try_state::<std::sync::Mutex<e2ee::trust::Trust>>() {
                            t.lock().unwrap().end_session();
                        }
                    }
                    if matches!(e, voice::engine::VoiceEvent::E2eeChanged) {
                        let _ = handle.emit(voice_cmd::E2EE_EVENT, ());
                        return;
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
            let trust_file = match &profile {
                Some(p) => format!("trust-{p}.json"),
                None => "trust.json".into(),
            };
            app.manage(std::sync::Mutex::new(e2ee::trust::Trust::open(dir.join(trust_file))));
            let h2 = app.handle().clone();
            let stream = stream::ctl::StreamCtl::new(
                voice.clone(),
                Arc::new(|source: &str, opts, video, audio, ended| {
                    let c = stream::capture::start(source, opts, video, audio, ended)?;
                    Ok(Box::new(move || c.stop()) as Box<dyn FnOnce() + Send>)
                }),
                Arc::new(move |e| {
                    let _ = h2.emit(stream_cmd::STREAM_EVENT, e);
                }),
                tauri::async_runtime::handle().inner().clone(),
            );
            let http = music::ytm::client();
            let h3 = app.handle().clone();
            let music = music::ctl::MusicCtl::new(
                Arc::new(music_cmd::VoiceOut(voice.clone())),
                music_cmd::loader(http.clone()),
                Arc::new(move |s| {
                    let _ = h3.emit(music_cmd::MUSIC_EVENT, s);
                }),
            );
            tauri::async_runtime::spawn({
                let m = music.clone();
                async move { m.spawn_ticker() }
            });
            app.manage(music_cmd::MusicHttp(http));
            // Captions: the model is shared by every profile on this device.
            let h4 = app.handle().clone();
            app.manage(captions::Captions::new(
                app.path().app_data_dir()?.join("models"),
                Arc::new(move |e| {
                    let _ = h4.emit(captions_cmd::CAPTIONS_EVENT, e);
                }),
            ));
            app.manage(commands::Core::new(store, dir.join(file), voice, stream, music));
            #[cfg(desktop)]
            {
                tray::build(app.handle())?;
                if let Err(e) = tray::register_shortcuts(app.handle(), &p) {
                    tracing::warn!("shortcuts: {e}");
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            apk_update::apk_update_check,
            apk_update::apk_update_download,
            commands::join,
            commands::claim_transfer,
            commands::logout,
            commands::rename_me,
            commands::set_profile,
            commands::create_channel,
            commands::update_channel,
            commands::delete_channel,
            commands::messages,
            commands::post_message,
            commands::edit_message,
            commands::delete_message,
            commands::send_typing,
            commands::mark_read,
            commands::transfer_code,
            commands::push_register,
            commands::push_unregister,
            commands::set_away,
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
            diag::export_stream_diagnostics,
            diag::app_version,
            stream_cmd::stream_sources,
            stream_cmd::stream_thumbnail,
            stream_cmd::stream_encoders,
            stream_cmd::stream_go_live,
            stream_cmd::stream_stop,
            stream_cmd::stream_is_live,
            stream_cmd::stream_watch,
            stream_cmd::stream_unwatch,
            stream_cmd::stream_set_volume,
            stream_cmd::stream_adjust,
            captions_cmd::captions_info,
            captions_cmd::captions_start,
            captions_cmd::captions_stop,
            captions_cmd::captions_set_lang,
            stream_cmd::stream_request_key,
            stream_cmd::stream_self_check,
            stream_cmd::open_capture_settings,
            music_cmd::music_play,
            music_cmd::music_search,
            music_cmd::music_cmd,
            music_cmd::music_lyrics,
            music_cmd::music_annotate,
            music_cmd::music_video_url,
            music_cmd::music_status,
            music_cmd::music_set_volume,
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
            voice_cmd::voice_invite,
            voice_cmd::e2ee_call,
            voice_cmd::e2ee_person_code,
            voice_cmd::e2ee_verify,
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

#[cfg(desktop)]
fn autostart_plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    let b = tauri_plugin_autostart::Builder::new();
    #[cfg(target_os = "macos")]
    let b = b.macos_launcher(tauri_plugin_autostart::MacosLauncher::LaunchAgent);
    b.build()
}
