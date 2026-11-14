use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, patch},
};
use rynuvo_proto::{Channel, ChannelKind, CreateChannelReq, UpdateChannelReq, User, WsServerMsg};
use sqlx::SqlitePool;

use crate::{
    app::AppState,
    auth::AuthUser,
    error::{AppError, AppResult},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/channels", get(list).post(create))
        .route("/channels/{id}", patch(update).delete(remove))
}

type ChannelRow = (i64, String, String, Option<String>, i64, i64);
macro_rules! select_channels {
    ($rest:literal) => {
        concat!("SELECT id, kind, name, topic, owner_id, position FROM channels ", $rest)
    };
}

pub(crate) fn row_to_channel((id, kind, name, topic, owner_id, position): ChannelRow) -> Channel {
    Channel { id, kind: ChannelKind::parse(&kind).unwrap_or(ChannelKind::Text), name, topic, owner_id, position }
}

pub async fn load_channel(db: &SqlitePool, id: i64) -> AppResult<Option<Channel>> {
    let row: Option<ChannelRow> = sqlx::query_as(select_channels!("WHERE id = ?")).bind(id).fetch_optional(db).await?;
    Ok(row.map(row_to_channel))
}

pub async fn list_channels(db: &SqlitePool) -> AppResult<Vec<Channel>> {
    let rows: Vec<ChannelRow> = sqlx::query_as(select_channels!(
        "ORDER BY CASE kind WHEN 'text' THEN 0 ELSE 1 END, position, id"
    ))
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(row_to_channel).collect())
}

/// Channel owner or super-admin may manage a channel.
pub fn can_manage(user: &User, ch: &Channel) -> bool {
    user.is_admin || ch.owner_id == user.id
}

/// Trim and validate a channel name; text channels become `lower-kebab`.
pub fn normalize_name(kind: ChannelKind, raw: &str) -> AppResult<String> {
    let t = raw.trim();
    let name = match kind {
        ChannelKind::Text => t.to_lowercase().split_whitespace().collect::<Vec<_>>().join("-"),
        ChannelKind::Voice => t.to_string(),
    };
    let len = name.chars().count();
    if !(1..=40).contains(&len) || name.chars().any(|c| c.is_control()) {
        return Err(AppError::BadRequest("Channel name must be 1–40 characters".into()));
    }
    Ok(name)
}

async fn list(State(st): State<AppState>, _au: AuthUser) -> AppResult<Json<Vec<Channel>>> {
    Ok(Json(list_channels(&st.db).await?))
}

async fn create(State(st): State<AppState>, au: AuthUser, Json(req): Json<CreateChannelReq>) -> AppResult<Json<Channel>> {
    let name = normalize_name(req.kind, &req.name)?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO channels(kind, name, owner_id, position)
         VALUES(?1, ?2, ?3, (SELECT COALESCE(MAX(position), 0) + 1 FROM channels WHERE kind = ?1)) RETURNING id",
    )
    .bind(req.kind.as_str())
    .bind(&name)
    .bind(au.user.id)
    .fetch_one(&st.db)
    .await?;
    let ch = load_channel(&st.db, id).await?.ok_or(AppError::NotFound)?;
    st.hub.broadcast(WsServerMsg::ChannelCreated(ch.clone()));
    Ok(Json(ch))
}

async fn managed(st: &AppState, au: &AuthUser, id: i64) -> AppResult<Channel> {
    let ch = load_channel(&st.db, id).await?.ok_or(AppError::NotFound)?;
    if !can_manage(&au.user, &ch) {
        return Err(AppError::Forbidden);
    }
    Ok(ch)
}

async fn update(
    State(st): State<AppState>,
    au: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<UpdateChannelReq>,
) -> AppResult<Json<Channel>> {
    let mut ch = managed(&st, &au, id).await?;
    if let Some(n) = &req.name {
        ch.name = normalize_name(ch.kind, n)?;
    }
    if let Some(topic) = req.topic {
        ch.topic = topic.map(|t| t.trim().chars().take(200).collect::<String>()).filter(|t| !t.is_empty());
    }
    if let Some(p) = req.position {
        ch.position = p;
    }
    sqlx::query("UPDATE channels SET name = ?, topic = ?, position = ? WHERE id = ?")
        .bind(&ch.name)
        .bind(&ch.topic)
        .bind(ch.position)
        .bind(id)
        .execute(&st.db)
        .await?;
    st.hub.broadcast(WsServerMsg::ChannelUpdated(ch.clone()));
    Ok(Json(ch))
}

async fn remove(State(st): State<AppState>, au: AuthUser, Path(id): Path<i64>) -> AppResult<StatusCode> {
    managed(&st, &au, id).await?;
    sqlx::query("DELETE FROM channels WHERE id = ?").bind(id).execute(&st.db).await?;
    st.hub.broadcast(WsServerMsg::ChannelDeleted { id });
    crate::voice::channel_deleted(&st, id);
    Ok(StatusCode::NO_CONTENT)
}
