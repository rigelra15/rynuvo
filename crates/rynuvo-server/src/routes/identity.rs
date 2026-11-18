use axum::{Json, Router, extract::State, routing::{get, post}};
use rynuvo_proto::{AVATAR_ICONS, is_avatar_color, ClaimTransferReq, JoinReq, JoinResp, ProfileReq, User, WsServerMsg};
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
        .route("/claim-transfer", post(claim_transfer))
        .route("/me", get(me).patch(update_me))
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
        .bind(if name.is_empty() { "device".to_string() } else { name })
        .bind(now_ms())
        .execute(db)
        .await?;
    Ok(token)
}

fn is_unique_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(d) if d.is_unique_violation())
}

fn nickname_taken() -> AppError {
    AppError::Conflict("nickname_taken", "Nickname is already taken".into())
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
            "Invite code is wrong or no longer valid".into(),
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
    let user = User { id, nickname, is_admin, avatar_icon: None, avatar_color: None, avatar_image: None };
    st.hub.broadcast(WsServerMsg::UserUpserted(user.clone()));
    Ok(Json(JoinResp { token, user }))
}

async fn me(au: AuthUser) -> Json<User> {
    Json(au.user)
}

async fn update_me(State(st): State<AppState>, au: AuthUser, Json(req): Json<ProfileReq>) -> AppResult<Json<User>> {
    let (nickname, key) = match &req.nickname {
        Some(n) => {
            let (nickname, k) = auth::nickname_key(n)?;
            (Some(nickname), Some(k))
        }
        None => (None, None),
    };
    let pick = |v: Option<Option<String>>, allowed: &[&str], what: &str| match v {
        Some(Some(s)) if !allowed.contains(&s.as_str()) => Err(AppError::BadRequest(format!("Unknown avatar {what}"))),
        v => Ok(v),
    };
    let icon = pick(req.avatar_icon, &AVATAR_ICONS, "icon")?;
    let color = match req.avatar_color {
        Some(Some(c)) if !is_avatar_color(&c.to_ascii_lowercase()) => return Err(AppError::BadRequest("Unknown avatar colour".into())),
        v => v.map(|c| c.map(|c| c.to_ascii_lowercase())),
    };
    // A picture must be one already uploaded here.
    if let Some(Some(img)) = &req.avatar_image {
        let found: Option<String> = sqlx::query_scalar("SELECT id FROM images WHERE id = ?").bind(img).fetch_optional(&st.db).await?;
        if found.is_none() {
            return Err(AppError::BadRequest("Unknown picture".into()));
        }
    }
    let image = req.avatar_image;
    // Only the fields that were sent change, so two edits at once (or a rename on another device)
    // never write back a stale copy of the others.
    let row: (i64, String, bool, Option<String>, Option<String>, Option<String>) = sqlx::query_as(
        "UPDATE users SET nickname = COALESCE(?1, nickname), nickname_key = COALESCE(?2, nickname_key), \
         avatar_icon = CASE WHEN ?3 THEN ?4 ELSE avatar_icon END, avatar_color = CASE WHEN ?5 THEN ?6 ELSE avatar_color END, \
         avatar_image = CASE WHEN ?8 THEN ?9 ELSE avatar_image END \
         WHERE id = ?7 RETURNING id, nickname, is_admin, avatar_icon, avatar_color, avatar_image",
    )
    .bind(&nickname)
    .bind(&key)
    .bind(icon.is_some())
    .bind(icon.flatten())
    .bind(color.is_some())
    .bind(color.flatten())
    .bind(au.user.id)
    .bind(image.is_some())
    .bind(image.flatten())
    .fetch_one(&st.db)
    .await
    .map_err(|e| if is_unique_violation(&e) { nickname_taken() } else { AppError::from(e) })?;
    let (id, nickname, is_admin, avatar_icon, avatar_color, avatar_image) = row;
    let user = User { id, nickname, is_admin, avatar_icon, avatar_color, avatar_image };
    st.hub.broadcast(WsServerMsg::UserUpserted(user.clone()));
    Ok(Json(user))
}

async fn claim_transfer(State(st): State<AppState>, Json(req): Json<ClaimTransferReq>) -> AppResult<Json<JoinResp>> {
    let h = auth::code_hash(&req.code);
    let claimed: Option<(i64, i64)> = sqlx::query_as(
        "UPDATE transfer_codes SET used = 1 WHERE code_hash = ? AND used = 0 AND expires_at > ? RETURNING user_id, expires_at",
    )
    .bind(&h[..])
    .bind(now_ms())
    .fetch_optional(&st.db)
    .await?;
    let invalid = || AppError::Coded(axum::http::StatusCode::FORBIDDEN, "invalid_code", "Transfer code is wrong or expired".into());
    let (user_id, code_expires_at) = claimed.ok_or_else(invalid)?;
    let user: Option<(i64, String, bool, Option<String>, Option<String>, Option<String>)> = sqlx::query_as("SELECT id, nickname, is_admin, avatar_icon, avatar_color, avatar_image FROM users WHERE id = ? AND removed_at IS NULL")
        .bind(user_id)
        .fetch_optional(&st.db)
        .await?;
    let (id, nickname, is_admin, avatar_icon, avatar_color, avatar_image) = user.ok_or_else(invalid)?;
    let token = issue_device(&st.db, id, &req.device_name).await?;
    // The device showing the code closes it: "Signed in on Android".
    let device: String = req.device_name.trim().chars().take(64).collect();
    st.hub.send_to_user(id, WsServerMsg::TransferClaimed { expires_at: code_expires_at, device: device.clone() });
    // An admin may have made the code for this user from the admin panel.
    st.hub.send_to_admins(WsServerMsg::TransferClaimed { expires_at: code_expires_at, device });
    Ok(Json(JoinResp { token, user: User { id, nickname, is_admin, avatar_icon, avatar_color, avatar_image } }))
}
