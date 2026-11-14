pub mod commands;
pub mod core;
mod diag;
#[cfg(target_os = "macos")]
mod mac_notify;
mod media;
mod notify;

use std::sync::Arc;

use tauri::Manager;

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
            app.manage(commands::Core::new(store, dir.join(file)));
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running Rynuvo");
}
