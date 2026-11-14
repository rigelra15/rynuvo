//! Voice channels: membership, signaling over the WebSocket, and usage accounting.
pub mod rooms;

pub use rooms::{Member, VoiceRooms};

use rynuvo_proto::{ChannelKind, Id, WsServerMsg};

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
        left(st, prev);
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
