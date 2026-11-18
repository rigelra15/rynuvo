//! Shared protocol types between rynuvo-server and rynuvo-desktop.

pub mod e2ee;
pub mod voice_wire;

use serde::{Deserialize, Deserializer, Serialize};

pub const PROTOCOL_VERSION: u32 = 4;
/// Max length of a base64 key envelope relayed by the server.
pub const MAX_ENVELOPE_B64: usize = 128;
pub const CLOSE_AUTH_REVOKED: u16 = 4001;
pub const CLOSE_PROTOCOL_OLD: u16 = 4002;

pub type Id = i64;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: Id,
    pub nickname: String,
    pub is_admin: bool,
    /// One of [`AVATAR_ICONS`]; `None` shows the nickname's first letter.
    #[serde(default)]
    pub avatar_icon: Option<String>,
    /// One of [`AVATAR_COLORS`] or a custom `#rrggbb` (see [`is_avatar_color`]); `None` picks a colour from the user id.
    #[serde(default)]
    pub avatar_color: Option<String>,
    /// An uploaded picture (media id) shown instead of the icon; `None` uses the icon or letter.
    #[serde(default)]
    pub avatar_image: Option<String>,
}

/// Avatar icons a user can pick (drawn by the desktop app).
pub const AVATAR_ICONS: [&str; 24] = [
    "cat", "dog", "rabbit", "bird", "fish", "turtle", "ghost", "skull", "rocket", "gamepad", "headphones", "music",
    "coffee", "pizza", "flame", "zap", "star", "moon", "sun", "leaf", "flower", "crown", "anchor", "camera",
];

/// Avatar colours a user can pick (the desktop app maps each key to a shade).
pub const AVATAR_COLORS: [&str; 12] =
    ["violet", "blue", "sky", "teal", "green", "lime", "amber", "orange", "red", "pink", "fuchsia", "slate"];

/// A colour a user may pick: a palette key, or any `#rrggbb` (lowercase), like one taken from their photo.
pub fn is_avatar_color(s: &str) -> bool {
    AVATAR_COLORS.contains(&s) || (s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ChannelKind {
    Text,
    Voice,
}

impl ChannelKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ChannelKind::Text => "text",
            ChannelKind::Voice => "voice",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "text" => Some(ChannelKind::Text),
            "voice" => Some(ChannelKind::Voice),
            _ => None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    pub id: Id,
    pub kind: ChannelKind,
    pub name: String,
    pub topic: Option<String>,
    pub owner_id: Id,
    pub position: i64,
}

/// How far I've read a text channel: unread while `last_message` is past `last_read`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReadMark {
    pub channel_id: Id,
    /// The newest message in the channel (None: it has none).
    pub last_message: Option<Id>,
    /// The newest message I've seen there (0: none yet).
    pub last_read: Id,
}

