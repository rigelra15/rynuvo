//! Voice channels: membership, signaling over the WebSocket, and usage accounting.
pub mod relay;
pub mod rooms;

pub use rooms::{Member, VoiceRooms};

use rynuvo_proto::{ChannelKind, Id, WsServerMsg};
use rooms::Viewer;

use crate::{app::AppState, now_ms, ws::hub::Outgoing};

pub const GONE: &str = "That voice channel is gone";
pub const FULL: &str = "This voice channel is full";
pub const KICKED: &str = "You were disconnected from the voice channel";
pub const DELETED: &str = "The voice channel was deleted";
pub const MOVED: &str = "You joined voice from another device";

fn ended(st: &AppState, conn_id: u64, reason: &str) {
    st.hub.send_to_conn(conn_id, Outgoing::Msg(WsServerMsg::VoiceEnded { reason: reason.into() }));
}

/// Announce that `m` left and store its traffic.
fn left(st: &AppState, m: Member) {
    st.hub.broadcast(WsServerMsg::VoiceMemberLeft { user_id: m.user_id, channel_id: m.channel_id });
    let ch = m.channel_id;
    store_usage(st, m);
    crate::music::wire::voice_left(st, ch);
}

/// The member's voice session is over: end their stream and their watching.
fn end_media(st: &AppState, m: &Member) {
    if let Some(s) = &m.stream {
        st.hub.broadcast(WsServerMsg::StreamStopped { user_id: m.user_id, channel_id: m.channel_id });
        for v in &s.viewers {
            stream_usage(st, m.user_id, v.clone());
        }
    }
    for (streamer, conn, v) in st.voice.drop_viewer(m.user_id) {
        viewer_left(st, streamer, conn, m.user_id, v);
    }
}

fn viewer_left(st: &AppState, streamer: Id, streamer_conn: u64, viewer: Id, v: Viewer) {
    st.hub.send_to_conn(
        streamer_conn,
        Outgoing::Msg(WsServerMsg::StreamViewer { streamer, viewer, codecs: Vec::new(), key: String::new(), joined: false }),
    );
    stream_usage(st, streamer, v);
}

fn stream_usage(st: &AppState, streamer: Id, v: Viewer) {
    let db = st.db.clone();
    tokio::spawn(async move {
        let r = sqlx::query("INSERT INTO stream_usage(streamer_id, viewer_id, started_at, ended_at, relay_bytes) VALUES(?, ?, ?, ?, ?)")
            .bind(streamer)
            .bind(v.user_id)
            .bind(v.started_at)
            .bind(now_ms())
            .bind(v.relay_bytes as i64)
            .execute(&db)
            .await;
        if let Err(e) = r {
            tracing::warn!("stream usage not recorded: {e}");
        }
        if v.relay_bytes > 0 {
            tracing::info!(streamer, viewer = v.user_id, relay_bytes = v.relay_bytes, "stream pair used the relay");
        }
    });
}

fn store_usage(st: &AppState, m: Member) {
    end_media(st, &m);
    let db = st.db.clone();
    tokio::spawn(async move {
        if let Err(e) = record_usage(&db, &m).await {
            tracing::warn!("voice usage not recorded: {e}");
        }
    });
}

pub async fn record_usage(db: &sqlx::SqlitePool, m: &Member) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO voice_usage(user_id, channel_id, started_at, ended_at, bytes_in, bytes_out) VALUES(?, ?, ?, ?, ?, ?)")
        .bind(m.user_id)
        .bind(m.channel_id)
        .bind(m.joined_at)
        .bind(now_ms())
        .bind(m.bytes_in as i64)
        .bind(m.bytes_out as i64)
        .execute(db)
        .await?;
    Ok(())
}

async fn channel_info(st: &AppState, channel_id: Id) -> Option<(ChannelKind, Id)> {
    let row: Option<(String, Id)> = sqlx::query_as("SELECT kind, owner_id FROM channels WHERE id = ?")
        .bind(channel_id)
        .fetch_optional(&st.db)
        .await
        .ok()?;
    row.and_then(|(k, o)| ChannelKind::parse(&k).map(|k| (k, o)))
}

