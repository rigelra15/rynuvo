//! Tauri commands: thin wrappers over `core`.
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use rynuvo_proto::*;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::core::{
    http::{Api, ClientError},
    identity::SecretStore,
    session::{Session, SessionEvent, SessionHandle},
};
use crate::{music::ctl::MusicCtl, stream::ctl::StreamCtl, voice::ctl::VoiceCtl};

pub const EVENT: &str = "rynuvo://event";
pub const VOICE_EVENT: &str = "rynuvo://voice";

#[derive(Default)]
struct Inner {
    api: Option<Api>,
    session: Option<SessionHandle>,
}

pub struct Core {
    pub store: Arc<dyn SecretStore>,
    pub state_path: PathBuf,
    pub voice: VoiceCtl,
    pub stream: StreamCtl,
    pub music: MusicCtl,
    inner: Mutex<Inner>,
}

#[derive(Serialize, Deserialize, Default)]
struct Persisted {
    server: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootInfo {
    server: String,
    me: Option<User>,
}

type CmdResult<T> = Result<T, String>;

fn err(e: ClientError) -> String {
    tracing::warn!("api error: {e}");
    e.user_message()
}

pub fn device_name() -> String {
    match std::env::consts::OS {
        "macos" => "Mac".into(),
        "windows" => "PC Windows".into(),
        o => o.into(),
    }
}

impl Core {
    pub fn new(store: Arc<dyn SecretStore>, state_path: PathBuf, voice: VoiceCtl, stream: StreamCtl, music: MusicCtl) -> Self {
        Self { store, state_path, voice, stream, music, inner: Mutex::default() }
    }

