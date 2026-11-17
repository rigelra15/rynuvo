//! Image commands and the `rynuvo-media://` protocol (authenticated, disk-cached).
use std::path::PathBuf;

use rynuvo_proto::{Id, ImageRef, Message};
use tauri::{AppHandle, Manager, Runtime, State, ipc::Request};

use crate::{commands::Core, core::image::prepare};

fn is_hex_id(id: &str) -> bool {
    id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

fn cache_dir<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    Some(app.path().app_cache_dir().ok()?.join("media"))
}

/// Raw body = image bytes. Compresses, uploads, returns the server ImageRef.
#[tauri::command]
pub async fn upload_image(core: State<'_, Core>, request: Request<'_>) -> Result<ImageRef, String> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("Invalid image data".into());
    };
    let bytes = bytes.clone();
    let prepared = tokio::task::spawn_blocking(move || prepare(&bytes)).await.map_err(|e| e.to_string())??;
    core.api()?.upload_media(prepared.bytes, prepared.mime).await.map_err(|e| e.user_message())
}

#[tauri::command]
pub async fn post_image_message(core: State<'_, Core>, channel: Id, content: String, image_id: String) -> Result<Message, String> {
    core.api()?.post_message(channel, &content, Some(image_id)).await.map_err(|e| e.user_message())
}

async fn get_bytes<R: Runtime>(app: &AppHandle<R>, id: &str) -> Result<(Vec<u8>, String), String> {
    if !is_hex_id(id) {
        return Err("bad id".into());
    }
    let dir = cache_dir(app).ok_or("no cache dir")?;
    let path = dir.join(id);
    let mime_path = dir.join(format!("{id}.mime"));
    if let (Ok(b), Ok(m)) = (tokio::fs::read(&path).await, tokio::fs::read_to_string(&mime_path).await) {
        return Ok((b, m));
    }
    let core = app.state::<Core>();
    let (b, m) = core.api()?.fetch_media(id).await.map_err(|e| e.to_string())?;
    let _ = tokio::fs::create_dir_all(&dir).await;
    let _ = tokio::fs::write(&path, &b).await;
    let _ = tokio::fs::write(&mime_path, &m).await;
    Ok((b, m))
}

/// Save an image to the Downloads folder; returns the path.
#[tauri::command]
pub async fn save_image(app: AppHandle, id: String) -> Result<String, String> {
    let (bytes, mime) = get_bytes(&app, &id).await?;
    let ext = match mime.as_str() {
        "image/gif" => "gif",
        "image/png" => "png",
        "image/jpeg" => "jpg",
        _ => "webp",
    };
    let dir = app.path().download_dir().map_err(|e| e.to_string())?;
    let path = dir.join(format!("rynuvo-{}.{ext}", &id[..12]));
    tokio::fs::write(&path, bytes).await.map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

pub fn protocol<R: Runtime>(
    ctx: tauri::UriSchemeContext<'_, R>,
    request: tauri::http::Request<Vec<u8>>,
    responder: tauri::UriSchemeResponder,
) {
    let app = ctx.app_handle().clone();
    let id = request.uri().path().trim_start_matches('/').to_string();
    tauri::async_runtime::spawn(async move {
        let resp = match get_bytes(&app, &id).await {
            Ok((b, mime)) => tauri::http::Response::builder()
                .header("content-type", mime)
                .header("cache-control", "max-age=31536000, immutable")
                // The app page (another origin) reads pictures too, e.g. for a photo's colour.
                .header("access-control-allow-origin", "*")
                .body(b),
            Err(_) => tauri::http::Response::builder().status(404).body(Vec::new()),
        };
        responder.respond(resp.unwrap());
    });
}
