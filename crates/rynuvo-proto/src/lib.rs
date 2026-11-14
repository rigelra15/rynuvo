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

/// Someone connected to a voice channel.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VoiceMember {
    pub user_id: Id,
    pub channel_id: Id,
    pub muted: bool,
    pub deafened: bool,
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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RenameReq {
    pub nickname: String,
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
        let m = VoiceMember { user_id: 1, channel_id: 2, muted: false, deafened: true };
        let v = serde_json::to_value(WsServerMsg::VoiceMemberUpdated(m)).unwrap();
        assert_eq!(v, json!({"t":"voiceMemberUpdated","d":{"userId":1,"channelId":2,"muted":false,"deafened":true}}));
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
