//! Tray icon (show / mute / deafen / quit), close-to-tray, and the global mute/deafen shortcuts.
use std::sync::Mutex;

use tauri::{
    AppHandle, Manager, Runtime,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::{commands::Core, prefs::Prefs};

pub struct TrayItems<R: Runtime> {
    mute: MenuItem<R>,
    deafen: MenuItem<R>,
}

/// The two registered shortcuts, to tell them apart in the handler.
#[derive(Default)]
pub struct Shortcuts(Mutex<Option<(Shortcut, Shortcut, Option<Shortcut>)>>);

pub fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn toggle_mute<R: Runtime>(app: &AppHandle<R>) {
    let core = app.state::<Core>();
    let m = core.voice.state();
    core.voice.set_muted(!m.muted);
}

fn toggle_deafen<R: Runtime>(app: &AppHandle<R>) {
    let core = app.state::<Core>();
    let m = core.voice.state();
    core.voice.set_deafened(!m.deafened);
}

pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show Rynuvo", true, None::<&str>)?;
    let mute = MenuItem::with_id(app, "mute", "Mute", true, None::<&str>)?;
    let deafen = MenuItem::with_id(app, "deafen", "Deafen", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Rynuvo", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&show, &sep, &mute, &deafen, &sep, &quit])?;
    let mut b = TrayIconBuilder::with_id("main")
        .tooltip("Rynuvo")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id().as_ref() {
            "show" => show_main(app),
            "mute" => toggle_mute(app),
            "deafen" => toggle_deafen(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        b = b.icon(icon.clone());
    }
    b.build(app)?;
    app.manage(TrayItems { mute, deafen });
    Ok(())
}

/// Keep the tray menu's labels in step with my voice state.
pub fn update<R: Runtime>(app: &AppHandle<R>, muted: bool, deafened: bool) {
    if let Some(t) = app.try_state::<TrayItems<R>>() {
        let _ = t.mute.set_text(if muted { "Unmute" } else { "Mute" });
        let _ = t.deafen.set_text(if deafened { "Undeafen" } else { "Deafen" });
    }
}

pub const SHORTCUT_TAKEN: &str = "That shortcut is already used by another app";

/// (Re)register the mute and deafen shortcuts from the preferences.
pub fn register_shortcuts<R: Runtime>(app: &AppHandle<R>, p: &Prefs) -> Result<(), String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let parse = |s: &str| s.parse::<Shortcut>().map_err(|_| format!("\"{s}\" is not a valid shortcut"));
    let mute = parse(&p.shortcut_mute)?;
    let deafen = parse(&p.shortcut_deafen)?;
    if mute == deafen {
        return Err("Mute and deafen need different shortcuts".into());
    }
    // The overlay's (fixed): if another app has it, the overlay just can't be clicked from the keyboard.
    let overlay = crate::overlay::SHORTCUT.parse::<Shortcut>().ok().filter(|o| *o != mute && *o != deafen);
    *app.state::<Shortcuts>().0.lock().unwrap() = Some((mute, deafen, overlay));
    for s in [mute, deafen] {
        if let Err(e) = gs.register(s) {
            tracing::warn!("register shortcut {s:?}: {e}");
            return Err(SHORTCUT_TAKEN.into());
        }
    }
    if let Some(o) = overlay
        && let Err(e) = gs.register(o)
    {
        tracing::warn!("overlay shortcut: {e}");
    }
    Ok(())
}

pub fn on_shortcut<R: Runtime>(app: &AppHandle<R>, s: &Shortcut, state: ShortcutState) {
    if state != ShortcutState::Pressed {
        return;
    }
    let pair = *app.state::<Shortcuts>().0.lock().unwrap();
    match pair {
        Some((m, _, _)) if &m == s => toggle_mute(app),
        Some((_, d, _)) if &d == s => toggle_deafen(app),
        Some((_, _, Some(o))) if &o == s => crate::overlay::toggle_interactive(app),
        _ => {}
    }
}