/// Someone connected to a voice channel.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VoiceMember {
    pub user_id: Id,
    pub channel_id: Id,
    pub muted: bool,
    pub deafened: bool,
    /// Server time (ms) they joined this channel; 0 from servers before 0.5.1.
    #[serde(default)]
    pub joined_at: i64,
    /// Server time (ms) the call in this channel began: when someone joined it empty. 0 from older servers.
    #[serde(default)]
    pub call_since: i64,
    /// Base64 X25519 public key for E2EE key exchange; empty from older clients.
    #[serde(default)]
    pub pubkey: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImageRef {
    pub id: String,
    pub width: u32,
    pub height: u32,
    pub mime: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: Id,
    pub channel_id: Id,
    pub author_id: Id,
    pub content: String,
    pub image: Option<ImageRef>,
    /// unix milliseconds
    pub created_at: i64,
    pub edited_at: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JoinReq {
    pub invite_code: String,
    pub nickname: String,
    pub device_name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JoinResp {
    pub token: String,
    pub user: User,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClaimTransferReq {
    pub code: String,
    pub device_name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PushRegisterReq {
    /// The device's Firebase Cloud Messaging token.
    pub token: String,
}

/// What a push needs to fetch its notification: a key that can read notifications and nothing else.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PushRegisterResp {
    pub key: String,
}

/// A notification as the phone shows it. Pushes carry only its id; the phone fetches this from the
/// server, so what was said never passes through Google.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PushNote {
    /// "message", "invite", or "room" (someone is hanging out in a voice room).
    pub kind: String,
    pub channel_id: Id,
    pub channel: String,
    /// Who: their nickname and id (for their avatar).
    pub from: String,
    pub from_id: Id,
    /// Their profile photo (a media id; fetched with the push key from `/push/avatar/{from_id}`),
    /// and their avatar colour for when there's none.
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    /// The text to show: a message's first 200 characters, or "sent an image".
    pub text: String,
    pub mention: bool,
    pub at: i64,
    /// A message's picture (media id, fetched with the push key from `/push/media/{id}`).
    #[serde(default)]
    pub image: Option<String>,
    /// "room": who is in there now, the one who just came in first.
    #[serde(default)]
    pub people: Vec<PushPerson>,
    /// Update what's showing without a sound (this room already rang this phone).
    #[serde(default)]
    pub quiet: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PushPerson {
    pub id: Id,
    pub name: String,
    pub avatar: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TransferCodeResp {
    pub code: String,
    pub expires_at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProfileReq {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    /// Absent: unchanged. `null`: cleared.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "some")]
    pub avatar_icon: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "some")]
    pub avatar_color: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "some")]
    pub avatar_image: Option<Option<String>>,
}

/// Present field (even `null`) → `Some`, so absent and `null` differ.
fn some<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CreateChannelReq {
    pub kind: ChannelKind,
    pub name: String,
}

fn double_option<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateChannelReq {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "double_option", skip_serializing_if = "Option::is_none")]
    pub topic: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PostMessageReq {
    pub content: String,
    #[serde(default)]
    pub image_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EditMessageReq {
    pub content: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RotateInviteResp {
    pub code: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

/// A song as the requester's app found it on YouTube Music.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MusicEntryReq {
    pub video_id: String,
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub album: Option<String>,
    /// Cover image (https), may be empty.
    #[serde(default)]
    pub art_url: String,
    pub duration_ms: u32,
    /// A music video (shown on the player) rather than a plain song.
    #[serde(default)]
    pub video: bool,
}

/// A song in a voice channel's music queue.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MusicEntry {
    pub id: u64,
    pub video_id: String,
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub album: Option<String>,
    #[serde(default)]
    pub art_url: String,
    pub duration_ms: u32,
    pub requested_by: Id,
    /// A music video (shown on the player) rather than a plain song.
    #[serde(default)]
    pub video: bool,
}

/// What a voice channel is listening to. Apps play `now` at position `server_time - started_at`
/// (adjusted for their clock offset), or hold at `paused_at`. `now: None` = no music.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MusicState {
    pub channel_id: Id,
    pub now: Option<MusicEntry>,
    pub started_at: i64,
    pub paused_at: Option<u32>,
    pub queue: Vec<MusicEntry>,
    pub server_time: i64,
    /// Where everyone in the channel gets lyrics from: one of `LYRICS_SOURCES`.
    #[serde(default = "auto_lyrics")]
    pub lyrics: String,
    /// Loop mode: "off", "song" (play it again) or "queue" (finished songs go to the back).
    #[serde(default = "repeat_off")]
    pub repeat: String,
    /// Who voted to skip the playing song, and how many votes it takes (0 = no vote going).
    #[serde(default)]
    pub skip_votes: Vec<Id>,
    #[serde(default)]
    pub skip_needed: u32,
}

fn repeat_off() -> String {
    "off".into()
}

fn auto_lyrics() -> String {
    "auto".into()
}

/// Lyrics sources a voice channel can pick (id, name). "auto" tries them in a good order.
pub const LYRICS_SOURCES: &[(&str, &str)] = &[
    ("auto", "Auto"),
    ("betterlyrics", "BetterLyrics"),
    ("lrcred", "LrcRed"),
    ("simpmusic", "SimpMusic"),
    ("lrclib", "LRCLIB"),
    ("netease", "NetEase"),
];

