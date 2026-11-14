use axum::{Json, Router, extract::State, routing::{get, post}};
use rynuvo_proto::{JoinReq, JoinResp, RenameReq, User, WsServerMsg};
use sqlx::SqlitePool;

use crate::{
    app::AppState,
    auth::{self, AuthUser},
    error::{AppError, AppResult},
    now_ms,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/join", post(join))
        .route("/me", get(me).patch(rename_me))
}

pub async fn get_setting(db: &SqlitePool, key: &str) -> AppResult<Option<String>> {
    Ok(sqlx::query_scalar("SELECT value FROM settings WHERE key = ?").bind(key).fetch_optional(db).await?)
}

pub async fn set_setting(db: &SqlitePool, key: &str, value: &str) -> AppResult<()> {
    sqlx::query("INSERT INTO settings(key, value) VALUES(?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
        .bind(key)
        .bind(value)
        .execute(db)
        .await?;
    Ok(())
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Store a new invite code (hash only).
pub async fn set_invite(db: &SqlitePool, code: &str) -> AppResult<()> {
    set_setting(db, "invite_hash", &hex(&auth::code_hash(code))).await
}

async fn store_owner_code(db: &SqlitePool) -> AppResult<String> {
    let code = auth::new_human_code();
    set_setting(db, "owner_code_hash", &hex(&auth::code_hash(&code))).await?;
    Ok(code)
}

/// If no admin exists yet and no owner code is pending, create one and return it.
pub async fn ensure_owner_code(db: &SqlitePool) -> AppResult<Option<String>> {
    let admins: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE is_admin = 1 AND removed_at IS NULL")
        .fetch_one(db)
        .await?;
    if admins > 0 || get_setting(db, "owner_code_hash").await?.is_some() {
        return Ok(None);
    }
    Ok(Some(store_owner_code(db).await?))
}

/// Always issue a fresh owner code (CLI `owner-code`).
pub async fn new_owner_code(db: &SqlitePool) -> AppResult<String> {
    store_owner_code(db).await
}

/// Create a device row for `user_id` and return the raw token.
pub async fn issue_device(db: &SqlitePool, user_id: i64, device_name: &str) -> AppResult<String> {
    let token = auth::new_secret();
    let name: String = device_name.trim().chars().take(64).collect();
    sqlx::query("INSERT INTO devices(user_id, token_hash, name, created_at) VALUES(?, ?, ?, ?)")
        .bind(user_id)
        .bind(&auth::hash(&token)[..])
        .bind(if name.is_empty() { "perangkat".to_string() } else { name })
        .bind(now_ms())
        .execute(db)
        .await?;
    Ok(token)
}

fn is_unique_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(d) if d.is_unique_violation())
}

fn nickname_taken() -> AppError {
    AppError::Conflict("nickname_taken", "Nickname sudah dipakai".into())
}

async fn join(State(st): State<AppState>, Json(req): Json<JoinReq>) -> AppResult<Json<JoinResp>> {
    let given = hex(&auth::code_hash(&req.invite_code));
    let owner = get_setting(&st.db, "owner_code_hash").await?;
    let invite = get_setting(&st.db, "invite_hash").await?;
    let is_admin = if owner.as_deref() == Some(given.as_str()) {
        true
    } else if invite.as_deref() == Some(given.as_str()) {
        false
    } else {
        return Err(AppError::Coded(
            axum::http::StatusCode::FORBIDDEN,
            "invalid_invite",
            "Kode undangan salah atau sudah tidak berlaku".into(),
        ));
    };
    let (nickname, key) = auth::nickname_key(&req.nickname)?;

    let mut tx = st.db.begin().await?;
    let id: i64 = sqlx::query_scalar("INSERT INTO users(nickname, nickname_key, is_admin) VALUES(?, ?, ?) RETURNING id")
        .bind(&nickname)
        .bind(&key)
        .bind(is_admin)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| if is_unique_violation(&e) { nickname_taken() } else { e.into() })?;
    if is_admin {
        sqlx::query("DELETE FROM settings WHERE key = 'owner_code_hash'").execute(&mut *tx).await?;
    }
    tx.commit().await?;

    let token = issue_device(&st.db, id, &req.device_name).await?;
    let user = User { id, nickname, is_admin };
    st.hub.broadcast(WsServerMsg::UserUpserted(user.clone()));
    Ok(Json(JoinResp { token, user }))
}

async fn me(au: AuthUser) -> Json<User> {
    Json(au.user)
}

async fn rename_me(State(st): State<AppState>, au: AuthUser, Json(req): Json<RenameReq>) -> AppResult<Json<User>> {
    let (nickname, key) = auth::nickname_key(&req.nickname)?;
    sqlx::query("UPDATE users SET nickname = ?, nickname_key = ? WHERE id = ?")
        .bind(&nickname)
        .bind(&key)
        .bind(au.user.id)
        .execute(&st.db)
        .await
        .map_err(|e| if is_unique_violation(&e) { nickname_taken() } else { AppError::from(e) })?;
    let user = User { nickname, ..au.user };
    st.hub.broadcast(WsServerMsg::UserUpserted(user.clone()));
    Ok(Json(user))
}
