//! Settings-page diagnostics: connection test, log upload, version.
use tauri::{AppHandle, State};

use crate::{
    commands::Core,
    core::{logs, nettest},
};

const STUN_PORTS: [u16; 2] = [3478, 3479];

#[tauri::command]
pub async fn run_nettest(core: State<'_, Core>) -> Result<nettest::NetReport, String> {
    let api = core.api()?;
    let host = url::Url::parse(&api.base()).ok().and_then(|u| u.host_str().map(|h| h.trim_matches(['[', ']']).to_string()));
    let host = host.ok_or("Invalid server address")?;
    let report = nettest::run(&api, &host, STUN_PORTS).await;
    tracing::info!(mapping = ?report.mapping, udp = report.udp_ok, ipv6 = report.ipv6_ok, mbps = ?report.upload_mbps, "nettest");
    let json = serde_json::to_string(&report).map_err(|e| e.to_string())?;
    api.report("nettest", &json).await.map_err(|e| e.user_message())?;
    Ok(report)
}

#[tauri::command]
pub async fn send_log(core: State<'_, Core>) -> Result<(), String> {
    let tail = logs::collect_tail(1024 * 1024);
    if tail.is_empty() {
        return Err("No log to send yet".into());
    }
    core.api()?.report("log", &tail).await.map_err(|e| e.user_message())
}

#[tauri::command]
pub fn app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}
