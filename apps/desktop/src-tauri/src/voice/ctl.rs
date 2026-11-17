//! Voice control: what channel I want to be in, mute/deafen, and the engine's lifecycle,
//! driven by my commands and by the server's voice messages.
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use base64::{Engine, engine::general_purpose::STANDARD as B64};
use rynuvo_proto::{
    Id, VoiceMember, WsClientMsg, WsServerMsg,
    e2ee::{DeviceKey, PublicKey, Scope},
    voice_wire::VoiceKey,
};
use tokio::sync::mpsc;

use super::{
    backend::AudioBackend,
    engine::{EngineOpts, EventSink, Tone, VoiceEngine, VoiceEvent},
    link::{LinkCfg, VideoSink},
};
use crate::{
    core::session::{SessionEvent, SessionHandle},
    e2ee::{KeyState, Outgoing, SharedKeys},
};

/// How often missing media keys are asked for again while in voice.
const KEY_REQUEST_TICK: Duration = Duration::from_millis(250);

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

/// Device public keys in my current call, for verification codes. Public keys only, and exactly the
/// ones my media keys are sealed to and opened from.
#[derive(Debug, Clone)]
pub struct CallKeys {
    pub me: (Id, PublicKey),
    pub others: Vec<(Id, PublicKey)>,
}

impl CallKeys {
    /// `user`'s key in use, if it is still `pubkey` (the one the UI showed and the person compared).
    pub fn verify_target(&self, user: Id, pubkey: &str) -> Result<PublicKey, String> {
        let (_, pk) = self.others.iter().find(|(u, _)| *u == user).ok_or("not in your call")?;
        if pk.to_b64() != pubkey {
            return Err("their key changed, compare the code again".into());
        }
        Ok(pk.clone())
    }
}

/// What the UI shows when someone in my call has no usable device key (an older server drops it).
pub const E2EE_UNSUPPORTED: &str = "The server needs an update for encrypted calls";

/// Others in channel `ch` without a usable device key: their media can't be end-to-end encrypted, so
/// it is never sent to nor played from them.
pub fn members_without_e2ee(me: Id, ch: Id, members: &[VoiceMember]) -> Vec<Id> {
    members
        .iter()
        .filter(|m| m.channel_id == ch && m.user_id != me && PublicKey::from_b64(&m.pubkey).is_none())
        .map(|m| m.user_id)
        .collect()
}

/// The keys for verification codes: for each announced (user, key), the key my crypto uses for them.
/// Users my crypto has no key for yet are left out; None if any key in use differs from the announced
/// one (codes would then vouch for a key that isn't the one in use).
pub fn agreed_keys(announced: &[(Id, String)], in_use: impl Fn(Id) -> Option<PublicKey>) -> Option<Vec<(Id, PublicKey)>> {
    let mut out = Vec::new();
    for (u, b64) in announced {
        let Some(used) = in_use(*u) else { continue };
        if PublicKey::from_b64(b64).as_ref() != Some(&used) {
            return None;
        }
        out.push((*u, used));
    }
    Some(out)
}

/// Asked when a member's device key changed in my call; returns stream-key envelopes to send.
pub type RekeyHook = Arc<dyn Fn(Id) -> Vec<Outgoing> + Send + Sync>;

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
    /// This device's identity key (from the keychain). Without one, a key for this run is made.
    device: Option<DeviceKey>,
    /// Media keys, made for the user id beside them.
    keys: Option<(Id, SharedKeys)>,
    /// Go Live's part when a member's device key changes (rotate my stream key).
    on_rekey: Option<RekeyHook>,
    /// The "server needs an update" notice was shown for this call.
    unsupported_warned: bool,
}

impl State {
    fn device(&mut self) -> &DeviceKey {
        self.device.get_or_insert_with(DeviceKey::generate)
    }

    /// The media-key state, made on first use once my user id is known.
    fn keys(&mut self) -> Option<SharedKeys> {
        let me = self.me?;
        if self.keys.as_ref().is_none_or(|(m, _)| *m != me) {
            let dev = self.device().clone();
            self.keys = Some((me, Arc::new(Mutex::new(KeyState::new(me, dev)))));
        }
        self.keys.as_ref().map(|(_, k)| k.clone())
    }