/// A music command for the voice channel the sender is in.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum MusicCmd {
    Add { entry: MusicEntryReq },
    /// Pick the channel's lyrics source (owner or admin only).
    Lyrics { source: String },
    /// Set the loop mode ("off", "song", "queue"); none = the next one (off → queue → song → off).
    Repeat {
        #[serde(default)]
        mode: Option<String>,
    },
    Skip,
    Pause,
    Resume,
    Seek { ms: u32 },
    /// 1-based queue position.
    Remove { index: u32 },
    Stop,
    Queue,
    NowPlaying,
    /// The requester's search found nothing (the bot says so).
    NotFound { query: String },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "t", content = "d", rename_all = "camelCase")]
pub enum WsClientMsg {
    #[serde(rename_all = "camelCase")]
    Hello {
        token: String,
        protocol: u32,
        /// This connection is a phone or tablet app. Older clients leave it out.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        mobile: bool,
    },
    #[serde(rename_all = "camelCase")]
    Typing { channel_id: Id },
    /// I've seen `channel_id` up to `message_id` (only ever moves forward).
    #[serde(rename_all = "camelCase")]
    Read { channel_id: Id, message_id: Id },
    Ping,
    #[serde(rename_all = "camelCase")]
    VoiceJoin { channel_id: Id, pubkey: String },
    #[serde(rename_all = "camelCase")]
    E2eeKey { to: Id, scope: e2ee::Scope, generation: u32, envelope: String },
    #[serde(rename_all = "camelCase")]
    E2eeKeyRequest { from: Id, scope: e2ee::Scope },
    VoiceLeave,
    #[serde(rename_all = "camelCase")]
    VoiceState { muted: bool, deafened: bool },
    #[serde(rename_all = "camelCase")]
    VoiceKick { user_id: Id },
    /// Ask these people to come to the voice channel I'm in ("come hang out").
    VoiceInvite { to: Vec<Id> },
    /// The app went to the background (true) or came back (false). While away, this device gets
    /// push notifications as if it weren't connected.
    Away { away: bool },
    /// The game I'm playing now, or none. Shown to others ("Playing …").
    Activity { game: Option<String> },
    /// Go Live in my current voice channel. `codecs`: what I can encode, best first.
    #[serde(rename_all = "camelCase")]
    StreamStart { codecs: Vec<String>, preset: String },
    StreamStop,
    /// Start watching `user_id`'s stream. `codecs`: what I can decode.
    #[serde(rename_all = "camelCase")]
    StreamWatch { user_id: Id, codecs: Vec<String> },
    #[serde(rename_all = "camelCase")]
    StreamUnwatch { user_id: Id },
    /// Opaque connection setup data (P2P candidates) for a streamer–viewer pair.
    #[serde(rename_all = "camelCase")]
    StreamSignal { to: Id, data: serde_json::Value },
    /// `text_channel_id`: where the command was typed (the bot answers there); `None` for buttons.
    #[serde(rename_all = "camelCase")]
    Music { cmd: MusicCmd, text_channel_id: Option<Id> },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StreamInfo {
    pub user_id: Id,
    pub channel_id: Id,
    pub codecs: Vec<String>,
    pub preset: String,
}

/// Someone's game: what they're playing now (none: nothing).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    pub user_id: Id,
    pub game: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LastSeen {
    pub user_id: Id,
    pub at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "t", content = "d", rename_all = "camelCase")]
