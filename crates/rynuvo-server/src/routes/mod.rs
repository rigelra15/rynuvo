use axum::{Json, Router, routing::get};
use rynuvo_proto::PROTOCOL_VERSION;

use crate::app::AppState;

pub mod identity;

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"ok": true, "protocol": PROTOCOL_VERSION}))
}

pub fn api() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .merge(identity::router())
}
