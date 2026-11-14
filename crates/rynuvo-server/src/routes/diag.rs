use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::time::Instant;

use crate::{
    app::AppState,
    auth::AuthUser,
    error::{AppError, AppResult},
    now_ms,
};

const UPLOAD_TEST_MAX: usize = 8 * 1024 * 1024;
const REPORT_MAX: usize = 2 * 1024 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/diag/upload-test", post(upload_test).layer(DefaultBodyLimit::max(UPLOAD_TEST_MAX)))
        .route("/diag/report", post(report).layer(DefaultBodyLimit::max(REPORT_MAX + 1024)))
        .route("/diag/reports", get(reports))
}

async fn upload_test(_au: AuthUser, body: Bytes) -> Json<serde_json::Value> {
    let t = Instant::now();
    let n = body.len();
    Json(serde_json::json!({"bytes": n, "ms": t.elapsed().as_millis() as u64}))
}

#[derive(Deserialize)]
struct ReportReq {
    kind: String,
    body: String,
}

fn valid_kind(k: &str) -> bool {
    matches!(k, "nettest" | "log")
}

async fn report(State(st): State<AppState>, au: AuthUser, Json(req): Json<ReportReq>) -> AppResult<StatusCode> {
    if !valid_kind(&req.kind) || req.body.len() > REPORT_MAX {
        return Err(AppError::BadRequest("Laporan tidak valid".into()));
    }
    sqlx::query("INSERT INTO diag_reports(user_id, kind, body, created_at) VALUES(?, ?, ?, ?)")
        .bind(au.user.id)
        .bind(&req.kind)
        .bind(&req.body)
        .bind(now_ms())
        .execute(&st.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct ReportsQ {
    kind: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
struct ReportRow {
    id: i64,
    user_id: i64,
    nickname: String,
    kind: String,
    body: String,
    created_at: i64,
}

async fn reports(State(st): State<AppState>, au: AuthUser, Query(q): Query<ReportsQ>) -> AppResult<Json<Vec<ReportRow>>> {
    au.require_admin()?;
    let rows: Vec<ReportRow> = sqlx::query_as(
        "SELECT r.id, r.user_id, u.nickname, r.kind, r.body, r.created_at FROM diag_reports r JOIN users u ON u.id = r.user_id
         WHERE (?1 IS NULL OR r.kind = ?1) ORDER BY r.id DESC LIMIT 50",
    )
    .bind(q.kind)
    .fetch_all(&st.db)
    .await?;
    Ok(Json(rows))
}
