use reqwest::{Method, StatusCode};
use rynuvo_proto::*;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::json;
use url::Url;

#[derive(Debug, Clone)]
pub enum ClientError {
    Api(ApiError),
    Network(String),
    Unauthorized,
}

impl ClientError {
    /// User-facing Indonesian message.
    pub fn user_message(&self) -> String {
        match self {
            ClientError::Api(e) => e.message.clone(),
            ClientError::Network(_) => "Tidak bisa terhubung ke server".into(),
            ClientError::Unauthorized => "Sesi berakhir, silakan masuk lagi".into(),
        }
    }
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::Api(e) => write!(f, "{}: {}", e.code, e.message),
            ClientError::Network(e) => write!(f, "network: {e}"),
            ClientError::Unauthorized => write!(f, "unauthorized"),
        }
    }
}

pub type ClientResult<T> = Result<T, ClientError>;

/// Typed REST client for one Rynuvo server.
#[derive(Clone)]
pub struct Api {
    base: Url,
    token: Option<String>,
    http: reqwest::Client,
}

/// Normalize a user-entered server address to `scheme://host[:port]`.
pub fn normalize_server(s: &str) -> Result<Url, String> {
    let t = s.trim().trim_end_matches('/');
    let with_scheme = if t.starts_with("http://") || t.starts_with("https://") { t.to_string() } else { format!("https://{t}") };
    let mut u = Url::parse(&with_scheme).map_err(|_| "Alamat server tidak valid".to_string())?;
    if u.host_str().is_none() {
        return Err("Alamat server tidak valid".into());
    }
    u.set_path("");
    u.set_query(None);
    Ok(u)
}