    /// Public keys of everyone in my current voice channel, me included, read from the key state my
    /// media keys are sealed with (members it has no key for are left out). None when not in voice, or
    /// when a key in use differs from the one the server announced.
    fn call_keys(&mut self) -> Option<CallKeys> {
        let ch = self.want?;
        let me = self.me?;
        let keys = self.keys()?;
        let announced = self.others_in(ch);
        let k = keys.lock().unwrap();
        let Some(others) = agreed_keys(&announced, |u| k.peer_pubkey(u)) else {
            tracing::warn!("e2ee: a key in use differs from the announced one, no verification code");
            return None;
        };
        Some(CallKeys { me: (me, k.my_public()), others })
    }

    /// Everyone else in voice channel `ch`, with their device keys.
    fn others_in(&self, ch: Id) -> Vec<(Id, String)> {
        self.members
            .iter()
            .filter(|m| m.channel_id == ch && Some(m.user_id) != self.me)
            .map(|m| (m.user_id, m.pubkey.clone()))
            .collect()
    }
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
            // The same server again (the page reloaded): stay in the call; Ready rejoins it.
            // Signing out already cleared `want`, so a different account never inherits it.
            if s.host != host {
                s.want = None;
            }
            s.host = host;
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

    /// Tell the engine whether anyone else is in my channel.
    fn sync_alone(&self) {
        let s = self.st.lock().unwrap();
        if let (Some(e), Some(ch)) = (&s.engine, s.engine_channel) {
            e.set_alone(alone(&s.members, ch, s.me));
        }
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

    /// Tell the UI the call's membership or someone's device key changed.
    fn e2ee_changed(&self) {
        (self.events)(VoiceEvent::E2eeChanged);
    }

    /// Everyone's public device keys in my current call, or None when not in voice.
    pub fn call_keys(&self) -> Option<CallKeys> {
        self.st.lock().unwrap().call_keys()
    }

    /// Go Live's hook for a member's device key change (see `RekeyHook`).
    pub fn set_on_peer_rekeyed(&self, f: RekeyHook) {
        self.st.lock().unwrap().on_rekey = Some(f);
    }

    /// Someone in my call has no usable device key (a server too old for encrypted calls): their media
    /// stays dropped; say why, once per call.
    fn check_e2ee_support(&self) {
        let missing = {
            let mut s = self.st.lock().unwrap();
            let (Some(ch), Some(me)) = (s.want, s.me) else {
                s.unsupported_warned = false;
                return;
            };
            let missing = members_without_e2ee(me, ch, &s.members);
            if missing.is_empty() || s.unsupported_warned {
                s.unsupported_warned &= !missing.is_empty();
                return;
            }
            s.unsupported_warned = true;
            missing
        };
        tracing::warn!("e2ee: members {missing:?} have no device key (server too old?); their audio is dropped");
        (self.events)(VoiceEvent::Notice { message: E2EE_UNSUPPORTED.into() });
    }

    pub fn channel(&self) -> Option<Id> {
        self.st.lock().unwrap().want
    }

    /// This device's end-to-end encryption identity key (see `e2ee::device`). Set it before `attach`.
    pub fn set_device(&self, key: DeviceKey) {
        let mut s = self.st.lock().unwrap();
        s.device = Some(key);
        s.keys = None;
    }

    fn voice_join(&self, channel_id: Id) {
        let pubkey = self.st.lock().unwrap().device().public().to_b64();
        self.send(WsClientMsg::VoiceJoin { channel_id, pubkey });
    }

    /// My media-key state (shared with the stream controller for Go Live). None until signed in.
    pub fn e2ee_keys(&self) -> Option<SharedKeys> {
        self.st.lock().unwrap().keys()
    }

    /// Send key envelopes over the current session, in order.
    pub fn send_keys(&self, out: Vec<Outgoing>) {
        for o in out {
            let envelope = B64.encode(&o.envelope);
            self.send(WsClientMsg::E2eeKey { to: o.to, scope: o.scope, generation: o.generation, envelope });
        }
    }

    /// My engine for `ch` just started: learn everyone's device key, send them my (new) media key, and
    /// ask for theirs (after a reconnect on the same device nobody else sees a change to react to).
    fn e2ee_start(&self, ch: Id) {
        let (keys, others) = {
            let mut s = self.st.lock().unwrap();
            (s.keys(), s.others_in(ch))
        };
        let Some(keys) = keys else { return };
        let scope = Scope::Voice(ch);
        let ids: Vec<Id> = others.iter().map(|(u, _)| *u).collect();
        let out = {
            let mut k = keys.lock().unwrap();
            for (u, pk) in &others {
                k.forget_peer(&scope, *u);
                match PublicKey::from_b64(pk) {
                    Some(pk) => k.set_peer_pubkey(*u, pk),
                    None => k.remove_peer_pubkey(*u),
                }
            }
            k.share_current(&scope, &ids, Instant::now())
        };
        self.send_keys(out);
        for from in ids {
            self.send(WsClientMsg::E2eeKeyRequest { from, scope: scope.clone() });
        }
        // The codes come from the keys now in use.
        self.e2ee_changed();
    }

    /// `user` appeared in my call: start them fresh and send them my current key (no rotation).
    fn e2ee_joined(&self, ch: Id, user: Id, pubkey: &str) {
        let Some(keys) = self.st.lock().unwrap().keys() else { return };
        let scope = Scope::Voice(ch);
        let out = {
            let mut k = keys.lock().unwrap();
            k.forget_peer(&scope, user);
            match PublicKey::from_b64(pubkey) {
                Some(pk) => k.set_peer_pubkey(user, pk),
                None => k.remove_peer_pubkey(user),
            }
            k.share_current(&scope, &[user], Instant::now())
        };
        self.send_keys(out);
    }

    /// `user`'s device key changed while they stayed in my call. Handled as a leave and a join: nothing
    /// of theirs is kept, everyone else gets a new key of mine, they get my keys sealed to the new device
    /// key, and if they watch my stream, all viewers get a new stream key.
    fn e2ee_rekeyed(&self, ch: Id, user: Id, pubkey: &str) {
        tracing::warn!("e2ee: user {user}'s device key changed without a leave; treating it as a new device");
        let (keys, others) = {
            let mut s = self.st.lock().unwrap();
            (s.keys(), s.others_in(ch))
        };
        let Some(keys) = keys else { return };
        let scope = Scope::Voice(ch);
        let rest: Vec<Id> = others.into_iter().map(|(u, _)| u).filter(|u| *u != user).collect();
        let out = {
            let mut k = keys.lock().unwrap();
            k.forget_peer(&scope, user);
            match PublicKey::from_b64(pubkey) {
                Some(pk) => k.set_peer_pubkey(user, pk),
                None => k.remove_peer_pubkey(user),
            }
            let now = Instant::now();
            let mut out = k.rotate(&scope, &rest, now);
            out.extend(k.share_current(&scope, &[user], now));
            out
        };
        self.send_keys(out);
        let hook = self.st.lock().unwrap().on_rekey.clone();
        if let Some(h) = hook {
            self.send_keys(h(user));
        }
    }

    /// `user` left my call: drop their keys and give everyone left a new key of mine.
    fn e2ee_left(&self, ch: Id, user: Id) {
        let (keys, others) = {
            let mut s = self.st.lock().unwrap();
            (s.keys(), s.others_in(ch))
        };
        let Some(keys) = keys else { return };
        let scope = Scope::Voice(ch);
        let ids: Vec<Id> = others.into_iter().map(|(u, _)| u).filter(|u| *u != user).collect();
        let out = {
            let mut k = keys.lock().unwrap();
            k.forget_peer(&scope, user);
            k.rotate(&scope, &ids, Instant::now())
        };
        self.send_keys(out);
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
        self.voice_join(channel_id);
        self.notify();
        self.e2ee_changed();
    }

    pub fn leave(&self) {
        tracing::info!("voice: leave");
        let was = self.st.lock().unwrap().want.take();
        if was.is_some() {
            self.send(WsClientMsg::VoiceLeave);
        }
        self.stop_engine_after(Some(Tone::Leave));
        self.notify();
        self.e2ee_changed();
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
        let (engine, session, forget) = {
            let mut s = self.st.lock().unwrap();
            let ch = s.engine_channel.take();
            let keys = s.keys.as_ref().map(|(_, k)| k.clone());
            (s.engine.take(), s.session.clone(), ch.zip(keys))
        };
        // A new engine makes a new key; nothing of this call's keys is kept.
        if let Some((ch, k)) = forget {
            k.lock().unwrap().forget_scope(&Scope::Voice(ch));
        }
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
        let Some(keys) = s.keys() else {
            tracing::warn!("voice: joined before the session was ready");
            return;
        };
        let ws_session = session.clone();
        let cfg = LinkCfg { host: s.host.clone(), udp_port, session: session_id, key, keys: Some(keys.clone()), channel: channel_id, unencrypted_for_tests: false };
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
        engine.set_alone(alone(&s.members, channel_id, s.me));
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
        // Ask senders whose frames I can't open for their key, until this engine is gone.
        let (weak, rs) = (Arc::downgrade(&engine), session.clone());
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(KEY_REQUEST_TICK);
            loop {
                tick.tick().await;
                if weak.strong_count() == 0 {
                    return;
                }
                let due = keys.lock().unwrap().due_requests(Instant::now());
                for (scope, from) in due {
                    rs.send(WsClientMsg::E2eeKeyRequest { from, scope });
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
                    self.voice_join(channel_id);
                }
                // The page may have reloaded since it last heard: tell it where it is.
                self.notify();
                self.e2ee_changed();
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
                self.e2ee_start(*channel_id);
                self.check_e2ee_support();
                // Always: a join that moved the call from another device carries that device's
                // mute/deafen on the server, which this device's own state must replace.
                self.send(WsClientMsg::VoiceState { muted, deafened });
            }
            WsServerMsg::VoiceEnded { reason } => {
                tracing::info!("voice ended: {reason}");
                self.st.lock().unwrap().want = None;
                self.stop_engine();
                self.notify();
                self.e2ee_changed();
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
                        self.e2ee_changed();
                    }
                } else if engine_ch == Some(*channel_id) {
                    if let Some(e) = engine {
                        e.remove_peer(*user_id);
                        e.play_tone(Tone::Leave);
                    }
                    self.e2ee_left(*channel_id, *user_id);
                }
                if Some(*user_id) != me && want == Some(*channel_id) {
                    self.e2ee_changed();
                }
                self.check_e2ee_support();
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
                let (fresh, key_changed, e2ee_changed, me, engine, engine_ch) = {
                    let mut s = self.st.lock().unwrap();
                    let old = s.members.iter().find(|x| x.user_id == m.user_id && x.channel_id == m.channel_id);
                    let fresh = old.is_none();
                    let key_changed = old.is_some_and(|x| x.pubkey != m.pubkey);
                    // They were in my call and moved to another channel.
                    let moved_out = s.want.is_some_and(|w| w != m.channel_id && s.members.iter().any(|x| x.user_id == m.user_id && x.channel_id == w));
                    let in_mine = s.want == Some(m.channel_id);
                    let e2ee_changed = (in_mine && (fresh || key_changed)) || moved_out;
                    s.members.retain(|x| x.user_id != m.user_id);
                    s.members.push(m.clone());
                    (fresh, key_changed, e2ee_changed, s.me, s.engine.clone(), s.engine_channel)
                };
                if Some(m.user_id) != me && engine_ch == Some(m.channel_id) {
                    if fresh {
                        if let Some(e) = engine {
                            e.play_tone(Tone::Join);
                        }
                        self.e2ee_joined(m.channel_id, m.user_id, &m.pubkey);
                    } else if key_changed {
                        self.e2ee_rekeyed(m.channel_id, m.user_id, &m.pubkey);
                    }
                }
                // After the key state is updated: the UI re-reads codes from the keys in use.
                if e2ee_changed {
                    self.e2ee_changed();
                }
                self.check_e2ee_support();
            }
            WsServerMsg::E2eeKey { from, scope, generation, envelope } => {
                let Ok(env) = B64.decode(envelope) else { return };
                let Some(keys) = self.st.lock().unwrap().keys() else { return };
                keys.lock().unwrap().on_envelope(*from, scope, *generation, &env, Instant::now());
            }
            WsServerMsg::E2eeKeyRequest { by, scope } => {
                // Only for the call my engine is in, and only to someone in it.
                let keys = {
                    let mut s = self.st.lock().unwrap();
                    let ok = matches!(scope, Scope::Voice(ch) if s.engine_channel == Some(*ch) && s.others_in(*ch).iter().any(|(u, _)| u == by));
                    if ok { s.keys() } else { None }
                };
                if let Some(keys) = keys {
                    let out = keys.lock().unwrap().share_current(scope, &[*by], Instant::now());
                    self.send_keys(out);
                }
            }
            _ => return,
        }
        self.sync_alone();
    }
}

