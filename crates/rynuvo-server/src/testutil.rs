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
    /// STUN + voice relay UDP address.
    pub udp: SocketAddr,
    pub client: reqwest::Client,
    pub state: AppState,
    pub _tmp: tempfile::TempDir,
}

impl TestApp {
    pub async fn spawn() -> TestApp {
        let tmp = tempfile::tempdir().unwrap();
        let udp_sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let udp = udp_sock.local_addr().unwrap();
        let cfg = Config {
            bind: "127.0.0.1:0".parse().unwrap(),
            public_url: "http://127.0.0.1".into(),
            public_ip: "127.0.0.1".parse().unwrap(),
            data_dir: tmp.path().to_path_buf(),
            stun_ports: [udp.port(), 0],
            disk_warn_pct: 85,
            disk_block_pct: 92,
            max_upload_bytes: 5_242_880,
            quota_gb: 400,
            backups_keep: 7,
        };
        std::fs::create_dir_all(cfg.media_dir()).unwrap();
        let pool = db::open_pool("sqlite::memory:").await.unwrap();
        let hub = Hub::default();
        let voice = crate::voice::VoiceRooms::default();
        let relay = crate::voice::relay::Relay::new(voice.clone(), hub.clone());
        let state = AppState {
            db: pool,
            disk: DiskState::new(cfg.disk_warn_pct, cfg.disk_block_pct),
            cfg: Arc::new(cfg),
            hub: hub.clone(),
            voice: voice.clone(),
            relay: relay.clone(),
            music: Default::default(),
        };
        crate::stun::spawn_on(udp_sock, Some(relay));
        crate::music::wire::spawn(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let router = build_router(state.clone());
        tokio::spawn(async move {
            axum::serve(listener, router.into_make_service_with_connect_info::<SocketAddr>()).await.unwrap();
        });
        TestApp { addr, udp, client: reqwest::Client::new(), state, _tmp: tmp }
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

    /// Raw WebSocket connection, no hello sent.
    pub async fn ws_raw(&self) -> WsClient {
        let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{}/api/v1/ws", self.addr)).await.unwrap();
        WsClient { ws }
    }

    /// Connected WebSocket that already sent Hello (Ready is the next message).
    pub async fn ws(&self, token: &str) -> WsClient {
        let mut c = self.ws_raw().await;
        c.send_json(serde_json::json!({"t":"hello","d":{"token":token,"protocol":rynuvo_proto::PROTOCOL_VERSION}})).await;
        c
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }
}

pub struct WsClient {
    ws: tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
}

impl WsClient {
    pub async fn send_json(&mut self, v: serde_json::Value) {
        use futures_util::SinkExt;
        self.ws.send(tokio_tungstenite::tungstenite::Message::text(v.to_string())).await.unwrap();
    }

    /// Next server message within `ms`, skipping control frames. None on timeout/close.
    pub async fn try_next(&mut self, ms: u64) -> Option<rynuvo_proto::WsServerMsg> {
        use futures_util::StreamExt;
        use tokio_tungstenite::tungstenite::Message;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(ms);
        loop {
            let m = tokio::time::timeout_at(deadline, self.ws.next()).await.ok()??.ok()?;
            match m {
                Message::Text(t) => return Some(serde_json::from_str(t.as_str()).expect("server msg")),
                Message::Close(_) => return None,
                _ => continue,
            }
        }
    }

    pub async fn send_binary(&mut self, b: Vec<u8>) {
        use futures_util::SinkExt;
        self.ws.send(tokio_tungstenite::tungstenite::Message::binary(b)).await.unwrap();
    }

    /// Next binary frame within `ms`, skipping text and control frames.
    pub async fn next_binary(&mut self, ms: u64) -> Option<Vec<u8>> {
        use futures_util::StreamExt;
        use tokio_tungstenite::tungstenite::Message;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(ms);
        loop {
            match tokio::time::timeout_at(deadline, self.ws.next()).await.ok()??.ok()? {
                Message::Binary(b) => return Some(b.to_vec()),
                Message::Close(_) => return None,
                _ => continue,
            }
        }
    }

    pub async fn next(&mut self) -> rynuvo_proto::WsServerMsg {
        self.try_next(3000).await.expect("expected a server message")
    }

    /// Wait for a close frame and return its code.
    pub async fn expect_close(&mut self) -> u16 {
        use futures_util::StreamExt;
        use tokio_tungstenite::tungstenite::Message;
        loop {
            let m = tokio::time::timeout(std::time::Duration::from_secs(3), self.ws.next())
                .await
                .expect("timeout waiting for close")
                .expect("stream ended without close frame");
            match m {
                Ok(Message::Close(Some(f))) => return u16::from(f.code),
                Ok(Message::Close(None)) => panic!("close without code"),
                Ok(_) => continue,
                Err(e) => panic!("ws error: {e}"),
            }
        }
    }

    pub async fn close(mut self) {
        let _ = self.ws.close(None).await;
    }
}