pub async fn join(st: &AppState, user_id: Id, conn_id: u64, channel_id: Id) {
    if !matches!(channel_info(st, channel_id).await, Some((ChannelKind::Voice, _))) {
        return ended(st, conn_id, GONE);
    }
    let j = match st.voice.join(user_id, conn_id, channel_id, now_ms()) {
        Ok(j) => j,
        Err(rooms::JoinError::Full) => return ended(st, conn_id, FULL),
    };
    if let Some(prev) = j.previous {
        if prev.conn_id != conn_id {
            ended(st, prev.conn_id, MOVED);
        }
        if prev.channel_id == channel_id {
            // Same channel (typically a reconnect): the member never really left.
            store_usage(st, prev);
        } else {
            left(st, prev);
        }
    }
    let m = j.member;
    st.hub.send_to_conn(
        conn_id,
        Outgoing::Msg(WsServerMsg::VoiceJoined {
            channel_id,
            session: m.session,
            key: m.key.to_b64(),
            udp_port: st.cfg.stun_ports[0],
        }),
    );
    st.hub.broadcast(WsServerMsg::VoiceMemberUpdated(m.public()));
}

pub fn leave(st: &AppState, user_id: Id) {
    if let Some(m) = st.voice.leave_user(user_id) {
        left(st, m);
    }
}

pub fn set_state(st: &AppState, user_id: Id, muted: bool, deafened: bool) {
    if let Some(v) = st.voice.set_state(user_id, muted, deafened) {
        st.hub.broadcast(WsServerMsg::VoiceMemberUpdated(v));
    }
}

/// Disconnect `target` from voice; allowed for admins and the channel's owner.
pub async fn kick(st: &AppState, by: Id, by_admin: bool, target: Id) {
    let Some(m) = st.voice.member(target) else { return };
    let allowed = by_admin || matches!(channel_info(st, m.channel_id).await, Some((_, owner)) if owner == by);
    if !allowed {
        return;
    }
    if let Some(m) = st.voice.leave_user(target) {
        ended(st, m.conn_id, KICKED);
        left(st, m);
    }
}

pub fn conn_closed(st: &AppState, conn_id: u64) {
    if let Some(m) = st.voice.leave_conn(conn_id) {
        left(st, m);
    }
}

pub fn channel_deleted(st: &AppState, channel_id: Id) {
    for m in st.voice.end_channel(channel_id) {
        ended(st, m.conn_id, DELETED);
        left(st, m);
    }
}

const MAX_CODECS: usize = 8;
const MAX_LABEL: usize = 32;
const MAX_SIGNAL: usize = 4096;

fn clean(codecs: Vec<String>) -> Vec<String> {
    codecs.into_iter().filter(|c| !c.is_empty() && c.len() <= MAX_LABEL).take(MAX_CODECS).collect()
}

pub fn stream_start(st: &AppState, user_id: Id, codecs: Vec<String>, preset: String) {
    let codecs = clean(codecs);
    if codecs.is_empty() || preset.len() > MAX_LABEL {
        return;
    }
    if let Some(info) = st.voice.start_stream(user_id, codecs, preset) {
        tracing::info!(user_id, channel_id = info.channel_id, "stream started");
        st.hub.broadcast(WsServerMsg::StreamStarted(info));
    }
}

pub fn stream_stop(st: &AppState, user_id: Id) {
    if let Some((info, viewers)) = st.voice.stop_stream(user_id) {
        st.hub.broadcast(WsServerMsg::StreamStopped { user_id, channel_id: info.channel_id });
        for v in viewers {
            stream_usage(st, user_id, v);
        }
    }
}

pub fn stream_watch(st: &AppState, viewer: Id, streamer: Id, codecs: Vec<String>) {
    let Some((key, s_conn, v_conn)) = st.voice.watch(viewer, streamer, now_ms()) else { return };
    let msg = WsServerMsg::StreamViewer { streamer, viewer, codecs: clean(codecs), key, joined: true };
    st.hub.send_to_conn(s_conn, Outgoing::Msg(msg.clone()));
    st.hub.send_to_conn(v_conn, Outgoing::Msg(msg));
}

pub fn stream_unwatch(st: &AppState, viewer: Id, streamer: Id) {
    if let Some((v, s_conn)) = st.voice.unwatch(viewer, streamer) {
        viewer_left(st, streamer, s_conn, viewer, v);
    }
}

pub fn stream_signal(st: &AppState, from: Id, to: Id, data: serde_json::Value) {
    if data.to_string().len() > MAX_SIGNAL {
        return;
    }
    if let Some(conn) = st.voice.pair_conn(from, to) {
        st.hub.send_to_conn(conn, Outgoing::Msg(WsServerMsg::StreamSignal { from, data }));
    }
}
