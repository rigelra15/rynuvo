//! Voice control: what channel I want to be in, mute/deafen, and the engine's lifecycle,
//! driven by my commands and by the server's voice messages.
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use rynuvo_proto::{Id, VoiceMember, WsClientMsg, WsServerMsg, voice_wire::VoiceKey};
use tokio::sync::mpsc;

use super::{
    backend::AudioBackend,
    engine::{EngineOpts, EventSink, Tone, VoiceEngine, VoiceEvent},
    link::{LinkCfg, VideoSink},
};
use crate::core::session::{SessionEvent, SessionHandle};

pub type BackendFactory = Arc<dyn Fn() -> Box<dyn AudioBackend> + Send + Sync>;

/// Audio preferences the engine starts with.
#[derive(Debug, Clone)]
pub struct AudioPrefs {
    pub input: Option<String>,
    pub output: Option<String>,
    pub threshold: f32,
    pub volumes: HashMap<Id, f32>,
    pub music_volume: f32,
}

impl Default for AudioPrefs {
    fn default() -> Self {
        AudioPrefs { input: None, output: None, threshold: 0.5, volumes: HashMap::new(), music_volume: 0.5 }
    }
}

/// My own voice state, as the UI shows it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MyVoice {
    pub channel_id: Option<Id>,
    pub muted: bool,
    pub deafened: bool,
}

#[derive(Default)]
struct State {
    session: Option<SessionHandle>,
    host: String,
    me: Option<Id>,
    /// The channel I want to be in; survives reconnects.
    want: Option<Id>,
    muted: bool,
    deafened: bool,
    muted_before_deafen: bool,
    engine: Option<Arc<VoiceEngine>>,
    engine_channel: Option<Id>,
    /// Audio-only engine for the mic test while not in voice.
    test_engine: Option<VoiceEngine>,
    members: Vec<VoiceMember>,
    prefs: AudioPrefs,
    /// Where relayed Go Live data goes (the stream controller); survives engine restarts.
    video_sink: Option<VideoSink>,
}

#[derive(Clone)]
pub struct VoiceCtl {
    st: Arc<Mutex<State>>,
    factory: BackendFactory,
    events: EventSink,
}

impl VoiceCtl {
    pub fn new(factory: BackendFactory, events: EventSink) -> Self {
        VoiceCtl { st: Arc::default(), factory, events }
    }

    /// Bind to a (new) session. `host` is the server's host name, used for the UDP relay.
    pub fn attach(&self, session: SessionHandle, host: String) {
        let old = {
            let mut s = self.st.lock().unwrap();
            s.host = host;
            s.want = None;
            s.members.clear();
            s.session.replace(session)
        };
        if let Some(o) = old {
            o.set_binary_sink(None);
        }
        self.stop_engine();
    }

    /// Signed out: leave voice and forget the session.
    pub fn detach(&self) {
        self.st.lock().unwrap().want = None;
        self.stop_engine();
        self.mic_test(false);
        let mut s = self.st.lock().unwrap();
        if let Some(se) = s.session.take() {
            se.set_binary_sink(None);
        }
        s.members.clear();
    }

    pub fn state(&self) -> MyVoice {
        let s = self.st.lock().unwrap();
        MyVoice { channel_id: s.want, muted: s.muted, deafened: s.deafened }
    }

    fn notify(&self) {
        let m = self.state();
        (self.events)(VoiceEvent::State { channel_id: m.channel_id, muted: m.muted, deafened: m.deafened });
    }

    /// Apply saved audio preferences (devices, threshold, per-person volumes).
    pub fn set_prefs(&self, p: AudioPrefs) {
        self.set_devices(p.input.clone(), p.output.clone());
        self.set_threshold(p.threshold);
        for (u, v) in p.volumes {
            self.set_volume(u, v);
        }
        self.set_music_volume(p.music_volume);
    }

    pub fn channel(&self) -> Option<Id> {
        self.st.lock().unwrap().want
    }

    /// Any message on the current session (used by the stream controller).
    pub fn send_ws(&self, m: WsClientMsg) {
        self.send(m);
    }

    fn send(&self, m: WsClientMsg) {
        if let Some(s) = self.st.lock().unwrap().session.as_ref() {
            s.send(m);
        }
    }

    pub fn join(&self, channel_id: Id) {
        self.st.lock().unwrap().want = Some(channel_id);
        self.send(WsClientMsg::VoiceJoin { channel_id });
        self.notify();
    }

    pub fn leave(&self) {
        tracing::info!("voice: leave");
        let was = self.st.lock().unwrap().want.take();
        if was.is_some() {
            self.send(WsClientMsg::VoiceLeave);
        }
        self.stop_engine_after(Some(Tone::Leave));
        self.notify();
    }

