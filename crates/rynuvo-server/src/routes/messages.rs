use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, patch},
};
use rynuvo_proto::{ChannelKind, EditMessageReq, ImageRef, Message, PostMessageReq, WsServerMsg};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::{
    app::AppState,
    auth::AuthUser,
    error::{AppError, AppResult},
    now_ms,
    routes::channels::{can_manage, load_channel},
};

pub const MAX_CHARS: usize = 4000;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/channels/{id}/messages", get(list).post(create))
        .route("/messages/{id}", patch(edit).delete(remove))
}

type Row = (i64, i64, i64, String, i64, Option<i64>, Option<String>, Option<i64>, Option<i64>, Option<String>);

macro_rules! select_messages {
    ($rest:literal) => {
        concat!(
            "SELECT m.id, m.channel_id, m.author_id, m.content, m.created_at, m.edited_at, i.id, i.width, i.height, i.mime
             FROM messages m LEFT JOIN images i ON i.id = m.image_id ",
            $rest
        )
    };
}

fn to_msg((id, channel_id, author_id, content, created_at, edited_at, iid, w, h, mime): Row) -> Message {
    let image = match (iid, w, h, mime) {
        (Some(id), Some(w), Some(h), Some(mime)) => Some(ImageRef { id, width: w as u32, height: h as u32, mime }),
        _ => None,
    };
    Message { id, channel_id, author_id, content, image, created_at, edited_at }
}

pub async fn load_message(db: &SqlitePool, id: i64) -> AppResult<Option<Message>> {
    let r: Option<Row> = sqlx::query_as(select_messages!("WHERE m.id = ?")).bind(id).fetch_optional(db).await?;
    Ok(r.map(to_msg))
}

#[derive(Deserialize)]
struct ListQ {
    before: Option<i64>,
    after: Option<i64>,
    limit: Option<i64>,
}

async fn list(State(st): State<AppState>, _au: AuthUser, Path(ch): Path<i64>, Query(q): Query<ListQ>) -> AppResult<Json<Vec<Message>>> {
    load_channel(&st.db, ch).await?.ok_or(AppError::NotFound)?;
    let limit = q.limit.unwrap_or(50).clamp(1, 100);
    let rows: Vec<Row> = if let Some(after) = q.after {
        sqlx::query_as(select_messages!("WHERE m.channel_id = ? AND m.id > ? ORDER BY m.id ASC LIMIT ?"))
            .bind(ch)
            .bind(after)
            .bind(limit)
            .fetch_all(&st.db)
            .await?
    } else {
        let mut r: Vec<Row> = sqlx::query_as(select_messages!("WHERE m.channel_id = ? AND m.id < ? ORDER BY m.id DESC LIMIT ?"))
            .bind(ch)
            .bind(q.before.unwrap_or(i64::MAX))
            .bind(limit)
            .fetch_all(&st.db)
            .await?;
        r.reverse();
        r
    };
    Ok(Json(rows.into_iter().map(to_msg).collect()))
}

fn clean_content(raw: &str) -> AppResult<String> {
    let c = raw.trim().to_string();
    if c.chars().count() > MAX_CHARS {
        return Err(AppError::Coded(StatusCode::BAD_REQUEST, "too_long", "Message is too long (max 4000 characters)".into()));
    }
    Ok(c)
}

async fn create(State(st): State<AppState>, au: AuthUser, Path(ch): Path<i64>, Json(req): Json<PostMessageReq>) -> AppResult<Json<Message>> {
    let channel = load_channel(&st.db, ch).await?.ok_or(AppError::NotFound)?;
    if channel.kind != ChannelKind::Text {
        return Err(AppError::BadRequest("Can't send messages to a voice channel".into()));
    }
    let content = clean_content(&req.content)?;
    if let Some(img) = &req.image_id {
        let ok: Option<String> = sqlx::query_scalar("SELECT id FROM images WHERE id = ?").bind(img).fetch_optional(&st.db).await?;
        ok.ok_or_else(|| AppError::BadRequest("Image not found".into()))?;
    } else if content.is_empty() {
        return Err(AppError::BadRequest("Message is empty".into()));
    }
    let id: i64 = sqlx::query_scalar("INSERT INTO messages(channel_id, author_id, content, image_id, created_at) VALUES(?, ?, ?, ?, ?) RETURNING id")
        .bind(ch)
        .bind(au.user.id)
        .bind(&content)
        .bind(&req.image_id)
        .bind(now_ms())
        .fetch_one(&st.db)
        .await?;
    let m = load_message(&st.db, id).await?.ok_or(AppError::NotFound)?;
    st.hub.broadcast(WsServerMsg::MessageCreated(m.clone()));
    Ok(Json(m))
}

async fn edit(State(st): State<AppState>, au: AuthUser, Path(id): Path<i64>, Json(req): Json<EditMessageReq>) -> AppResult<Json<Message>> {
    let m = load_message(&st.db, id).await?.ok_or(AppError::NotFound)?;
    if m.author_id != au.user.id {
        return Err(AppError::Forbidden);
    }
    let content = clean_content(&req.content)?;
    if content.is_empty() && m.image.is_none() {
        return Err(AppError::BadRequest("Message is empty".into()));
    }
    sqlx::query("UPDATE messages SET content = ?, edited_at = ? WHERE id = ?")
        .bind(&content)
        .bind(now_ms())
        .bind(id)
        .execute(&st.db)
        .await?;
    let m = load_message(&st.db, id).await?.ok_or(AppError::NotFound)?;
    st.hub.broadcast(WsServerMsg::MessageUpdated(m.clone()));
    Ok(Json(m))
}

async fn remove(State(st): State<AppState>, au: AuthUser, Path(id): Path<i64>) -> AppResult<StatusCode> {
    let m = load_message(&st.db, id).await?.ok_or(AppError::NotFound)?;
    if m.author_id != au.user.id {
        let ch = load_channel(&st.db, m.channel_id).await?.ok_or(AppError::NotFound)?;
        if !can_manage(&au.user, &ch) {
            return Err(AppError::Forbidden);
        }
    }
    sqlx::query("DELETE FROM messages WHERE id = ?").bind(id).execute(&st.db).await?;
    st.hub.broadcast(WsServerMsg::MessageDeleted { id, channel_id: m.channel_id });
    Ok(StatusCode::NO_CONTENT)
}