    fn load(&self) -> Persisted {
        std::fs::read(&self.state_path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    fn save(&self, p: &Persisted) {
        if let Some(dir) = self.state_path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&self.state_path, serde_json::to_vec(p).unwrap());
    }

    pub fn api(&self) -> CmdResult<Api> {
        self.inner.lock().unwrap().api.clone().ok_or_else(|| "Not signed in".to_string())
    }

    pub fn session(&self) -> Option<SessionHandle> {
        self.inner.lock().unwrap().session.clone()
    }

    fn start(&self, app: &AppHandle, api: Api) {
        let mut g = self.inner.lock().unwrap();
        if let Some(s) = g.session.take() {
            s.stop();
        }
        let app2 = app.clone();
        let voice = self.voice.clone();
        let stream = self.stream.clone();
        let music = self.music.clone();
        let emit = Arc::new(move |e: SessionEvent| {
            voice.on_session(&e);
            stream.on_session(&e);
            if let SessionEvent::Server(m) = &e {
                music.on_server(m, crate::music::now_ms());
            }
            crate::notify::on_event(&app2, &e);
            let _ = app2.emit(EVENT, e);
        });
        let server = api.base();
        let host = url::Url::parse(&server).ok().and_then(|u| u.host_str().map(|h| h.trim_matches(['[', ']']).to_string())).unwrap_or_default();
        self.voice.set_device(crate::e2ee::device::load_or_create(&*self.store, &server));
        let s = Session::start(api.clone(), self.store.clone(), server, emit);
        self.voice.attach(s.clone(), host);
        g.session = Some(s);
        g.api = Some(api);
    }

    fn signed_in(&self, app: &AppHandle, server: String, resp: JoinResp) -> CmdResult<User> {
        let api = Api::new(&server, Some(resp.token.clone()))?;
        self.store.set(&api.base(), &resp.token);
        self.save(&Persisted { server: Some(api.base()) });
        self.start(app, api);
        Ok(resp.user)
    }
}

/// The server to resume a saved sign-in on: only the one this app talks to. A sign-in saved for any
/// other server (an old address, a local test server) is ignored, so the user signs in again.
fn boot_server(saved: Option<&str>, fixed: &str) -> Option<String> {
    let norm = |s: &str| s.trim_end_matches('/').to_string();
    saved.map(norm).filter(|s| *s == norm(fixed))
}

#[tauri::command]
pub async fn bootstrap(app: AppHandle, core: State<'_, Core>) -> CmdResult<Option<BootInfo>> {
    let Some(server) = boot_server(core.load().server.as_deref(), &server()) else { return Ok(None) };
    let Some(token) = core.store.get(&server) else { return Ok(None) };
    let api = Api::new(&server, Some(token))?;
    let me = match api.me().await {
        Ok(u) => Some(u),
        Err(ClientError::Unauthorized) => {
            core.store.clear(&server);
            return Ok(None);
        }
        Err(_) => None, // offline: session keeps retrying, Ready fills `me`
    };
    core.start(&app, api);
    Ok(Some(BootInfo { server, me }))
}

/// The circle's server. Fixed in the app; `RYNUVO_SERVER` overrides it for local testing.
pub const SERVER: &str = "https://rynuvo.pokyvl.my.id";

fn server() -> String {
    std::env::var("RYNUVO_SERVER").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| SERVER.to_string())
}

#[tauri::command]
pub async fn join(app: AppHandle, core: State<'_, Core>, invite: String, nickname: String) -> CmdResult<User> {
    let api = Api::new(&server(), None)?;
    let resp = api.join(&invite, &nickname, &device_name()).await.map_err(err)?;
    core.signed_in(&app, api.base(), resp)
}

#[tauri::command]
pub async fn claim_transfer(app: AppHandle, core: State<'_, Core>, code: String) -> CmdResult<User> {
    let api = Api::new(&server(), None)?;
    let resp = api.claim_transfer(&code, &device_name()).await.map_err(err)?;
    core.signed_in(&app, api.base(), resp)
}

#[tauri::command]
pub fn logout(core: State<'_, Core>) {
    core.stream.end_all("Signed out");
    core.voice.detach();
    let mut g = core.inner.lock().unwrap();
    if let Some(s) = g.session.take() {
        s.stop();
    }
    if let Some(api) = g.api.take() {
        core.store.clear(&api.base());
    }
    crate::notify::reset();
}

#[tauri::command]
pub async fn rename_me(core: State<'_, Core>, nickname: String) -> CmdResult<User> {
    core.api()?.rename_me(&nickname).await.map_err(err)
}

#[tauri::command]
pub async fn set_profile(core: State<'_, Core>, profile: rynuvo_proto::ProfileReq) -> CmdResult<User> {
    core.api()?.update_me(&profile).await.map_err(err)
}

#[tauri::command]
pub async fn create_channel(core: State<'_, Core>, kind: ChannelKind, name: String) -> CmdResult<Channel> {
    core.api()?.create_channel(kind, &name).await.map_err(err)
}

#[tauri::command]
pub async fn update_channel(core: State<'_, Core>, id: Id, req: UpdateChannelReq) -> CmdResult<Channel> {
    core.api()?.update_channel(id, &req).await.map_err(err)
}

#[tauri::command]
pub async fn delete_channel(core: State<'_, Core>, id: Id) -> CmdResult<()> {
    core.api()?.delete_channel(id).await.map_err(err)
}

/// Load a page of history. Loading the latest page (no `before`) starts catch-up tracking.
#[tauri::command]
pub async fn messages(core: State<'_, Core>, channel: Id, before: Option<Id>) -> CmdResult<Vec<Message>> {
    let list = core.api()?.messages(channel, before, None, Some(50)).await.map_err(err)?;
    if before.is_none() {
        if let Some(s) = core.session() {
            s.mark_loaded(channel, list.last().map(|m| m.id).unwrap_or(0));
        }
    }
    Ok(list)
}

#[tauri::command]
pub async fn post_message(core: State<'_, Core>, channel: Id, content: String) -> CmdResult<Message> {
    core.api()?.post_message(channel, &content, None).await.map_err(err)
}

#[tauri::command]
pub async fn edit_message(core: State<'_, Core>, id: Id, content: String) -> CmdResult<Message> {
    core.api()?.edit_message(id, &content).await.map_err(err)
}

#[tauri::command]
pub async fn delete_message(core: State<'_, Core>, id: Id) -> CmdResult<()> {
    core.api()?.delete_message(id).await.map_err(err)
}

#[tauri::command]
pub fn send_typing(core: State<'_, Core>, channel: Id) {
    if let Some(s) = core.session() {
        s.send_typing(channel);
    }
}

#[tauri::command]
pub async fn transfer_code(core: State<'_, Core>) -> CmdResult<TransferCodeResp> {
    core.api()?.transfer_code().await.map_err(err)
}

#[tauri::command]
pub async fn admin_rotate_invite(core: State<'_, Core>) -> CmdResult<RotateInviteResp> {
    core.api()?.admin_rotate_invite().await.map_err(err)
}

#[tauri::command]
pub async fn admin_kick(core: State<'_, Core>, user: Id) -> CmdResult<()> {
    core.api()?.admin_kick(user).await.map_err(err)
}

#[tauri::command]
pub async fn admin_transfer_code(core: State<'_, Core>, user: Id) -> CmdResult<TransferCodeResp> {
    core.api()?.admin_transfer_code(user).await.map_err(err)
}

#[tauri::command]
pub async fn admin_reports(core: State<'_, Core>) -> CmdResult<serde_json::Value> {
    core.api()?.reports(None).await.map_err(err)
}

#[tauri::command]
pub async fn admin_report_body(core: State<'_, Core>, id: i64) -> CmdResult<String> {
    let v = core.api()?.report_body(id).await.map_err(err)?;
    Ok(v["body"].as_str().unwrap_or_default().to_string())
}

#[cfg(test)]
mod tests {
    use super::boot_server;

    #[test]
    fn a_saved_server_other_than_the_apps_own_is_ignored() {
        let fixed = "https://rynuvo.pokyvl.my.id";
        assert_eq!(boot_server(Some("http://127.0.0.1:8110"), fixed), None, "old test server");
        assert_eq!(boot_server(Some("https://rynuvo.pokyvl.my.id"), fixed).as_deref(), Some(fixed));
        assert_eq!(boot_server(Some("https://rynuvo.pokyvl.my.id/"), fixed).as_deref(), Some(fixed), "trailing slash");
        assert_eq!(boot_server(None, fixed), None, "never signed in");
    }
}
