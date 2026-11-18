//! Push notifications for phones whose app is closed (Firebase Cloud Messaging).
//!
//! What was said never goes through Google: a push carries only a random id. The phone wakes, fetches
//! the notification for that id from here with its push key (a key that can read notifications and
//! nothing else), and shows it. Notifications wait here for 15 minutes.
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use rynuvo_proto::{Id, Message, PushNote, PushPerson};
use serde::Deserialize;

use crate::{app::AppState, auth, now_ms};

const NOTE_TTL_MS: i64 = 15 * 60_000;
const MAX_NOTES: usize = 5000;
const FCM_SCOPE: &str = "https://www.googleapis.com/auth/firebase.messaging";
/// Out of a room and back within this long is a dropped connection, not news.
pub const REJOIN_MS: i64 = 2 * 60_000;
/// A room that emptied is over after this long (so a lone member's hiccup doesn't start a new one).
const ROOM_GRACE_MS: u64 = 60_000;

/// A room's hangout, from its first member in until it has stayed empty for a minute. Each phone rings
/// once per hangout; later arrivals update its notification quietly.
#[derive(Default)]
struct Hangout {
    /// Phones showing its notification (device, token), to clear when it's over.
    shown: Vec<(i64, String)>,
    /// Users whose phone already rang for it.
    rang: std::collections::HashSet<Id>,
    /// When each member last left it.
    left: HashMap<Id, i64>,
    /// Bumped each time it empties, so a later rejoin cancels the pending end.
    emptied: u64,
}

#[derive(Clone, Default)]
pub struct Push {
    inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
    transport: Transport,
    notes: Mutex<HashMap<String, (Id, PushNote, i64)>>,
    hangouts: Mutex<HashMap<Id, Hangout>>,
    /// How long an emptied room waits before it's over (shorter in tests).
    grace_ms: Option<u64>,
}

#[derive(Default)]
enum Transport {
    /// No Firebase key configured: notifications are kept but nothing is sent.
    #[default]
    Off,
    /// Tests: remember what would have been sent, as (token, note id).
    Record(Mutex<Vec<(String, String)>>),
    Fcm(Fcm),
}

struct Fcm {
    project: String,
    email: String,
    key: jsonwebtoken::EncodingKey,
    token_uri: String,
    http: reqwest::Client,
    access: tokio::sync::Mutex<Option<(String, Instant)>>,
}

#[derive(Deserialize)]
struct ServiceAccount {
    project_id: String,
    client_email: String,
    private_key: String,
    token_uri: String,
}

impl Push {
    /// Sends through Firebase with this service account key file (JSON, from the Firebase console).
    pub fn fcm(key_file: &Path) -> anyhow::Result<Push> {
        let sa: ServiceAccount = serde_json::from_slice(&std::fs::read(key_file)?)?;
        let fcm = Fcm {
            key: jsonwebtoken::EncodingKey::from_rsa_pem(sa.private_key.as_bytes())?,
            project: sa.project_id,
            email: sa.client_email,
            token_uri: sa.token_uri,
            http: reqwest::Client::builder().timeout(Duration::from_secs(15)).build()?,
            access: tokio::sync::Mutex::new(None),
        };
        Ok(Push { inner: Arc::new(Inner { transport: Transport::Fcm(fcm), ..Default::default() }) })
    }

    /// For tests: records instead of sending.
    pub fn recording() -> Push {
        Push { inner: Arc::new(Inner { transport: Transport::Record(Mutex::default()), grace_ms: Some(300), ..Default::default() }) }
    }

    /// What a recording `Push` would have sent, as (token, note id).
    pub fn sent(&self) -> Vec<(String, String)> {
        match &self.inner.transport {
            Transport::Record(r) => r.lock().unwrap().clone(),
            _ => Vec::new(),
        }
    }

    fn keep(&self, user: Id, note: PushNote) -> String {
        let id = auth::new_secret();
        let now = now_ms();
        let mut g = self.inner.notes.lock().unwrap();
        g.retain(|_, (_, _, exp)| *exp > now);
        if g.len() >= MAX_NOTES {
            return id; // full: the phone will show a plain "something new" instead
        }
        g.insert(id.clone(), (user, note, now + NOTE_TTL_MS));
        id
    }

    /// The note `id`, if it is for `user` and still fresh.
    pub fn note(&self, id: &str, user: Id) -> Option<PushNote> {
        let g = self.inner.notes.lock().unwrap();
        g.get(id).filter(|(u, _, exp)| *u == user && *exp > now_ms()).map(|(_, n, _)| n.clone())
    }

    /// Send one push. Returns false when Firebase says the token is dead (app removed, data cleared).
    async fn send(&self, token: &str, note_id: &str, kind: &str) -> bool {
        self.send_data(token, note_id, kind, None).await
    }

