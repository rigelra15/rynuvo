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

    pub async fn set_invite(&self, code: &str) {
        crate::routes::identity::set_invite(&self.state.db, code).await.unwrap();
    }

    pub const INVITE: &'static str = "TEST-TEST-TEST";

    pub async fn join_raw(&self, code: &str, nick: &str) -> reqwest::Response {
        self.client
            .post(self.url("/api/v1/join"))
            .json(&serde_json::json!({"inviteCode": code, "nickname": nick, "deviceName": "test"}))
            .send()
            .await
            .unwrap()
    }

    /// Join as super-admin "Admin" via a fresh owner code.
    pub async fn admin(&self) -> rynuvo_proto::JoinResp {
        let code = crate::routes::identity::new_owner_code(&self.state.db).await.unwrap();
        let r = self.join_raw(&code, "Admin").await;
        assert_eq!(r.status(), 200);
        r.json().await.unwrap()
    }

    /// Join as a regular user with [`Self::INVITE`] (sets it if unset).
    pub async fn user(&self, nick: &str) -> rynuvo_proto::JoinResp {
        if crate::routes::identity::get_setting(&self.state.db, "invite_hash").await.unwrap().is_none() {
            self.set_invite(Self::INVITE).await;
        }
        let r = self.join_raw(Self::INVITE, nick).await;
        assert_eq!(r.status(), 200, "join {nick}");
        r.json().await.unwrap()
    }

    pub async fn create_channel(&self, token: &str, kind: &str, name: &str) -> rynuvo_proto::Channel {
        let r = self
            .client
            .post(self.url("/api/v1/channels"))
            .bearer_auth(token)
            .json(&serde_json::json!({"kind": kind, "name": name}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200, "create channel {name}");
        r.json().await.unwrap()
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }
}