    /// Send my mute/deafen state on, with the sound for what changed (`before`: as it was).
    fn push_state_from(&self, before: (bool, bool)) {
        let after = {
            let s = self.st.lock().unwrap();
            (s.muted, s.deafened)
        };
        self.push_state();
        if let (Some(t), Some(e)) = (toggle_tone(before, after), self.st.lock().unwrap().engine.clone()) {
            e.play_tone(t);
        }
    }

    fn push_state(&self) {
        let (muted, deafened, engine, in_voice) = {
            let s = self.st.lock().unwrap();
            (s.muted, s.deafened, s.engine.clone(), s.want.is_some())
        };
        if let Some(e) = engine {
            e.set_muted(muted);
            e.set_deafened(deafened);
        }
        if in_voice {
            self.send(WsClientMsg::VoiceState { muted, deafened });
        }
    }

    /// Unmuting while deafened also undeafens.
    pub fn set_muted(&self, muted: bool) -> MyVoice {
        let before = {
            let mut s = self.st.lock().unwrap();
            let before = (s.muted, s.deafened);
            s.muted = muted;
            if !muted {
                s.deafened = false;
            }
            before
        };
        self.push_state_from(before);
        self.notify();
        self.state()
    }

    /// Deafening mutes too; undeafening restores the mute state from before.
    pub fn set_deafened(&self, deafened: bool) -> MyVoice {
        let before = {
            let mut s = self.st.lock().unwrap();
            let before = (s.muted, s.deafened);
            if deafened && !s.deafened {
                s.muted_before_deafen = s.muted;
                s.muted = true;
            } else if !deafened && s.deafened {
                s.muted = s.muted_before_deafen;
            }
            s.deafened = deafened;
            before
        };
        self.push_state_from(before);
        self.notify();
        self.state()
    }

    pub fn set_video_sink(&self, f: VideoSink) {
        let mut s = self.st.lock().unwrap();
        if let Some(l) = s.engine.as_ref().and_then(|e| e.link()) {
            l.set_video_sink(f.clone());
        }
        s.video_sink = Some(f);
    }

    /// Go Live data to `to` through the server. Dropped when I'm not in voice (no relay session).
    pub fn relay_video(&self, to: Id, data: &[u8]) {
        let l = self.st.lock().unwrap().engine.as_ref().and_then(|e| e.link());
        if let Some(l) = l {
            l.send_video(to, data);
        }
    }

    pub fn stream_audio(&self, user: Id, samples: Vec<f32>) {
        let e = self.st.lock().unwrap().engine.clone();
        if let Some(e) = e {
            e.stream_audio(user, samples);
        }
    }

    pub fn set_stream_volume(&self, user: Id, v: f32) {
        let e = self.st.lock().unwrap().engine.clone();
        if let Some(e) = e {
            e.set_stream_volume(user, v);
        }
    }

    /// Play (or keep in time) the listen-together song in the voice engine, if I'm in voice.
    pub fn music_set(&self, entry: u64, song: Arc<crate::music::webm::Song>, at_ms: u32, paused: bool) {
        let e = self.st.lock().unwrap().engine.clone();
        if let Some(e) = e {
            e.music_set(entry, song, at_ms, paused);
        }
    }

    pub fn music_clear(&self) {
        let e = self.st.lock().unwrap().engine.clone();
        if let Some(e) = e {
            e.music_clear();
        }
    }

    pub fn set_music_volume(&self, v: f32) {
        let e = {
            let mut s = self.st.lock().unwrap();
            s.prefs.music_volume = v.clamp(0.0, 1.0);
            s.engine.clone()
        };
        if let Some(e) = e {
            e.set_music_volume(v);
        }
    }

    pub fn remove_stream(&self, user: Id) {
        let e = self.st.lock().unwrap().engine.clone();
        if let Some(e) = e {
            e.remove_stream(user);
        }
    }

    /// My user id once the session is ready.
    pub fn me(&self) -> Option<Id> {
        self.st.lock().unwrap().me
    }

    /// The server's host name (also its STUN host).
    pub fn host(&self) -> String {
        self.st.lock().unwrap().host.clone()
    }

    pub fn kick(&self, user_id: Id) {
        self.send(WsClientMsg::VoiceKick { user_id });
    }

    pub fn prefs(&self) -> AudioPrefs {
        self.st.lock().unwrap().prefs.clone()
    }

