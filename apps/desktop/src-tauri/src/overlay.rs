//! The call overlay: a small see-through window that stays on top of other apps (games included,
//! when they run windowed or borderless) while Rynuvo is in the background, showing who's in the
//! room and who's talking. Clicks go through it to whatever is underneath, until the overlay
//! shortcut makes it take them (to mute, deafen, open Rynuvo or move it); the shortcut again, or
//! its lock button, lets clicks through again.
//!
//! The main window decides when it shows ([overlay_show]) and sends it what to draw; the overlay
//! page sends its buttons back to the main window, which owns the call.
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, Runtime, WebviewUrl, WebviewWindowBuilder, WindowEvent};

pub const LABEL: &str = "overlay";
/// To the overlay page: `true` when it takes clicks.
pub const MODE_EVENT: &str = "rynuvo://overlay-mode";
pub const SHORTCUT: &str = "CmdOrCtrl+Shift+O";
const WIDTH: f64 = 260.0;

#[derive(Default)]
pub struct Overlay {
    /// It takes clicks (the shortcut was pressed); otherwise they pass through.
    interactive: Mutex<bool>,
}

#[derive(Serialize, Deserialize, Default)]
struct Saved {
    x: i32,
    y: i32,
}

fn saved_path<R: Runtime>(app: &AppHandle<R>) -> Option<std::path::PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("overlay-position.json"))
}

fn window<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<tauri::WebviewWindow<R>> {
    if let Some(w) = app.get_webview_window(LABEL) {
        return Ok(w);
    }
    let w = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html".into()))
        .title("Rynuvo overlay")
        .inner_size(WIDTH, 120.0)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .focusable(false)
        .focused(false)
        .visible(false)
        .build()?;
    // Where it was left last time, else the top left corner of the main screen.
    let saved = saved_path(app).and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice::<Saved>(&b).ok());
    let on_screen = |s: &Saved| {
        w.available_monitors().unwrap_or_default().iter().any(|m| {
            let (p, z) = (m.position(), m.size());
            s.x >= p.x && s.y >= p.y && s.x < p.x + z.width as i32 - 40 && s.y < p.y + z.height as i32 - 40
        })
    };
    match saved.filter(on_screen) {
        Some(s) => {
            let _ = w.set_position(PhysicalPosition::new(s.x, s.y));
        }
        None => {
            if let Ok(Some(m)) = w.primary_monitor() {
                                let _ = w.set_position(PhysicalPosition::new(m.position().x, m.position().y));
            }
        }
    }
    let _ = w.set_ignore_cursor_events(true);
    #[cfg(target_os = "macos")]
    above_full_screen(&w);
    let h = app.clone();
    w.on_window_event(move |e| {
        // Only where it was dragged to (it takes clicks then), not where it was put.
        if let WindowEvent::Moved(p) = e
            && *h.state::<Overlay>().interactive.lock().unwrap()
            && let Some(path) = saved_path(&h)
        {
            let _ = std::fs::write(path, serde_json::to_vec(&Saved { x: p.x, y: p.y }).unwrap_or_default());
        }
    });
    Ok(w)
}

/// macOS: over other apps' full-screen spaces too, and above ordinary floating windows.
#[cfg(target_os = "macos")]
fn above_full_screen<R: Runtime>(w: &tauri::WebviewWindow<R>) {
    use objc2::{msg_send, runtime::AnyObject};
    const CAN_JOIN_ALL_SPACES: usize = 1 << 0;
    const STATIONARY: usize = 1 << 4;
    const IGNORES_CYCLE: usize = 1 << 6;
    const FULL_SCREEN_AUXILIARY: usize = 1 << 8;
    // Above the shield a full-screen game puts over the display (it takes the screen, changing its
    // resolution); anything lower stays hidden behind it.
    unsafe extern "C" {
        fn CGShieldingWindowLevel() -> i32;
    }
    let level = unsafe { CGShieldingWindowLevel() } as isize + 1;
    let w2 = w.clone();
    let _ = w.run_on_main_thread(move || unsafe {
        let Ok(ns) = w2.ns_window() else { return };
        let ns = &*(ns as *mut AnyObject);
        let _: () = msg_send![ns, setCollectionBehavior: CAN_JOIN_ALL_SPACES | STATIONARY | IGNORES_CYCLE | FULL_SCREEN_AUXILIARY];
        let _: () = msg_send![ns, setLevel: level];
        let _: () = msg_send![ns, setHidesOnDeactivate: false];
    });
}

fn set_interactive<R: Runtime>(app: &AppHandle<R>, on: bool) {
    *app.state::<Overlay>().interactive.lock().unwrap() = on;
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.set_ignore_cursor_events(!on);
        let _ = w.emit_to(LABEL, MODE_EVENT, on);
    }
}

/// The overlay shortcut: take clicks, or let them through again.
pub fn toggle_interactive<R: Runtime>(app: &AppHandle<R>) {
    let Some(w) = app.get_webview_window(LABEL) else { return };
    if !w.is_visible().unwrap_or(false) {
        return;
    }
    let on = !*app.state::<Overlay>().interactive.lock().unwrap();
    set_interactive(app, on);
}

/// Show or hide it (from the main window: in a call, with Rynuvo in the background).
/// Async: a window built from a synchronous command deadlocks on Windows (it runs on the thread the
/// window needs).
#[tauri::command]
pub async fn overlay_show(app: AppHandle, show: bool) -> Result<(), String> {
    if show {
        let w = window(&app).map_err(|e| e.to_string())?;
        let _ = w.show();
    } else if let Some(w) = app.get_webview_window(LABEL) {
        set_interactive(&app, false);
        let _ = w.hide();
    }
    Ok(())
}

/// The page's height, as it lays out (its width is fixed).
#[tauri::command]
pub fn overlay_fit(app: AppHandle, height: f64) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.set_size(LogicalSize::new(WIDTH, height.clamp(40.0, 640.0)));
    }
}

/// The lock button: clicks go through again.
#[tauri::command]
pub fn overlay_lock(app: AppHandle) {
    set_interactive(&app, false);
}

/// Open Rynuvo from the overlay.
#[tauri::command]
pub fn overlay_open(app: AppHandle) {
    set_interactive(&app, false);
    crate::tray::show_main(&app);
}

/// Dragging it by its header (it can't be focused, so the page asks).
#[tauri::command]
pub fn overlay_drag(app: AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.start_dragging();
    }
}
