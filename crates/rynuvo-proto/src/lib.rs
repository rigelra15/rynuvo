//! Shared protocol types between rynuvo-server and rynuvo-desktop.

pub mod voice_wire;

use serde::{Deserialize, Deserializer, Serialize};

pub const PROTOCOL_VERSION: u32 = 3;
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
    /// One of [`AVATAR_COLORS`]; `None` picks a colour from the user id.
    #[serde(default)]
    pub avatar_color: Option<String>,
}

/// Avatar icons a user can pick (drawn by the desktop app).
pub const AVATAR_ICONS: [&str; 24] = [
    "cat", "dog", "rabbit", "bird", "fish", "turtle", "ghost", "skull", "rocket", "gamepad", "headphones", "music",
    "coffee", "pizza", "flame", "zap", "star", "moon", "sun", "leaf", "flower", "crown", "anchor", "camera",
];

/// Avatar colours a user can pick (the desktop app maps each key to a shade).
pub const AVATAR_COLORS: [&str; 12] =
    ["violet", "blue", "sky", "teal", "green", "lime", "amber", "orange", "red", "pink", "fuchsia", "slate"];

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
    Hello { token: String, protocol: u32 },
    #[serde(rename_all = "camelCase")]
    Typing { channel_id: Id },
    Ping,
    #[serde(rename_all = "camelCase")]
    VoiceJoin { channel_id: Id },
    VoiceLeave,
    #[serde(rename_all = "camelCase")]
    VoiceState { muted: bool, deafened: bool },
    #[serde(rename_all = "camelCase")]
    VoiceKick { user_id: Id },
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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "t", content = "d", rename_all = "camelCase")]
pub enum WsServerMsg {
    #[serde(rename_all = "camelCase")]
    Ready {
        me: User,
        users: Vec<User>,
        channels: Vec<Channel>,
        online: Vec<Id>,
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
    },
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
    Presence { user_id: Id, online: bool },
    #[serde(rename_all = "camelCase")]
    Typing { channel_id: Id, user_id: Id },
    #[serde(rename_all = "camelCase")]
    AdminAlert { message: String },
    Pong,
    /// Sent only to the connection that joined: credentials for the media relay.
    #[serde(rename_all = "camelCase")]
    VoiceJoined { channel_id: Id, session: u32, key: String, udp_port: u16 },
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
    fn voice_msgs_shape() {
        let v = serde_json::to_value(WsClientMsg::VoiceJoin { channel_id: 5 }).unwrap();
        assert_eq!(v, json!({"t":"voiceJoin","d":{"channelId":5}}));
        let v = serde_json::to_value(WsClientMsg::VoiceState { muted: true, deafened: false }).unwrap();
        assert_eq!(v, json!({"t":"voiceState","d":{"muted":true,"deafened":false}}));
        let leave: WsClientMsg = serde_json::from_str(r#"{"t":"voiceLeave"}"#).unwrap();
        assert_eq!(leave, WsClientMsg::VoiceLeave);
        let m = VoiceMember { user_id: 1, channel_id: 2, muted: false, deafened: true, joined_at: 10, call_since: 5 };
        let v = serde_json::to_value(WsServerMsg::VoiceMemberUpdated(m)).unwrap();
        assert_eq!(v, json!({"t":"voiceMemberUpdated","d":{"userId":1,"channelId":2,"muted":false,"deafened":true,"joinedAt":10,"callSince":5}}));
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
        let v = serde_json::to_value(WsServerMsg::Presence { user_id: 7, online: true }).unwrap();
        assert_eq!(v, json!({"t":"presence","d":{"userId":7,"online":true}}));
    }

    #[test]
    fn ws_client_hello_roundtrip() {
        let m = WsClientMsg::Hello { token: "abc".into(), protocol: PROTOCOL_VERSION };
        let s = serde_json::to_string(&m).unwrap();
        assert_eq!(s, r#"{"t":"hello","d":{"token":"abc","protocol":3}}"#);
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