    pub fn set_volume(&self, user: Id, v: f32) {
        let v = v.clamp(0.0, 2.0);
        let e = {
            let mut s = self.st.lock().unwrap();
            s.prefs.volumes.insert(user, v);
            s.engine.clone()
        };
        if let Some(e) = e {
            e.set_volume(user, v);
        }
    }

    pub fn set_threshold(&self, t: f32) {
        let t = t.clamp(0.0, 1.0);
        let mut s = self.st.lock().unwrap();
        s.prefs.threshold = t;
        if let Some(e) = &s.engine {
            e.set_threshold(t);
        }
        if let Some(e) = &s.test_engine {
            e.set_threshold(t);
        }
    }

    pub fn set_devices(&self, input: Option<String>, output: Option<String>) {
        let mut s = self.st.lock().unwrap();
        s.prefs.input = input.clone();
        s.prefs.output = output.clone();
        if let Some(e) = &s.engine {
            e.set_devices(input.clone(), output.clone());
        }
        if let Some(e) = &s.test_engine {
            e.set_devices(input, output);
        }
    }

    /// Hear your own processed mic. Outside voice this runs a local audio-only engine.
    pub fn mic_test(&self, on: bool) {
        let mut s = self.st.lock().unwrap();
        if let Some(e) = &s.engine {
            e.mic_test(on);
            return;
        }
        if on {
            if s.test_engine.is_none() {
                let e = VoiceEngine::start((self.factory)(), self.opts(&s), None, Arc::new(|_| {}), self.events.clone());
                e.mic_test(true);
                s.test_engine = Some(e);
            }
        } else if let Some(e) = s.test_engine.take() {
            drop(s);
            e.stop();
        }
    }

    fn opts(&self, s: &State) -> EngineOpts {
        EngineOpts {
            me: s.me.unwrap_or(0),
            input: s.prefs.input.clone(),
            output: s.prefs.output.clone(),
            threshold: s.prefs.threshold,
            volumes: s.prefs.volumes.clone(),
            music_volume: s.prefs.music_volume,
        }
    }

    fn stop_engine(&self) {
        self.stop_engine_after(None);
    }

