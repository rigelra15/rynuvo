use axum::Router;
use sqlx::SqlitePool;
use std::sync::Arc;

use crate::{config::Config, disk::DiskState, routes, ws::{self, Hub}};

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub cfg: Arc<Config>,
    pub hub: Hub,
    pub disk: DiskState,
}

pub fn build_router(state: AppState) -> Router {
    Router::new().nest("/api/v1", routes::api(&state.cfg).merge(ws::router())).with_state(state)
}
