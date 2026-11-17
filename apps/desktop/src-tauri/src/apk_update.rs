//! Android self-update: the desktop updater plugin doesn't run on Android, so the app reads the
//! same `latest.json`, downloads the APK it lists (with progress for the splash), checks it against
//! the updater's signing key, and hands it to the system installer (which asks the user to confirm).

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Runtime};

/// Where every installed app looks for a new version (the same file the desktop updater reads).
const LATEST: &str = "https://rynuvo.pokyvl.my.id/downloads/latest.json";
/// The platform key in `latest.json` for the Android build.
const PLATFORM: &str = "android-aarch64";
pub const PROGRESS_EVENT: &str = "apk-update-progress";

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApkUpdate {
    pub version: String,
    pub notes: Option<String>,
    pub url: String,
    pub signature: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Progress {
    received: u64,
    total: Option<u64>,
}

#[derive(Deserialize)]
struct Latest {
    version: String,
    notes: Option<String>,
    #[serde(default)]
    platforms: std::collections::HashMap<String, Platform>,
}

#[derive(Deserialize)]
struct Platform {
    url: String,
    signature: String,
}

/// `a` is a later version than `b` ("0.10.0" > "0.9.1"; a leading "v" and any "-pre" are ignored).
pub fn newer(a: &str, b: &str) -> bool {
    let parts = |s: &str| -> Vec<u64> {
        s.trim_start_matches('v').split(['-', '+']).next().unwrap_or("").split('.').map(|p| p.parse().unwrap_or(0)).collect()
    };
    let (mut x, mut y) = (parts(a), parts(b));
    let n = x.len().max(y.len());
    x.resize(n, 0);
    y.resize(n, 0);
    x > y
}

/// The update for this device in `latest.json`, if it's newer than `current`.
fn pick(doc: Latest, current: &str) -> Option<ApkUpdate> {
    if !newer(&doc.version, current) {
        return None;
    }
    let p = doc.platforms.into_iter().find(|(k, _)| k == PLATFORM)?.1;
    Some(ApkUpdate { version: doc.version, notes: doc.notes, url: p.url, signature: p.signature })
}

fn client() -> Result<reqwest::Client, String> {
    crate::core::http::tls_roots(reqwest::Client::builder())
        .connect_timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())
}

/// A newer Android build, or None (up to date, or nothing published for Android).
#[tauri::command]
pub async fn apk_update_check<R: Runtime>(app: AppHandle<R>) -> Result<Option<ApkUpdate>, String> {
    let doc: Latest = client()?
        .get(LATEST)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    Ok(pick(doc, &app.package_info().version.to_string()))
}

/// Download the APK (reporting progress), verify it, and return its path for the installer.
#[tauri::command]
pub async fn apk_update_download<R: Runtime>(app: AppHandle<R>, url: String, signature: String) -> Result<String, String> {
    use futures_util::StreamExt;
    use tauri::Manager;
    let resp = client()?.get(&url).send().await.and_then(|r| r.error_for_status()).map_err(|e| e.to_string())?;
    let total = resp.content_length();
    let mut body = Vec::with_capacity(total.unwrap_or(0) as usize);
    let mut stream = resp.bytes_stream();
    let mut last_emit = 0u64;
    while let Some(chunk) = stream.next().await {
        body.extend_from_slice(&chunk.map_err(|e| e.to_string())?);
        let received = body.len() as u64;
        // About every 64 KB, plus the end: smooth enough for a progress bar, light on events.
        if received - last_emit >= 64 * 1024 || Some(received) == total {
            last_emit = received;
            let _ = app.emit(PROGRESS_EVENT, Progress { received, total });
        }
    }
    verify(&app, &body, &signature)?;
    let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("update");
    tokio::fs::create_dir_all(&dir).await.map_err(|e| e.to_string())?;
    let path = dir.join("Rynuvo.apk");
    tokio::fs::write(&path, &body).await.map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

/// The APK must be signed with the updater's key (the same one desktop updates are checked with).
fn verify<R: Runtime>(app: &AppHandle<R>, body: &[u8], signature: &str) -> Result<(), String> {
    let pubkey = app
        .config()
        .plugins
        .0
        .get("updater")
        .and_then(|u| u.get("pubkey"))
        .and_then(|k| k.as_str())
        .ok_or("No update key in the app")?;
    check_signature(pubkey, signature, body)
}

/// `pubkey` and `signature` as Tauri writes them: base64 of minisign's text files.
#[cfg(target_os = "android")]
pub fn check_signature(pubkey: &str, signature: &str, body: &[u8]) -> Result<(), String> {
    use base64::Engine;
    let text = |b64: &str| -> Result<String, String> {
        let raw = base64::engine::general_purpose::STANDARD.decode(b64.trim()).map_err(|_| "Bad update signature".to_string())?;
        String::from_utf8(raw).map_err(|_| "Bad update signature".to_string())
    };
    let key = minisign_verify::PublicKey::decode(&text(pubkey)?).map_err(|_| "Bad update key")?;
    let sig = minisign_verify::Signature::decode(&text(signature)?).map_err(|_| "Bad update signature")?;
    key.verify(body, &sig, true).map_err(|_| "The download isn't a genuine Rynuvo update".to_string())
}

/// Elsewhere the updater plugin does its own checking; this path is never taken.
#[cfg(not(target_os = "android"))]
pub fn check_signature(_pubkey: &str, _signature: &str, _body: &[u8]) -> Result<(), String> {
    Err("Only used on Android".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_number_not_text() {
        assert!(newer("0.10.0", "0.9.1"));
        assert!(newer("v1.0.0", "0.99.99"));
        assert!(newer("0.9.2", "0.9.1"));
        assert!(!newer("0.9.1", "0.9.1"));
        assert!(!newer("0.9.0", "0.9.1"));
        assert!(!newer("0.9.1-beta", "0.9.1"));
        assert!(newer("0.9.1.1", "0.9.1"));
    }

    #[test]
    fn picks_the_android_build_only_when_newer() {
        let doc = |v: &str, with_android: bool| -> Latest {
            let mut platforms = std::collections::HashMap::new();
            platforms.insert("darwin-aarch64".into(), Platform { url: "mac".into(), signature: "s1".into() });
            if with_android {
                platforms.insert(PLATFORM.into(), Platform { url: "apk".into(), signature: "s2".into() });
            }
            Latest { version: v.into(), notes: Some("n".into()), platforms }
        };
        assert_eq!(
            pick(doc("0.10.0", true), "0.9.1"),
            Some(ApkUpdate { version: "0.10.0".into(), notes: Some("n".into()), url: "apk".into(), signature: "s2".into() })
        );
        assert_eq!(pick(doc("0.9.1", true), "0.9.1"), None, "same version");
        assert_eq!(pick(doc("0.10.0", false), "0.9.1"), None, "no Android build published");
    }
}