    async fn send_data(&self, token: &str, note_id: &str, kind: &str, channel: Option<Id>) -> bool {
        match &self.inner.transport {
            Transport::Off => true,
            Transport::Record(r) => {
                r.lock().unwrap().push((token.to_string(), note_id.to_string()));
                true
            }
            Transport::Fcm(f) => match f.send(token, note_id, kind, channel).await {
                Ok(alive) => alive,
                Err(e) => {
                    tracing::warn!("push failed: {e:#}");
                    true
                }
            },
        }
    }
}

impl Fcm {
    async fn access_token(&self) -> anyhow::Result<String> {
        let mut g = self.access.lock().await;
        if let Some((t, until)) = g.as_ref() {
            if Instant::now() < *until {
                return Ok(t.clone());
            }
        }
        let iat = now_ms() / 1000;
        let claims = serde_json::json!({"iss": self.email, "scope": FCM_SCOPE, "aud": self.token_uri, "iat": iat, "exp": iat + 3600});
        let jwt = jsonwebtoken::encode(&jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256), &claims, &self.key)?;
        let body = format!("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer&assertion={jwt}");
        #[derive(Deserialize)]
        struct Tok {
            access_token: String,
            expires_in: u64,
        }
        let r = self.http.post(&self.token_uri).header("content-type", "application/x-www-form-urlencoded").body(body).send().await?;
        if !r.status().is_success() {
            anyhow::bail!("token: {} {}", r.status(), r.text().await.unwrap_or_default());
        }
        let t: Tok = r.json().await?;
        // Renew a few minutes early.
        *g = Some((t.access_token.clone(), Instant::now() + Duration::from_secs(t.expires_in.saturating_sub(300))));
        Ok(t.access_token)
    }

    async fn send(&self, token: &str, note_id: &str, kind: &str, channel: Option<Id>) -> anyhow::Result<bool> {
        let access = self.access_token().await?;
        let url = format!("https://fcm.googleapis.com/v1/projects/{}/messages:send", self.project);
        // Data only (the app draws the notification), high priority so a sleeping phone wakes for it.
        let ttl = if kind == "message" { "900s" } else { "120s" };
        let mut data = serde_json::json!({"n": note_id, "k": kind});
        if let Some(c) = channel {
            data["c"] = c.to_string().into();
        }
        let body = serde_json::json!({"message": {"token": token, "data": data, "android": {"priority": "high", "ttl": ttl}}});
        let r = self.http.post(&url).bearer_auth(access).json(&body).send().await?;
        let status = r.status();
        if status.is_success() {
            return Ok(true);
        }
        let text = r.text().await.unwrap_or_default();
        if status.as_u16() == 404 || text.contains("UNREGISTERED") {
            return Ok(false);
        }
        anyhow::bail!("send: {status} {text}")
    }
}

/// Phones registered for push, of these users, whose app isn't open right now.
async fn away_phones(st: &AppState, users: &[Id]) -> Vec<(i64, Id, String)> {
    if users.is_empty() {
        return Vec::new();
    }
    let rows: Vec<(i64, Id, String)> = match sqlx::query_as("SELECT device_id, user_id, token FROM push_tokens").fetch_all(&st.db).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("push tokens: {e}");
            return Vec::new();
        }
    };
    let open = st.hub.active_devices();
    rows.into_iter().filter(|(d, u, _)| users.contains(u) && !open.contains(d)).collect()
}

async fn deliver(st: &AppState, phones: Vec<(i64, Id, String)>, kind: &str, note_for: impl Fn(Id) -> PushNote) {
    for (device, user, token) in phones {
        let id = st.push.keep(user, note_for(user));
        if !st.push.send(&token, &id, kind).await {
            let _ = sqlx::query("DELETE FROM push_tokens WHERE device_id = ?").bind(device).execute(&st.db).await;
        }
    }
}

async fn names(st: &AppState) -> HashMap<Id, String> {
    sqlx::query_as::<_, (Id, String)>("SELECT id, nickname FROM users").fetch_all(&st.db).await.unwrap_or_default().into_iter().collect()
}

/// A user's profile photo (media id) and avatar colour.
async fn looks(st: &AppState, user: Id) -> (Option<String>, Option<String>) {
    sqlx::query_as("SELECT avatar_image, avatar_color FROM users WHERE id = ?").bind(user).fetch_optional(&st.db).await.ok().flatten().unwrap_or((None, None))
}

async fn channel_name(st: &AppState, id: Id) -> String {
    sqlx::query_scalar("SELECT name FROM channels WHERE id = ?").bind(id).fetch_optional(&st.db).await.ok().flatten().unwrap_or_default()
}