    /// Stop the engine; with a `farewell` sound, play it first and stop once it has rung out.
    fn stop_engine_after(&self, farewell: Option<Tone>) {
        let (engine, session) = {
            let mut s = self.st.lock().unwrap();
            s.engine_channel = None;
            (s.engine.take(), s.session.clone())
        };
        if let Some(se) = session {
            se.set_binary_sink(None);
        }
        if let Some(e) = engine {
            tracing::info!("voice: stopping engine");
            match farewell {
                Some(t) => {
                    e.play_tone(t);
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(650));
                        e.stop_detached();
                    });
                }
                None => e.stop_detached(),
            }
        }
    }

    fn start_engine(&self, channel_id: Id, session_id: u32, key: VoiceKey, udp_port: u16) {
        self.stop_engine();
        let test = self.st.lock().unwrap().test_engine.take();
        if let Some(t) = test {
            t.stop();
        }
        let mut s = self.st.lock().unwrap();
        let Some(session) = s.session.clone() else { return };
        let ws_session = session.clone();
        let cfg = LinkCfg { host: s.host.clone(), udp_port, session: session_id, key };
        let engine = Arc::new(VoiceEngine::start(
            (self.factory)(),
            self.opts(&s),
            Some(cfg),
            Arc::new(move |b| ws_session.send_binary(b)),
            self.events.clone(),
        ));
        if let (Some(f), Some(l)) = (s.video_sink.clone(), engine.link()) {
            l.set_video_sink(f);
        }
        let rs = session.clone();
        engine.set_on_dead(Arc::new(move || rs.reconnect()));
        engine.set_muted(s.muted);
        engine.set_deafened(s.deafened);
        engine.play_tone(Tone::Join);
        let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
        session.set_binary_sink(Some(tx));
        let weak = Arc::downgrade(&engine);
        tokio::spawn(async move {
            while let Some(b) = rx.recv().await {
                match weak.upgrade() {
                    Some(e) => e.feed_ws(&b),
                    None => return,
                }
            }
        });
        tracing::info!("voice: engine started for channel {channel_id}");
        if let Some(old) = s.engine.replace(engine) {
            tracing::warn!("voice: replaced a running engine");
            drop(s);
            old.stop();
            s = self.st.lock().unwrap();
        }
        s.engine_channel = Some(channel_id);
    }

    /// Feed every server message; voice ones drive the engine.
    /// Everything the session reports. Losing the identity (removed from the circle) or needing an
    /// update ends voice for good: the mic must not stay open for a server that ignores it.
    pub fn on_session(&self, e: &SessionEvent) {
        match e {
            SessionEvent::Server(m) => self.on_server(m),
            SessionEvent::Revoked | SessionEvent::UpdateRequired => self.detach(),
            SessionEvent::Conn(_) => {}
        }
    }

    pub fn on_server(&self, m: &WsServerMsg) {
        match m {
            WsServerMsg::Ready { me, voice, .. } => {
                let want = {
                    let mut s = self.st.lock().unwrap();
                    s.me = Some(me.id);
                    s.members = voice.clone();
                    s.want
                };
                // The old connection's voice session is gone with it: join again, same channel.
                self.stop_engine();
                if let Some(channel_id) = want {
                    self.send(WsClientMsg::VoiceJoin { channel_id });
                }
            }
            WsServerMsg::VoiceJoined { channel_id, session, key, udp_port } => {
                let (want, muted, deafened) = {
                    let s = self.st.lock().unwrap();
                    (s.want, s.muted, s.deafened)
                };
                if want != Some(*channel_id) {
                    return;
                }
                let Some(key) = VoiceKey::from_b64(key) else {
                    tracing::warn!("voice: bad key from server");
                    return;
                };
                self.start_engine(*channel_id, *session, key, *udp_port);
                if muted || deafened {
                    self.send(WsClientMsg::VoiceState { muted, deafened });
                }
            }
            WsServerMsg::VoiceEnded { reason } => {
                tracing::info!("voice ended: {reason}");
                self.st.lock().unwrap().want = None;
                self.stop_engine();
                self.notify();
            }
            WsServerMsg::VoiceMemberLeft { user_id, channel_id } => {
                let (me, want, engine, engine_ch) = {
                    let mut s = self.st.lock().unwrap();
                    s.members.retain(|m| m.user_id != *user_id);
                    (s.me, s.want, s.engine.clone(), s.engine_channel)
                };
                if Some(*user_id) == me {
                    // Only a leave for the session that is actually running counts; while a (re)join is
                    // pending it is a stale echo of the previous connection.
                    if want == Some(*channel_id) && engine_ch == Some(*channel_id) {
                        self.st.lock().unwrap().want = None;
                        self.stop_engine();
                        self.notify();
                    }
                } else if engine_ch == Some(*channel_id) {
                    if let Some(e) = engine {
                        e.remove_peer(*user_id);
                        e.play_tone(Tone::Leave);
                    }
                }
            }
            WsServerMsg::StreamStarted(info) => {
                let (engine, engine_ch) = {
                    let s = self.st.lock().unwrap();
                    (s.engine.clone(), s.engine_channel)
                };
                if let Some(e) = engine.filter(|_| engine_ch == Some(info.channel_id)) {
                    e.play_tone(Tone::Live);
                }
            }
            WsServerMsg::VoiceMemberUpdated(m) => {
                let (fresh, me, engine, engine_ch) = {
                    let mut s = self.st.lock().unwrap();
                    let fresh = !s.members.iter().any(|x| x.user_id == m.user_id && x.channel_id == m.channel_id);
                    s.members.retain(|x| x.user_id != m.user_id);
                    s.members.push(m.clone());
                    (fresh, s.me, s.engine.clone(), s.engine_channel)
                };
                if fresh && Some(m.user_id) != me && engine_ch == Some(m.channel_id) {
                    if let Some(e) = engine {
                        e.play_tone(Tone::Join);
                    }
                }
            }
            _ => {}
        }
    }
}

/// The sound for going from `(muted, deafened)` to the next: deafening wins (it mutes too).
fn toggle_tone(before: (bool, bool), after: (bool, bool)) -> Option<Tone> {
    if before.1 != after.1 {
        Some(if after.1 { Tone::Deafen } else { Tone::Undeafen })
    } else if before.0 != after.0 {
        Some(if after.0 { Tone::Mute } else { Tone::Unmute })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sound_for_a_mute_or_deafen_change() {
        // (muted, deafened) before → after
        assert_eq!(toggle_tone((false, false), (true, false)), Some(Tone::Mute));
        assert_eq!(toggle_tone((true, false), (false, false)), Some(Tone::Unmute));
        // Deafening also mutes: one sound, the deafen one.
        assert_eq!(toggle_tone((false, false), (true, true)), Some(Tone::Deafen));
        assert_eq!(toggle_tone((true, true), (false, false)), Some(Tone::Undeafen));
        // Unmuting while deafened undeafens too.
        assert_eq!(toggle_tone((true, true), (false, false)), Some(Tone::Undeafen));
        assert_eq!(toggle_tone((true, false), (true, false)), None, "nothing changed, no sound");
    }
}
