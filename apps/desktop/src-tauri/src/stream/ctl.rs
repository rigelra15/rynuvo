//! Go Live control: my own stream and the streams I watch, driven by commands and server messages.
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use rynuvo_proto::{Id, WsClientMsg, WsServerMsg, voice_wire::VoiceKey};

use super::{
    capture::{AudioSink, CaptureOpts, VideoSink},
    engine::{self, Broadcast, EventSink, FrameOut, Net, Stopper, StreamEvent, Watch},
};

/// Starts capturing `source_id` (from `capture::sources`) into the sinks.
pub type StartCapture = Arc<dyn Fn(&str, CaptureOpts, VideoSink, AudioSink) -> Result<Stopper, String> + Send + Sync>;
use crate::{core::session::SessionEvent, voice::ctl::VoiceCtl};

/// The server's STUN ports (both answer, which also tells us the NAT type).
pub const STUN_PORTS: [u16; 2] = [3478, 3479];

#[derive(Default)]
struct State {
    live: Option<Broadcast>,
    watches: HashMap<Id, (Watch, Option<FrameOut>)>,
    stun: Vec<SocketAddr>,
}

#[derive(Clone)]
pub struct StreamCtl {
    st: Arc<Mutex<State>>,
    voice: VoiceCtl,
    capture: StartCapture,
    events: EventSink,
    rt: tokio::runtime::Handle,
}

impl StreamCtl {
    pub fn new(voice: VoiceCtl, capture: StartCapture, events: EventSink, rt: tokio::runtime::Handle) -> StreamCtl {
        let ctl = StreamCtl { st: Arc::default(), voice: voice.clone(), capture, events, rt };
        let weak = Arc::downgrade(&ctl.st);
        voice.set_video_sink(Arc::new(move |from, data: &[u8]| {
            let Some(st) = weak.upgrade() else { return };
            let s = st.lock().unwrap();
            if let Some(b) = s.live.as_ref().filter(|b| b.has_viewer(from)) {
                b.on_relay(from, data);
            } else if let Some((w, _)) = s.watches.get(&from) {
                w.on_relay(data);
            }
        }));
        ctl
    }

    /// STUN servers for direct-path candidates (resolved from the server host on connect).
    pub fn set_stun(&self, stun: Vec<SocketAddr>) {
        self.st.lock().unwrap().stun = stun;
    }

    fn net(&self) -> Net {
        let v1 = self.voice.clone();
        let v2 = self.voice.clone();
        Net {
            signal: Arc::new(move |to, data| v1.send_ws(WsClientMsg::StreamSignal { to, data })),
            relay: Arc::new(move |to, b| v2.relay_video(to, &b)),
            stun: self.st.lock().unwrap().stun.clone(),
            rt: self.rt.clone(),
        }
    }

    fn emit(&self, e: StreamEvent) {
        (self.events)(e);
    }

    pub fn is_live(&self) -> bool {
        self.st.lock().unwrap().live.is_some()
    }

    pub fn go_live(&self, source_id: &str, preset: &str) -> Result<(), String> {
        if self.voice.channel().is_none() {
            return Err("Join a voice channel first".into());
        }
        self.stop_local();
        let p = engine::preset(preset);
        let source = source_id.to_string();
        let capture = self.capture.clone();
        let start: engine::CaptureFn = Arc::new(move |opts, video, audio| capture(&source, opts, video, audio));
        let b = Broadcast::start(start, p, self.net(), self.events.clone())?;
        self.st.lock().unwrap().live = Some(b);
        let codecs = super::encode::available().iter().map(|c| c.name().to_string()).collect();
        self.voice.send_ws(WsClientMsg::StreamStart { codecs, preset: preset.to_string() });
        Ok(())
    }

    fn stop_local(&self) -> bool {
        let b = self.st.lock().unwrap().live.take();
        let was = b.is_some();
        if let Some(b) = b {
            b.stop();
        }
        was
    }

    pub fn stop_live(&self) {
        if self.stop_local() {
            self.voice.send_ws(WsClientMsg::StreamStop);
        }
    }

    /// Start watching `user_id`. `codecs`: what the webview can decode.
    pub fn watch(&self, user_id: Id, codecs: Vec<String>, out: FrameOut) -> Result<(), String> {
        if self.voice.channel().is_none() {
            return Err("Join the voice channel to watch".into());
        }
        let old = self.st.lock().unwrap().watches.insert(user_id, (Watch::new(user_id), Some(out)));
        if let Some((w, _)) = old {
            w.stop();
        }
        self.voice.send_ws(WsClientMsg::StreamWatch { user_id, codecs });
        Ok(())
    }