pub enum WsServerMsg {
    #[serde(rename_all = "camelCase")]
    Ready {
        me: User,
        users: Vec<User>,
        channels: Vec<Channel>,
        online: Vec<Id>,
        /// Of `online`: those connected only from phones or tablets.
        #[serde(default)]
        mobile: Vec<Id>,
        /// When each user was last connected (ms), for those who ever were.
        #[serde(default)]
        last_seen: Vec<LastSeen>,
        alerts: Vec<String>,
        #[serde(default)]
        voice: Vec<VoiceMember>,
        #[serde(default)]
        streams: Vec<StreamInfo>,
        #[serde(default)]
        music: Vec<MusicState>,
        /// The "Rynuvo Music" user: shown as the author of its replies, never as a member.
        #[serde(default)]
        music_bot: Option<Id>,
        /// Where I am in each text channel, for its unread dot.
        #[serde(default)]
        reads: Vec<ReadMark>,
        /// Who is playing what right now.
        #[serde(default)]
        activities: Vec<Activity>,
    },
    /// I read further on another of my devices.
    #[serde(rename_all = "camelCase")]
    ReadUpdated { channel_id: Id, last_read: Id },
    /// A transfer code of mine was just used to sign in a new device (`expires_at` tells which code).
    #[serde(rename_all = "camelCase")]
    TransferClaimed { expires_at: i64, device: String },
    MessageCreated(Message),
    MessageUpdated(Message),
    #[serde(rename_all = "camelCase")]
    MessageDeleted { id: Id, channel_id: Id },
    ChannelCreated(Channel),
    ChannelUpdated(Channel),
    #[serde(rename_all = "camelCase")]
    ChannelDeleted { id: Id },
    UserUpserted(User),
    #[serde(rename_all = "camelCase")]
    UserRemoved { id: Id },
    #[serde(rename_all = "camelCase")]
    Presence {
        user_id: Id,
        online: bool,
        /// Online only from phones or tablets (no computer connected).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        mobile: bool,
        /// When going offline: the moment they were last connected (ms).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        last_seen: Option<i64>,
    },
    #[serde(rename_all = "camelCase")]
    Typing { channel_id: Id, user_id: Id },
    /// Someone started or stopped playing a game.
    Activity(Activity),
    #[serde(rename_all = "camelCase")]
    AdminAlert { message: String },
    Pong,
    /// Sent only to the connection that joined: credentials for the media relay.
    #[serde(rename_all = "camelCase")]
    VoiceJoined { channel_id: Id, session: u32, key: String, udp_port: u16 },
    /// `from` asks me to come to voice channel `channel_id`.
    #[serde(rename_all = "camelCase")]
    VoiceInvited { channel_id: Id, from: Id },
    VoiceMemberUpdated(VoiceMember),
    #[serde(rename_all = "camelCase")]
    VoiceMemberLeft { user_id: Id, channel_id: Id },
    /// Your voice connection was ended by the server; `reason` is shown to the user.
    #[serde(rename_all = "camelCase")]
    VoiceEnded { reason: String },
    StreamStarted(StreamInfo),
    #[serde(rename_all = "camelCase")]
    StreamStopped { user_id: Id, channel_id: Id },
    /// To both sides of a pair when a viewer starts (`joined`) or stops watching. `key` encrypts
    /// their direct P2P packets; `codecs` is the viewer's decodable list.
    #[serde(rename_all = "camelCase")]
    StreamViewer { streamer: Id, viewer: Id, codecs: Vec<String>, key: String, joined: bool },
    #[serde(rename_all = "camelCase")]
    StreamSignal { from: Id, data: serde_json::Value },
    MusicState(MusicState),
    #[serde(rename_all = "camelCase")]
    E2eeKey { from: Id, scope: e2ee::Scope, generation: u32, envelope: String },
    #[serde(rename_all = "camelCase")]
    E2eeKeyRequest { by: Id, scope: e2ee::Scope },
}

