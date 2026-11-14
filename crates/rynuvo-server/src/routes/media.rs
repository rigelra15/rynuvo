use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, header},
    response::IntoResponse,
    routing::{get, post},
};
use rynuvo_proto::ImageRef;
use sha2::{Digest, Sha256};

use crate::{
    app::AppState,
    auth::AuthUser,
    error::{AppError, AppResult},
    now_ms,
};

pub fn router(max_upload: u64) -> Router<AppState> {
    Router::new()
        .route("/media", post(upload).layer(DefaultBodyLimit::max(max_upload as usize + 1)))
        .route("/media/{id}", get(fetch))
}

fn expected_type(mime: &str) -> Option<imagesize::ImageType> {
    use imagesize::ImageType::*;
    match mime {
        "image/webp" => Some(Webp),
        "image/png" => Some(Png),
        "image/jpeg" => Some(Jpeg),
        "image/gif" => Some(Gif),
        _ => None,
    }
}

/// Extra header checks beyond magic bytes (imagesize trusts fixed offsets).
fn structure_ok(t: imagesize::ImageType, b: &[u8]) -> bool {
    use imagesize::ImageType::*;
    match t {
        Png => b.len() > 24 && &b[12..16] == b"IHDR",
        Webp => b.len() > 16 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" && &b[12..15] == b"VP8",
        Gif => b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a"),
        Jpeg => b.starts_with(&[0xFF, 0xD8, 0xFF]),
        _ => false,
    }
}

fn is_hex_id(id: &str) -> bool {
    id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

async fn upload(State(st): State<AppState>, au: AuthUser, headers: HeaderMap, body: Bytes) -> AppResult<Json<ImageRef>> {
    if st.disk.blocked() {
        return Err(AppError::DiskFull);
    }
    if body.len() as u64 > st.cfg.max_upload_bytes {
        return Err(AppError::TooLarge);
    }
    let mime = headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let bad = || AppError::BadRequest("File bukan gambar yang didukung".into());
    let want = expected_type(&mime).ok_or_else(bad)?;
    let got = imagesize::image_type(&body).map_err(|_| bad())?;
    if got != want || !structure_ok(got, &body) {
        return Err(bad());
    }
    let size = imagesize::blob_size(&body).map_err(|_| bad())?;
    if size.width == 0 || size.height == 0 || size.width > 16384 || size.height > 16384 {
        return Err(bad());
    }
    let id: String = Sha256::digest(&body).iter().map(|b| format!("{b:02x}")).collect();

    let path = st.cfg.media_dir().join(&id);
    if !path.exists() {
        let tmp = st.cfg.media_dir().join(format!("{id}.tmp"));
        tokio::fs::write(&tmp, &body).await.map_err(anyhow::Error::from)?;
        tokio::fs::rename(&tmp, &path).await.map_err(anyhow::Error::from)?;
    }
    sqlx::query("INSERT OR IGNORE INTO images(id, mime, width, height, bytes, uploader_id, created_at) VALUES(?, ?, ?, ?, ?, ?, ?)")
        .bind(&id)
        .bind(&mime)
        .bind(size.width as i64)
        .bind(size.height as i64)
        .bind(body.len() as i64)
        .bind(au.user.id)
        .bind(now_ms())
        .execute(&st.db)
        .await?;
    let (mime, w, h): (String, i64, i64) = sqlx::query_as("SELECT mime, width, height FROM images WHERE id = ?")
        .bind(&id)
        .fetch_one(&st.db)
        .await?;
    Ok(Json(ImageRef { id, width: w as u32, height: h as u32, mime }))
}

async fn fetch(State(st): State<AppState>, _au: AuthUser, Path(id): Path<String>) -> AppResult<impl IntoResponse> {
    if !is_hex_id(&id) {
        return Err(AppError::NotFound);
    }
    let mime: Option<String> = sqlx::query_scalar("SELECT mime FROM images WHERE id = ?").bind(&id).fetch_optional(&st.db).await?;
    let mime = mime.ok_or(AppError::NotFound)?;
    let bytes = tokio::fs::read(st.cfg.media_dir().join(&id)).await.map_err(|_| AppError::NotFound)?;
    Ok((
        [(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "private, max-age=31536000, immutable".to_string())],
        bytes,
    ))
}
