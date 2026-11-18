//! Tauri commands for closed captions.
use std::sync::Arc;

use tauri::State;

use crate::captions::{Captions, Lang};

pub const CAPTIONS_EVENT: &str = "rynuvo://captions";

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptionsInfo {
    on: bool,
    /// The model is on this device already (turning on is quick, no download).
    has_model: bool,
}

#[tauri::command]
pub fn captions_info(c: State<'_, Arc<Captions>>) -> CaptionsInfo {
    CaptionsInfo { on: crate::captions::active(), has_model: c.has_model() }
}

/// Turn captions on (downloading the model the first time); progress and lines come as events.
#[tauri::command]
pub fn captions_start(c: State<'_, Arc<Captions>>, lang: String) {
    let c = c.inner().clone();
    tauri::async_runtime::spawn(async move { c.start(Lang::parse(&lang)).await });
}

#[tauri::command]
pub fn captions_stop(c: State<'_, Arc<Captions>>) {
    c.stop();
}

#[tauri::command]
pub fn captions_set_lang(c: State<'_, Arc<Captions>>, lang: String) {
    c.set_lang(Lang::parse(&lang));
}
