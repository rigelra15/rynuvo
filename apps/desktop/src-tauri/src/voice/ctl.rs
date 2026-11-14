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
    link::LinkCfg,
};
use crate::core::session::SessionHandle;

pub type BackendFactory = Arc<dyn Fn() -> Box<dyn AudioBackend> + Send + Sync>;

/// Audio preferences the engine starts with.
#[derive(Debug, Clone)]
pub struct AudioPrefs {
    pub input: Option<String>,
    pub output: Option<String>,
    pub threshold: f32,
    pub volumes: HashMap<Id, f32>,
}

impl Default for AudioPrefs {
    fn default() -> Self {
        AudioPrefs { input: None, output: None, threshold: 0.5, volumes: HashMap::new() }
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
    }

    pub fn channel(&self) -> Option<Id> {
        self.st.lock().unwrap().want
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
        self.stop_engine();
        self.notify();
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
        {
            let mut s = self.st.lock().unwrap();
            s.muted = muted;
            if !muted {
                s.deafened = false;
            }
        }
        self.push_state();
        self.notify();
        self.state()
    }

    /// Deafening mutes too; undeafening restores the mute state from before.
    pub fn set_deafened(&self, deafened: bool) -> MyVoice {
        {
            let mut s = self.st.lock().unwrap();
            if deafened && !s.deafened {
                s.muted_before_deafen = s.muted;
                s.muted = true;
            } else if !deafened && s.deafened {
                s.muted = s.muted_before_deafen;
            }
            s.deafened = deafened;
        }
        self.push_state();
        self.notify();
        self.state()
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
        }
    }

    fn stop_engine(&self) {
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
            e.stop();
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
                    if want == Some(*channel_id) {
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
