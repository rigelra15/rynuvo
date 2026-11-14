//! Test helpers (feature `test-util`).
use std::{net::SocketAddr, sync::Arc};

use crate::{
    app::{AppState, build_router},
    config::Config,
    db,
    disk::DiskState,
    ws::Hub,
};

pub struct TestApp {
    pub addr: SocketAddr,
    pub client: reqwest::Client,
    pub state: AppState,
    pub _tmp: tempfile::TempDir,
}

impl TestApp {
    pub async fn spawn() -> TestApp {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = Config {
            bind: "127.0.0.1:0".parse().unwrap(),
            public_url: "http://127.0.0.1".into(),
            public_ip: "127.0.0.1".parse().unwrap(),
            data_dir: tmp.path().to_path_buf(),
            stun_ports: [0, 0],
            disk_warn_pct: 85,
            disk_block_pct: 92,
            max_upload_bytes: 5_242_880,
        };
        std::fs::create_dir_all(cfg.media_dir()).unwrap();
        let pool = db::open_pool("sqlite::memory:").await.unwrap();
        let state = AppState {
            db: pool,
            disk: DiskState::new(cfg.disk_block_pct),
            cfg: Arc::new(cfg),
            hub: Hub::default(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let router = build_router(state.clone());
        tokio::spawn(async move {
            axum::serve(listener, router.into_make_service_with_connect_info::<SocketAddr>()).await.unwrap();
        });
        TestApp { addr, client: reqwest::Client::new(), state, _tmp: tmp }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }
}