    pub fn unwatch(&self, user_id: Id) {
        let w = self.st.lock().unwrap().watches.remove(&user_id);
        if let Some((w, _)) = w {
            w.stop();
            self.voice.remove_stream(user_id);
            self.voice.send_ws(WsClientMsg::StreamUnwatch { user_id });
        }
    }

    pub fn set_stream_volume(&self, user_id: Id, v: f32) {
        self.voice.set_stream_volume(user_id, v);
    }

    fn end_watch(&self, user_id: Id, reason: &str) {
        let w = self.st.lock().unwrap().watches.remove(&user_id);
        if let Some((w, _)) = w {
            w.stop();
            self.voice.remove_stream(user_id);
            self.emit(StreamEvent::Ended { user_id, reason: reason.into() });
        }
    }

    /// End my stream and everything I watch (sign-out, lost connection).
    pub fn end_all(&self, reason: &str) {
        let me = self.voice.me();
        if self.stop_local() {
            if let Some(me) = me {
                self.emit(StreamEvent::Ended { user_id: me, reason: reason.into() });
            }
        }
        let ids: Vec<Id> = self.st.lock().unwrap().watches.keys().copied().collect();
        for id in ids {
            self.end_watch(id, reason);
        }
    }

    pub fn on_session(&self, e: &SessionEvent) {
        match e {
            SessionEvent::Server(m) => self.on_server(m),
            SessionEvent::Revoked | SessionEvent::UpdateRequired => self.end_all("Signed out"),
            SessionEvent::Conn(_) => {}
        }
    }

    fn resolve_stun(&self) {
        let host = self.voice.host();
        if host.is_empty() || host.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback()) || host == "localhost" {
            return; // local test servers: no STUN, the relay carries everything
        }
        let st = Arc::downgrade(&self.st);
        self.rt.spawn(async move {
            let mut out = Vec::new();
            for port in STUN_PORTS.iter().take(1) {
                if let Ok(addrs) = tokio::net::lookup_host((host.as_str(), *port)).await {
                    out.extend(addrs);
                }
            }
            if let Some(st) = st.upgrade() {
                st.lock().unwrap().stun = out;
            }
        });
    }

    pub fn on_server(&self, m: &WsServerMsg) {
        let me = self.voice.me();
        match m {
            WsServerMsg::Ready { .. } => {
                // A new connection: the server forgot every stream and pairing of the old one.
                self.end_all("Connection lost");
                self.resolve_stun();
            }
            WsServerMsg::StreamViewer { streamer, viewer, codecs, key, joined } => {
                if Some(*streamer) == me {
                    let s = self.st.lock().unwrap();
                    let Some(b) = s.live.as_ref() else { return };
                    if *joined {
                        match VoiceKey::from_b64(key) {
                            Some(k) => b.add_viewer(*viewer, codecs, k),
                            None => tracing::warn!("stream: bad pair key"),
                        }
                    } else {
                        b.remove_viewer(*viewer);
                    }
                } else if Some(*viewer) == me {
                    if !*joined {
                        self.end_watch(*streamer, "The stream ended");
                        return;
                    }
                    let Some(k) = VoiceKey::from_b64(key) else { return };
                    let net = self.net();
                    let s = self.st.lock().unwrap();
                    let Some((w, out)) = s.watches.get(streamer) else { return };
                    let Some(out) = out.clone() else { return };
                    let voice = self.voice.clone();
                    let ctl = self.clone();
                    let events: EventSink = Arc::new(move |e| match e {
                        StreamEvent::Ended { user_id, reason } => ctl.end_watch(user_id, &reason),
                        e => ctl.emit(e),
                    });
                    w.connect(k, &net, out, Arc::new(move |u, pcm| voice.stream_audio(u, pcm)), events);
                }
            }
            WsServerMsg::StreamSignal { from, data } => {
                let s = self.st.lock().unwrap();
                if let Some(b) = s.live.as_ref().filter(|b| b.has_viewer(*from)) {
                    b.on_signal(*from, data);
                } else if let Some((w, _)) = s.watches.get(from) {
                    w.on_signal(data);
                }
            }
            WsServerMsg::StreamStopped { user_id, .. } => {
                if Some(*user_id) == me {
                    if self.stop_local() {
                        self.emit(StreamEvent::Ended { user_id: *user_id, reason: "Your stream ended".into() });
                    }
                } else {
                    self.end_watch(*user_id, "The stream ended");
                }
            }
            WsServerMsg::VoiceEnded { .. } => self.end_all("You left voice"),
            _ => {}
        }
    }
}
