use axum::Router;
use sqlx::SqlitePool;
use std::sync::Arc;

use crate::{config::Config, disk::DiskState, routes, voice::{VoiceRooms, relay::Relay}, ws::{self, Hub}};

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub cfg: Arc<Config>,
    pub hub: Hub,
    pub disk: DiskState,
    pub voice: VoiceRooms,
    pub relay: Relay,
}

pub fn build_router(state: AppState) -> Router {
    let downloads = tower_http::services::ServeDir::new(state.cfg.downloads_dir());
    Router::new()
        .nest("/api/v1", routes::api(&state.cfg).merge(ws::router()))
        .nest_service("/downloads", downloads)
        .with_state(state)
}
