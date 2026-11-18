//! A phone signs up for push, signs off, and fetches what a push is about.
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use rynuvo_proto::{PushNote, PushRegisterReq, PushRegisterResp};

use crate::{
    app::AppState,
    auth::{self, AuthUser},
    error::{AppError, AppResult},
    now_ms,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/me/push", post(register).delete(unregister))
        .route("/push/notes/{id}", get(note))
        .route("/push/avatar/{user}", get(avatar))
        .route("/push/media/{id}", get(media))
        .route("/push/reply", post(reply))
}

async fn register(State(st): State<AppState>, au: AuthUser, Json(req): Json<PushRegisterReq>) -> AppResult<Json<PushRegisterResp>> {
    let token = req.token.trim();
    if token.is_empty() || token.len() > 4096 {
        return Err(AppError::BadRequest("bad push token".into()));
    }
    let key = auth::new_secret();
    // A token belongs to one install: if it was signed in as someone else before, that goes.
    sqlx::query("DELETE FROM push_tokens WHERE token = ? AND device_id != ?").bind(token).bind(au.device_id).execute(&st.db).await?;
    sqlx::query("INSERT OR REPLACE INTO push_tokens(device_id, user_id, token, key_hash, created_at) VALUES(?, ?, ?, ?, ?)")
        .bind(au.device_id)
        .bind(au.user.id)
        .bind(token)
        .bind(&auth::hash(&key)[..])
        .bind(now_ms())
        .execute(&st.db)
        .await?;
    Ok(Json(PushRegisterResp { key }))
}

async fn unregister(State(st): State<AppState>, au: AuthUser) -> AppResult<StatusCode> {
    sqlx::query("DELETE FROM push_tokens WHERE device_id = ?").bind(au.device_id).execute(&st.db).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The push key's user, from the `x-push-key` header.
async fn key_user(st: &AppState, headers: &HeaderMap) -> AppResult<i64> {
    let key = headers.get("x-push-key").and_then(|v| v.to_str().ok()).ok_or(AppError::Unauthorized)?;
    let user: Option<i64> = sqlx::query_scalar("SELECT user_id FROM push_tokens WHERE key_hash = ?").bind(&auth::hash(key)[..]).fetch_optional(&st.db).await?;
    user.ok_or(AppError::Unauthorized)
}

/// Someone's profile photo, for their face on a notification.
async fn avatar(State(st): State<AppState>, headers: HeaderMap, Path(user): Path<i64>) -> AppResult<impl IntoResponse> {
    key_user(&st, &headers).await?;
    let image: Option<String> = sqlx::query_scalar("SELECT avatar_image FROM users WHERE id = ?").bind(user).fetch_optional(&st.db).await?.flatten();
    let id = image.ok_or(AppError::NotFound)?;
    let mime: String = sqlx::query_scalar("SELECT mime FROM images WHERE id = ?").bind(&id).fetch_optional(&st.db).await?.ok_or(AppError::NotFound)?;
    let bytes = tokio::fs::read(st.cfg.media_dir().join(&id)).await.map_err(|_| AppError::NotFound)?;
    Ok(([(header::CONTENT_TYPE, mime)], bytes))
}

/// A message's picture, for its notification.
async fn media(State(st): State<AppState>, headers: HeaderMap, Path(id): Path<String>) -> AppResult<impl IntoResponse> {
    key_user(&st, &headers).await?;
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::NotFound);
    }
    let mime: String = sqlx::query_scalar("SELECT mime FROM images WHERE id = ?").bind(&id).fetch_optional(&st.db).await?.ok_or(AppError::NotFound)?;
    let bytes = tokio::fs::read(st.cfg.media_dir().join(&id)).await.map_err(|_| AppError::NotFound)?;
    Ok(([(header::CONTENT_TYPE, mime)], bytes))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReplyReq {
    channel_id: i64,
    content: String,
}

/// A reply typed into a message notification, sent as the phone's user.
async fn reply(State(st): State<AppState>, headers: HeaderMap, Json(req): Json<ReplyReq>) -> AppResult<StatusCode> {
    let user = key_user(&st, &headers).await?;
    crate::routes::messages::post(&st, user, req.channel_id, rynuvo_proto::PostMessageReq { content: req.content, image_id: None }).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The notification a push was about. Authorized by the phone's push key (header `x-push-key`).
async fn note(State(st): State<AppState>, headers: HeaderMap, Path(id): Path<String>) -> AppResult<Json<PushNote>> {
    let user = key_user(&st, &headers).await?;
    st.push.note(&id, user).map(Json).ok_or(AppError::NotFound)
}