/// A new message: tell everyone else whose app is closed.
pub async fn message(st: AppState, m: Message) {
    let names = names(&st).await;
    let users: Vec<Id> = names.keys().copied().filter(|u| *u != m.author_id).collect();
    let phones = away_phones(&st, &users).await;
    if phones.is_empty() {
        return;
    }
    let channel = channel_name(&st, m.channel_id).await;
    let from = names.get(&m.author_id).cloned().unwrap_or_else(|| "Someone".into());
    let (avatar, color) = looks(&st, m.author_id).await;
    let all: Vec<&str> = names.values().map(String::as_str).collect();
    let mut text: String = m.content.chars().take(200).collect();
    if text.trim().is_empty() && m.image.is_some() {
        text = "sent an image".into();
    }
    deliver(&st, phones, "message", |user| PushNote {
        kind: "message".into(),
        channel_id: m.channel_id,
        channel: channel.clone(),
        from: from.clone(),
        from_id: m.author_id,
        avatar: avatar.clone(),
        color: color.clone(),
        text: text.clone(),
        mention: names.get(&user).is_some_and(|me| rynuvo_proto::mentions_me(&m.content, me, &all)),
        at: m.created_at,
        image: m.image.as_ref().map(|i| i.id.clone()),
        people: Vec::new(),
        quiet: false,
    })
    .await;
}

/// "Come hang out" for people whose app is closed.
pub async fn invite(st: &AppState, from: Id, to: &[Id], channel_id: Id) {
    let phones = away_phones(st, to).await;
    if phones.is_empty() {
        return;
    }
    let names = names(st).await;
    let channel = channel_name(st, channel_id).await;
    let nick = names.get(&from).cloned().unwrap_or_else(|| "Someone".into());
    let (avatar, color) = looks(st, from).await;
    let at = now_ms();
    deliver(st, phones, "invite", |_| PushNote {
        kind: "invite".into(),
        channel_id,
        channel: channel.clone(),
        from: nick.clone(),
        from_id: from,
        avatar: avatar.clone(),
        color: color.clone(),
        text: String::new(),
        mention: false,
        at,
        image: None,
        people: Vec::new(),
        quiet: false,
    })
    .await;
}

/// Someone came into voice room `channel_id`: tell the others with the app closed (not those in
/// there). Each phone rings once per hangout; whoever comes after updates it quietly. Back within
/// two minutes of leaving doesn't count.
pub async fn room(st: AppState, channel_id: Id, joiner: Id) {
    let mut inside: Vec<Id> = st.voice.members().into_iter().filter(|m| m.channel_id == channel_id).map(|m| m.user_id).collect();
    if !inside.contains(&joiner) {
        return; // gone again already
    }
    let now = now_ms();
    {
        let mut g = st.push.inner.hangouts.lock().unwrap();
        let h = g.entry(channel_id).or_default();
        h.emptied += 1; // whatever end was pending, it's off
        if h.left.get(&joiner).is_some_and(|at| now - at < REJOIN_MS) {
            return;
        }
    }
    // The one who just came in first.
    inside.retain(|u| *u != joiner);
    inside.insert(0, joiner);
    let names = names(&st).await;
    let users: Vec<Id> = names.keys().copied().filter(|u| !inside.contains(u)).collect();
    let phones = away_phones(&st, &users).await;
    if phones.is_empty() {
        return;
    }
    let mut people = Vec::new();
    for id in inside.iter().take(6) {
        let (avatar, color) = looks(&st, *id).await;
        people.push(PushPerson { id: *id, name: names.get(id).cloned().unwrap_or_default(), avatar, color });
    }
    let channel = channel_name(&st, channel_id).await;
    let first = people[0].clone();
    let rang: std::collections::HashSet<Id> = {
        let mut g = st.push.inner.hangouts.lock().unwrap();
        let h = g.entry(channel_id).or_default();
        let before = h.rang.clone();
        for (d, u, t) in &phones {
            h.rang.insert(*u);
            if !h.shown.iter().any(|(sd, _)| sd == d) {
                h.shown.push((*d, t.clone()));
            }
        }
        before
    };
    deliver(&st, phones, "room", |user| PushNote {
        kind: "room".into(),
        channel_id,
        channel: channel.clone(),
        from: first.name.clone(),
        from_id: first.id,
        avatar: first.avatar.clone(),
        color: first.color.clone(),
        text: String::new(),
        mention: false,
        at: now,
        image: None,
        people: people.clone(),
        quiet: rang.contains(&user),
    })
    .await;
}

/// `user` left room `channel_id`. Once it has stayed empty a minute the hangout is over and its
/// notifications go away.
pub async fn room_left(st: AppState, channel_id: Id, user: Id) {
    let empty = !st.voice.members().iter().any(|v| v.channel_id == channel_id);
    let ticket = {
        let mut g = st.push.inner.hangouts.lock().unwrap();
        let Some(h) = g.get_mut(&channel_id) else { return };
        h.left.insert(user, now_ms());
        if !empty {
            return;
        }
        h.emptied += 1;
        h.emptied
    };
    tokio::time::sleep(Duration::from_millis(st.push.inner.grace_ms.unwrap_or(ROOM_GRACE_MS))).await;
    let shown = {
        let mut g = st.push.inner.hangouts.lock().unwrap();
        let still_empty = !st.voice.members().iter().any(|v| v.channel_id == channel_id);
        match g.get(&channel_id) {
            Some(h) if h.emptied == ticket && still_empty => g.remove(&channel_id).map(|h| h.shown).unwrap_or_default(),
            _ => return,
        }
    };
    for (_, token) in shown {
        st.push.send_data(&token, "", "roomEnded", Some(channel_id)).await;
    }
}