/// Does `content` @mention `me` (or @everyone)? Same rules as the app's `findMentions`: case-insensitive,
/// word boundaries on both sides, and the longest matching name wins ("@Budi Santoso" is not "@Budi").
pub fn mentions_me(content: &str, me: &str, names: &[&str]) -> bool {
    let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    let mut names: Vec<String> = names.iter().map(|n| n.to_lowercase()).chain(["everyone".to_string()]).collect();
    names.sort_by_key(|n| std::cmp::Reverse(n.chars().count()));
    let me = me.to_lowercase();
    let chars: Vec<char> = content.chars().collect();
    let lower: Vec<char> = chars.iter().map(|c| c.to_lowercase().next().unwrap_or(*c)).collect();
    for i in 0..chars.len() {
        if chars[i] != '@' || (i > 0 && word(Some(chars[i - 1]))) {
            continue;
        }
        let hit = names.iter().find(|n| {
            let n: Vec<char> = n.chars().collect();
            let end = i + 1 + n.len();
            end <= lower.len() && lower[i + 1..end] == n[..] && !word(chars.get(end).copied())
        });
        if let Some(n) = hit {
            if *n == me || n == "everyone" {
                return true;
            }
        }
    }
    false
}


#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn music_msgs_shape() {
        let v = serde_json::to_value(WsClientMsg::Music { cmd: MusicCmd::Seek { ms: 1000 }, text_channel_id: None }).unwrap();
        assert_eq!(v, json!({"t":"music","d":{"cmd":{"op":"seek","ms":1000},"textChannelId":null}}));
        let add: WsClientMsg = serde_json::from_value(json!({"t":"music","d":{"textChannelId":4,"cmd":{"op":"add","entry":{
            "videoId":"abc","title":"Stuck","artist":"LANY","durationMs":200000}}}})).unwrap();
        assert!(matches!(add, WsClientMsg::Music { cmd: MusicCmd::Add { entry }, text_channel_id: Some(4) } if entry.art_url.is_empty()));
    }

    #[test]
    fn stream_msgs_shape() {
        let v = serde_json::to_value(WsClientMsg::StreamWatch { user_id: 3, codecs: vec!["h264".into()] }).unwrap();
        assert_eq!(v, json!({"t":"streamWatch","d":{"userId":3,"codecs":["h264"]}}));
        let v = serde_json::to_value(WsServerMsg::StreamViewer { streamer: 1, viewer: 2, codecs: vec![], key: "k".into(), joined: true }).unwrap();
        assert_eq!(v, json!({"t":"streamViewer","d":{"streamer":1,"viewer":2,"codecs":[],"key":"k","joined":true}}));
        let s = StreamInfo { user_id: 1, channel_id: 2, codecs: vec!["hevc".into()], preset: "1080p60".into() };
        let v = serde_json::to_value(WsServerMsg::StreamStarted(s.clone())).unwrap();
        assert_eq!(v, json!({"t":"streamStarted","d":{"userId":1,"channelId":2,"codecs":["hevc"],"preset":"1080p60"}}));
        assert_eq!(serde_json::from_value::<WsServerMsg>(v).unwrap(), WsServerMsg::StreamStarted(s));
        assert_eq!(serde_json::to_value(WsClientMsg::StreamStop).unwrap(), json!({"t":"streamStop"}));
    }

    #[test]
    fn e2ee_msgs_roundtrip() {
        use crate::e2ee::Scope;
        let m = WsClientMsg::E2eeKey { to: 7, scope: Scope::Stream(3), generation: 2, envelope: "AAAA".into() };
        let v = serde_json::to_value(&m).unwrap();
        assert_eq!(v, json!({"t":"e2eeKey","d":{"to":7,"scope":{"kind":"stream","id":3},"generation":2,"envelope":"AAAA"}}));
        assert_eq!(serde_json::from_value::<WsClientMsg>(v).unwrap(), m);
        let r = WsClientMsg::E2eeKeyRequest { from: 4, scope: Scope::Voice(9) };
        assert_eq!(serde_json::from_value::<WsClientMsg>(serde_json::to_value(&r).unwrap()).unwrap(), r);
        let s = WsServerMsg::E2eeKey { from: 1, scope: Scope::Stream(3), generation: 0, envelope: "x".into() };
        assert_eq!(serde_json::from_value::<WsServerMsg>(serde_json::to_value(&s).unwrap()).unwrap(), s);
        let q = WsServerMsg::E2eeKeyRequest { by: 2, scope: Scope::Voice(1) };
        assert_eq!(serde_json::from_value::<WsServerMsg>(serde_json::to_value(&q).unwrap()).unwrap(), q);
        assert_eq!(PROTOCOL_VERSION, 4);
    }

    #[test]
    fn voice_member_without_pubkey_defaults_empty() {
        let m: VoiceMember = serde_json::from_value(json!({"userId":1,"channelId":2,"muted":false,"deafened":false})).unwrap();
        assert_eq!(m.pubkey, "");
    }

    #[test]
    fn voice_msgs_shape() {
        let v = serde_json::to_value(WsClientMsg::VoiceJoin { channel_id: 5, pubkey: "pk".into() }).unwrap();
        assert_eq!(v, json!({"t":"voiceJoin","d":{"channelId":5,"pubkey":"pk"}}));
        let v = serde_json::to_value(WsClientMsg::VoiceState { muted: true, deafened: false }).unwrap();
        assert_eq!(v, json!({"t":"voiceState","d":{"muted":true,"deafened":false}}));
        let leave: WsClientMsg = serde_json::from_str(r#"{"t":"voiceLeave"}"#).unwrap();
        assert_eq!(leave, WsClientMsg::VoiceLeave);
        let m = VoiceMember { user_id: 1, channel_id: 2, muted: false, deafened: true, joined_at: 10, call_since: 5, pubkey: String::new() };
        let v = serde_json::to_value(WsServerMsg::VoiceMemberUpdated(m)).unwrap();
        assert_eq!(v, json!({"t":"voiceMemberUpdated","d":{"userId":1,"channelId":2,"muted":false,"deafened":true,"joinedAt":10,"callSince":5,"pubkey":""}}));
        let old: VoiceMember = serde_json::from_value(json!({"userId":1,"channelId":2,"muted":false,"deafened":true})).unwrap();
        assert_eq!((old.joined_at, old.call_since), (0, 0));
        let v = serde_json::to_value(WsServerMsg::VoiceJoined { channel_id: 2, session: 9, key: "k".into(), udp_port: 3478 }).unwrap();
        assert_eq!(v, json!({"t":"voiceJoined","d":{"channelId":2,"session":9,"key":"k","udpPort":3478}}));
    }

    #[test]
    fn ready_without_voice_field_parses() {
        let s = r#"{"t":"ready","d":{"me":{"id":1,"nickname":"x","isAdmin":false},"users":[],"channels":[],"online":[],"alerts":[]}}"#;
        match serde_json::from_str::<WsServerMsg>(s).unwrap() {
            WsServerMsg::Ready { voice, .. } => assert!(voice.is_empty()),
            m => panic!("{m:?}"),
        }
    }

    #[test]
    fn ws_server_msg_shape() {
        let v = serde_json::to_value(WsServerMsg::Presence { user_id: 7, online: true, mobile: false, last_seen: None }).unwrap();
        assert_eq!(v, json!({"t":"presence","d":{"userId":7,"online":true}}));
    }

    #[test]
    fn ws_client_hello_roundtrip() {
        let m = WsClientMsg::Hello { token: "abc".into(), protocol: PROTOCOL_VERSION, mobile: false };
        let s = serde_json::to_string(&m).unwrap();
        assert_eq!(s, r#"{"t":"hello","d":{"token":"abc","protocol":4}}"#);
        assert_eq!(serde_json::from_str::<WsClientMsg>(&s).unwrap(), m);
        let ping: WsClientMsg = serde_json::from_str(r#"{"t":"ping"}"#).unwrap();
        assert_eq!(ping, WsClientMsg::Ping);
    }

    #[test]
    fn update_channel_topic_null_vs_absent() {
        let a: UpdateChannelReq = serde_json::from_str(r#"{"topic":null}"#).unwrap();
        assert_eq!(a.topic, Some(None));
        let b: UpdateChannelReq = serde_json::from_str("{}").unwrap();
        assert_eq!(b.topic, None);
        let c: UpdateChannelReq = serde_json::from_str(r#"{"topic":"x"}"#).unwrap();
        assert_eq!(c.topic, Some(Some("x".into())));
    }
}
