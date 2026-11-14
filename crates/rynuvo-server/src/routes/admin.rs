use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, post},
};
use rynuvo_proto::{RotateInviteResp, TransferCodeResp, WsServerMsg};

use crate::{
    app::AppState,
    auth::{self, AuthUser},
    error::{AppError, AppResult},
    now_ms,
    routes::{channels::load_channel, identity},
};

pub const TRANSFER_TTL_MS: i64 = 10 * 60 * 1000;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/invite", post(rotate_invite))
        .route("/admin/users/{id}", delete(kick))
        .route("/admin/users/{id}/transfer-code", post(admin_transfer_code))
        .route("/me/transfer-code", post(my_transfer_code))
}

async fn rotate_invite(State(st): State<AppState>, au: AuthUser) -> AppResult<Json<RotateInviteResp>> {
    au.require_admin()?;
    let code = auth::new_human_code();
    identity::set_invite(&st.db, &code).await?;
    Ok(Json(RotateInviteResp { code }))
}

async fn kick(State(st): State<AppState>, au: AuthUser, Path(id): Path<i64>) -> AppResult<StatusCode> {
    au.require_admin()?;
    if id == au.user.id {
        return Err(AppError::BadRequest("You can't remove yourself".into()));
    }
    let mut tx = st.db.begin().await?;
    let n = sqlx::query("UPDATE users SET removed_at = ? WHERE id = ? AND removed_at IS NULL")
        .bind(now_ms())
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(AppError::NotFound);
    }
    sqlx::query("DELETE FROM devices WHERE user_id = ?").bind(id).execute(&mut *tx).await?;
    let moved: Vec<i64> = sqlx::query_scalar("UPDATE channels SET owner_id = ? WHERE owner_id = ? RETURNING id")
        .bind(au.user.id)
        .bind(id)
        .fetch_all(&mut *tx)
        .await?;
    tx.commit().await?;

    st.hub.kick_user(id);
    st.hub.broadcast(WsServerMsg::UserRemoved { id });
    for cid in moved {
        if let Some(ch) = load_channel(&st.db, cid).await? {
            st.hub.broadcast(WsServerMsg::ChannelUpdated(ch));
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn issue_transfer(st: &AppState, user_id: i64) -> AppResult<TransferCodeResp> {
    let code = auth::new_human_code();
    let expires_at = now_ms() + TRANSFER_TTL_MS;
    sqlx::query("INSERT INTO transfer_codes(code_hash, user_id, expires_at) VALUES(?, ?, ?)")
        .bind(&auth::code_hash(&code)[..])
        .bind(user_id)
        .bind(expires_at)
        .execute(&st.db)
        .await?;
    Ok(TransferCodeResp { code, expires_at })
}

async fn my_transfer_code(State(st): State<AppState>, au: AuthUser) -> AppResult<Json<TransferCodeResp>> {
    Ok(Json(issue_transfer(&st, au.user.id).await?))
}

async fn admin_transfer_code(State(st): State<AppState>, au: AuthUser, Path(id): Path<i64>) -> AppResult<Json<TransferCodeResp>> {
    au.require_admin()?;
    let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM users WHERE id = ? AND removed_at IS NULL")
        .bind(id)
        .fetch_optional(&st.db)
        .await?;
    exists.ok_or(AppError::NotFound)?;
    Ok(Json(issue_transfer(&st, id).await?))
}

