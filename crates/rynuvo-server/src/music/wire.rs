//! Music on the wire: commands from WebSocket, state broadcasts, the bot's chat replies, and the
//! ticker that moves a channel on when its song ends.
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use rynuvo_proto::{Id, MusicCmd, MusicState, WsServerMsg};

use super::{Actor, Cmd, Session};
use crate::{app::AppState, now_ms, routes::messages::load_message};

pub const BOT_KEY: &str = "#bot:music";
const TICK: Duration = Duration::from_millis(500);

/// Music sessions per voice channel. In memory only: a restart clears all music.
#[derive(Clone, Default)]
pub struct MusicRooms {
    inner: Arc<Mutex<HashMap<Id, Session>>>,
}

impl MusicRooms {
    /// States of every channel that has music (for Ready).
    pub fn states(&self, now: i64) -> Vec<MusicState> {
        self.inner.lock().unwrap().iter().filter(|(_, s)| !s.is_empty()).map(|(ch, s)| s.state(*ch, now)).collect()
    }
}

pub async fn bot_id(st: &AppState) -> Option<Id> {
    sqlx::query_scalar("SELECT id FROM users WHERE nickname_key = ?").bind(BOT_KEY).fetch_optional(&st.db).await.ok().flatten()
}

async fn nicks(st: &AppState) -> HashMap<Id, String> {
    sqlx::query_as::<_, (Id, String)>("SELECT id, nickname FROM users").fetch_all(&st.db).await.unwrap_or_default().into_iter().collect()
}

async fn is_text_channel(st: &AppState, id: Id) -> bool {
    matches!(sqlx::query_scalar::<_, String>("SELECT kind FROM channels WHERE id = ?").bind(id).fetch_optional(&st.db).await, Ok(Some(k)) if k == "text")
}

/// Post `text` in `channel` as Rynuvo Music.
pub async fn say(st: &AppState, channel: Id, text: &str) {
    let Some(bot) = bot_id(st).await else { return };
    let r: sqlx::Result<i64> = sqlx::query_scalar("INSERT INTO messages(channel_id, author_id, content, created_at) VALUES(?, ?, ?, ?) RETURNING id")
        .bind(channel)
        .bind(bot)
        .bind(text)
        .bind(now_ms())
        .fetch_one(&st.db)
        .await;
    match r {
        Ok(id) => {
            if let Ok(Some(m)) = load_message(&st.db, id).await {
                st.hub.broadcast(WsServerMsg::MessageCreated(m));
            }
        }
        Err(e) => tracing::warn!("music reply not posted: {e}"),
    }
}

fn to_cmd(c: MusicCmd) -> Cmd {
    match c {
        MusicCmd::Add { entry } => Cmd::Add(entry),
        MusicCmd::Lyrics { source } => Cmd::Lyrics(source),
        MusicCmd::Repeat { mode } => Cmd::Repeat(mode),
        MusicCmd::Skip => Cmd::Skip,
        MusicCmd::Pause => Cmd::Pause,
        MusicCmd::Resume => Cmd::Resume,
        MusicCmd::Seek { ms } => Cmd::Seek(ms),
        MusicCmd::Remove { index } => Cmd::Remove(index as usize),
        MusicCmd::Stop => Cmd::Stop,
        MusicCmd::Queue => Cmd::Queue,
        MusicCmd::NowPlaying => Cmd::NowPlaying,
        MusicCmd::NotFound { query } => Cmd::NotFound(query),
    }
}

pub async fn command(st: &AppState, user_id: Id, is_admin: bool, cmd: MusicCmd, text_channel: Option<Id>) {
    let text_channel = match text_channel {
        Some(c) if is_text_channel(st, c).await => Some(c),
        _ => None,
    };
    let Some(me) = st.voice.member(user_id) else {
        if let Some(c) = text_channel {
            say(st, c, "Join a voice channel first").await;
        }
        return;
    };
    let ch = me.channel_id;
    let names = nicks(st).await;
    let owner = sqlx::query_scalar::<_, Option<Id>>("SELECT owner_id FROM channels WHERE id = ?").bind(ch).fetch_optional(&st.db).await.ok().flatten().flatten();
    let manager = is_admin || owner == Some(user_id);
    let voice = st.voice.clone();
    let listeners = st.voice.members().iter().filter(|m| m.channel_id == ch).count();
    let present = move |id: Id| voice.member(id).is_some_and(|m| m.channel_id == ch);
    let nick = |id: Id| names.get(&id).cloned().unwrap_or_else(|| "Someone".into());
    let now = now_ms();
    let (out, state, reply_to) = {
        let mut g = st.music.inner.lock().unwrap();
        let s = g.entry(ch).or_default();
        if text_channel.is_some() {
            s.reply_to = text_channel;
        }
        let out = s.handle(to_cmd(cmd), &Actor { id: user_id, admin: is_admin, manager, present: &present, listeners, nick: &nick }, now);
        (out, s.state(ch, now), text_channel.or(s.reply_to))
    };
    if out.changed {
        st.hub.broadcast(WsServerMsg::MusicState(state));
    }
    if let (Some(text), Some(c)) = (out.reply, reply_to) {
        say(st, c, &text).await;
    }
}

/// Someone left `channel`: with nobody left, its music ends.
pub fn voice_left(st: &AppState, channel: Id) {
    if st.voice.members().iter().any(|m| m.channel_id == channel) {
        return;
    }
    let had = st.music.inner.lock().unwrap().remove(&channel).is_some_and(|s| !s.is_empty());
    if had {
        st.hub.broadcast(WsServerMsg::MusicState(Session::default().state(channel, now_ms())));
    }
}

/// Moves channels on when their song has ended.
pub fn spawn(st: AppState) {
    tokio::spawn(async move {
        let mut t = tokio::time::interval(TICK);
        loop {
            t.tick().await;
            let now = now_ms();
            let advanced: Vec<(MusicState, Option<(Id, rynuvo_proto::MusicEntry)>)> = {
                let mut g = st.music.inner.lock().unwrap();
                g.iter_mut()
                    .filter(|(_, s)| s.finished(now))
                    .map(|(ch, s)| {
                        let was = s.playing.as_ref().map(|p| p.entry.id);
                        // A looped song coming round again needs no announcement.
                        let next = s.advance(now, true).filter(|e| Some(e.id) != was).cloned();
                        (s.state(*ch, now), s.reply_to.zip(next))
                    })
                    .collect()
            };
            if advanced.is_empty() {
                continue;
            }
            let names = nicks(&st).await;
            for (state, reply) in advanced {
                st.hub.broadcast(WsServerMsg::MusicState(state));
                if let Some((c, e)) = reply {
                    let nick = names.get(&e.requested_by).cloned().unwrap_or_else(|| "Someone".into());
                    say(&st, c, &format!("▶ Now playing: {} · requested by {nick}", super::line(&e))).await;
                }
            }
        }
    });
}