/// Nobody but me in `channel`.
pub fn alone(members: &[VoiceMember], channel: Id, me: Option<Id>) -> bool {
    !members.iter().any(|m| m.channel_id == channel && Some(m.user_id) != me)
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

    fn member(user_id: Id, channel_id: Id, pubkey: &str) -> VoiceMember {
        VoiceMember { user_id, channel_id, muted: false, deafened: false, joined_at: 0, call_since: 0, pubkey: pubkey.into() }
    }

    #[test]
    fn members_without_a_device_key_are_found() {
        let good = DeviceKey::generate().public().to_b64();
        let ms = vec![member(1, 10, ""), member(2, 10, &good), member(3, 10, "garbage"), member(4, 11, ""), member(5, 10, "")];
        // 5 is me; 4 is in another channel.
        assert_eq!(members_without_e2ee(5, 10, &ms), vec![1, 3]);
        assert!(members_without_e2ee(5, 11, &[member(4, 11, &good)]).is_empty());
    }

    #[test]
    fn codes_use_only_keys_both_sides_agree_on() {
        let (k1, k2) = (DeviceKey::generate().public(), DeviceKey::generate().public());
        let announced = vec![(1, k1.to_b64()), (2, k2.to_b64())];
        let (a1, a2) = (k1.clone(), k2.clone());
        let same = agreed_keys(&announced, |u| if u == 1 { Some(a1.clone()) } else { Some(a2.clone()) });
        assert_eq!(same, Some(vec![(1, k1.clone()), (2, k2.clone())]));
        // Not yet in use (engine not started): left out, not a mismatch.
        let a1 = k1.clone();
        assert_eq!(agreed_keys(&announced, |u| (u == 1).then(|| a1.clone())), Some(vec![(1, k1.clone())]));
        // The crypto uses another key than the one announced: no code at all.
        let other = DeviceKey::generate().public();
        assert_eq!(agreed_keys(&announced, |u| if u == 1 { Some(other.clone()) } else { Some(k2.clone()) }), None);
    }

    #[test]
    fn verify_refuses_a_key_that_is_not_the_one_in_use() {
        let (me, them) = (DeviceKey::generate().public(), DeviceKey::generate().public());
        let keys = CallKeys { me: (1, me), others: vec![(2, them.clone())] };
        assert_eq!(keys.verify_target(2, &them.to_b64()), Ok(them.clone()));
        let shown = DeviceKey::generate().public().to_b64();
        assert!(keys.verify_target(2, &shown).is_err(), "the UI showed another key than the one in use");
        assert!(keys.verify_target(3, &them.to_b64()).is_err(), "not in the call");
    }

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

    #[test]
    fn alone_means_nobody_else_in_my_channel() {
        let m = |user_id, channel_id| VoiceMember { user_id, channel_id, muted: false, deafened: false, joined_at: 0, call_since: 0, pubkey: String::new() };
        assert!(alone(&[m(1, 5)], 5, Some(1)), "just me");
        assert!(alone(&[m(1, 5), m(2, 6)], 5, Some(1)), "someone in another channel");
        assert!(!alone(&[m(1, 5), m(2, 5)], 5, Some(1)));
        assert!(alone(&[], 5, Some(1)));
    }
}