impl Api {
    pub fn new(base: &str, token: Option<String>) -> Result<Api, String> {
        let base = normalize_server(base)?;
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(8))
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Api { base, token, http })
    }

    pub fn base(&self) -> String {
        self.base.as_str().trim_end_matches('/').to_string()
    }

    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    /// `ws://` / `wss://` URL of the realtime endpoint.
    pub fn ws_url(&self) -> String {
        let b = self.base();
        let b = b.replacen("https://", "wss://", 1).replacen("http://", "ws://", 1);
        format!("{b}/api/v1/ws")
    }

    fn url(&self, path: &str) -> String {
        format!("{}/api/v1{path}", self.base())
    }

    fn req(&self, m: Method, path: &str) -> reqwest::RequestBuilder {
        let r = self.http.request(m, self.url(path));
        match &self.token {
            Some(t) => r.bearer_auth(t),
            None => r,
        }
    }

    async fn finish(r: Result<reqwest::Response, reqwest::Error>) -> ClientResult<reqwest::Response> {
        let r = r.map_err(|e| ClientError::Network(e.to_string()))?;
        let status = r.status();
        if status.is_success() {
            return Ok(r);
        }
        if status == StatusCode::UNAUTHORIZED {
            return Err(ClientError::Unauthorized);
        }
        match r.json::<ApiError>().await {
            Ok(e) => Err(ClientError::Api(e)),
            Err(_) => Err(ClientError::Api(ApiError { code: "http".into(), message: format!("Server error ({status})") })),
        }
    }

    async fn json<T: DeserializeOwned>(&self, m: Method, path: &str, body: Option<&impl Serialize>) -> ClientResult<T> {
        let mut r = self.req(m, path);
        if let Some(b) = body {
            r = r.json(b);
        }
        Self::finish(r.send().await).await?.json().await.map_err(|e| ClientError::Network(e.to_string()))
    }

    async fn empty(&self, m: Method, path: &str) -> ClientResult<()> {
        Self::finish(self.req(m, path).send().await).await.map(|_| ())
    }

    const NONE: Option<&'static ()> = None;

    pub async fn join(&self, invite: &str, nickname: &str, device: &str) -> ClientResult<JoinResp> {
        let b = JoinReq { invite_code: invite.into(), nickname: nickname.into(), device_name: device.into() };
        self.json(Method::POST, "/join", Some(&b)).await
    }
    pub async fn claim_transfer(&self, code: &str, device: &str) -> ClientResult<JoinResp> {
        let b = ClaimTransferReq { code: code.into(), device_name: device.into() };
        self.json(Method::POST, "/claim-transfer", Some(&b)).await
    }
    pub async fn me(&self) -> ClientResult<User> {
        self.json(Method::GET, "/me", Self::NONE).await
    }
    pub async fn rename_me(&self, nickname: &str) -> ClientResult<User> {
        self.json(Method::PATCH, "/me", Some(&RenameReq { nickname: nickname.into() })).await
    }
    pub async fn channels(&self) -> ClientResult<Vec<Channel>> {
        self.json(Method::GET, "/channels", Self::NONE).await
    }
    pub async fn create_channel(&self, kind: ChannelKind, name: &str) -> ClientResult<Channel> {
        self.json(Method::POST, "/channels", Some(&CreateChannelReq { kind, name: name.into() })).await
    }
    pub async fn update_channel(&self, id: Id, req: &UpdateChannelReq) -> ClientResult<Channel> {
        self.json(Method::PATCH, &format!("/channels/{id}"), Some(req)).await
    }
    pub async fn delete_channel(&self, id: Id) -> ClientResult<()> {
        self.empty(Method::DELETE, &format!("/channels/{id}")).await
    }
    pub async fn messages(&self, channel: Id, before: Option<Id>, after: Option<Id>, limit: Option<i64>) -> ClientResult<Vec<Message>> {
        let mut q = Vec::new();
        if let Some(b) = before {
            q.push(format!("before={b}"));
        }
        if let Some(a) = after {
            q.push(format!("after={a}"));
        }
        if let Some(l) = limit {
            q.push(format!("limit={l}"));
        }
        let qs = if q.is_empty() { String::new() } else { format!("?{}", q.join("&")) };
        self.json(Method::GET, &format!("/channels/{channel}/messages{qs}"), Self::NONE).await
    }
    pub async fn post_message(&self, channel: Id, content: &str, image_id: Option<String>) -> ClientResult<Message> {
        self.json(Method::POST, &format!("/channels/{channel}/messages"), Some(&PostMessageReq { content: content.into(), image_id })).await
    }
    pub async fn edit_message(&self, id: Id, content: &str) -> ClientResult<Message> {
        self.json(Method::PATCH, &format!("/messages/{id}"), Some(&EditMessageReq { content: content.into() })).await
    }
    pub async fn delete_message(&self, id: Id) -> ClientResult<()> {
        self.empty(Method::DELETE, &format!("/messages/{id}")).await
    }
    pub async fn upload_media(&self, bytes: Vec<u8>, mime: &str) -> ClientResult<ImageRef> {
        let r = self.req(Method::POST, "/media").header("content-type", mime).body(bytes).send().await;
        Self::finish(r).await?.json().await.map_err(|e| ClientError::Network(e.to_string()))
    }
    pub async fn fetch_media(&self, id: &str) -> ClientResult<(Vec<u8>, String)> {
        let r = Self::finish(self.req(Method::GET, &format!("/media/{id}")).send().await).await?;
        let mime = r.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("application/octet-stream").to_string();
        let b = r.bytes().await.map_err(|e| ClientError::Network(e.to_string()))?;
        Ok((b.to_vec(), mime))
    }
    pub async fn transfer_code(&self) -> ClientResult<TransferCodeResp> {
        self.json(Method::POST, "/me/transfer-code", Self::NONE).await
    }
    pub async fn admin_rotate_invite(&self) -> ClientResult<RotateInviteResp> {
        self.json(Method::POST, "/admin/invite", Self::NONE).await
    }
    pub async fn admin_kick(&self, user: Id) -> ClientResult<()> {
        self.empty(Method::DELETE, &format!("/admin/users/{user}")).await
    }
    pub async fn admin_transfer_code(&self, user: Id) -> ClientResult<TransferCodeResp> {
        self.json(Method::POST, &format!("/admin/users/{user}/transfer-code"), Self::NONE).await
    }
    pub async fn upload_test(&self, bytes: Vec<u8>) -> ClientResult<serde_json::Value> {
        let r = self.req(Method::POST, "/diag/upload-test").body(bytes).send().await;
        Self::finish(r).await?.json().await.map_err(|e| ClientError::Network(e.to_string()))
    }
    pub async fn report(&self, kind: &str, body: &str) -> ClientResult<()> {
        let r = self.req(Method::POST, "/diag/report").json(&json!({"kind": kind, "body": body})).send().await;
        Self::finish(r).await.map(|_| ())
    }
    pub async fn reports(&self, kind: Option<&str>) -> ClientResult<serde_json::Value> {
        let path = match kind {
            Some(k) => format!("/diag/reports?kind={k}"),
            None => "/diag/reports".into(),
        };
        self.json(Method::GET, &path, Self::NONE).await
    }
}
