//! End-to-end encryption primitives shared by the server and the desktop client: key envelopes,
//! media frame encryption and verification codes.

// chacha20poly1305 0.10 only offers `from_slice` for keys and nonces (same as voice_wire).
#![allow(deprecated)]

use base64::{Engine, engine::general_purpose::STANDARD as B64};
use chacha20poly1305::{
    ChaCha20Poly1305, Key, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey as XPublic, StaticSecret};

use crate::Id;

const INFO_PREFIX: &[u8] = b"rynuvo-e2ee-v1";
const TAG: usize = 16;
const HEADER: usize = 8;

/// Bytes a sealed frame adds to its plaintext: generation + counter header and the AEAD tag.
pub const FRAME_OVERHEAD: usize = HEADER + TAG;
pub const KIND_VOICE: u8 = 1;
pub const KIND_VIDEO: u8 = 2;
pub const KIND_STREAM_AUDIO: u8 = 3;

/// Long-term device identity key. Never printed or logged.
#[derive(Clone)]
pub struct DeviceKey(StaticSecret);

impl std::fmt::Debug for DeviceKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DeviceKey(..)")
    }
}

impl DeviceKey {
    pub fn generate() -> Self {
        Self::from_bytes(random32())
    }
    pub fn from_bytes(b: [u8; 32]) -> Self {
        Self(StaticSecret::from(b))
    }
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }
    pub fn public(&self) -> PublicKey {
        PublicKey(XPublic::from(&self.0).to_bytes())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicKey(pub [u8; 32]);

impl PublicKey {
    pub fn to_b64(&self) -> String {
        B64.encode(self.0)
    }
    pub fn from_b64(s: &str) -> Option<Self> {
        B64.decode(s).ok()?.try_into().ok().map(Self)
    }
}

/// Symmetric key protecting one sender's media in one scope.
#[derive(Clone, PartialEq, Eq)]
pub struct MediaKey(pub [u8; 32]);

impl std::fmt::Debug for MediaKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MediaKey(..)")
    }
}

impl MediaKey {
    pub fn random() -> Self {
        Self(random32())
    }
}

/// Where a media key applies: a voice channel, or a stream (identified by its streamer).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "id")]
pub enum Scope {
    Voice(Id),
    Stream(Id),
}

fn random32() -> [u8; 32] {
    let mut k = [0u8; 32];
    getrandom::fill(&mut k).expect("system randomness");
    k
}

fn envelope_key(ikm: &[u8], scope: &Scope, from: Id, to: Id, generation: u32) -> Option<[u8; 32]> {
    let (kind, id) = match scope {
        Scope::Voice(id) => (1u8, *id),
        Scope::Stream(id) => (2u8, *id),
    };
    let mut info = Vec::with_capacity(INFO_PREFIX.len() + 1 + 8 * 3 + 4);
    info.extend_from_slice(INFO_PREFIX);
    info.push(kind);
    info.extend_from_slice(&id.to_be_bytes());
    info.extend_from_slice(&from.to_be_bytes());
    info.extend_from_slice(&to.to_be_bytes());
    info.extend_from_slice(&generation.to_be_bytes());
    let mut out = [0u8; 32];
    Hkdf::<Sha256>::new(None, ikm).expand(&info, &mut out).ok()?;
    Some(out)
}

fn dh_pair(a: &StaticSecret, a_peer: &XPublic, b: &StaticSecret, b_peer: &XPublic) -> Option<Vec<u8>> {
    let x = a.diffie_hellman(a_peer);
    let y = b.diffie_hellman(b_peer);
    if !x.was_contributory() || !y.was_contributory() {
        return None;
    }
    let mut ikm = x.as_bytes().to_vec();
    ikm.extend_from_slice(y.as_bytes());
    Some(ikm)
}

/// Seal `key` for `to`: `eph_pub(32) || ChaCha20-Poly1305(key)` (48 bytes). The envelope key is single-use
/// (fresh ephemeral), so the nonce is zero.
pub fn seal_envelope(
    me: &DeviceKey,
    from: Id,
    to: Id,
    to_pub: &PublicKey,
    scope: &Scope,
    generation: u32,
    key: &MediaKey,
) -> Vec<u8> {
    let eph = StaticSecret::from(random32());
    let eph_pub = XPublic::from(&eph);
    let to_x = XPublic::from(to_pub.0);
    let ikm = dh_pair(&eph, &to_x, &me.0, &to_x).expect("recipient key is a valid curve point");
    let k = envelope_key(&ikm, scope, from, to, generation).expect("hkdf length");
    let ct = ChaCha20Poly1305::new(Key::from_slice(&k))
        .encrypt(Nonce::from_slice(&[0u8; 12]), key.0.as_slice())
        .expect("encrypt");
    let mut out = eph_pub.as_bytes().to_vec();
    out.extend_from_slice(&ct);
    out
}

/// Open an envelope addressed to `me` from `from`. `None` on any mismatch or malformed input.
pub fn open_envelope(
    me: &DeviceKey,
    from: Id,
    from_pub: &PublicKey,
    to: Id,
    scope: &Scope,
    generation: u32,
    env: &[u8],
) -> Option<MediaKey> {
    if env.len() != 32 + 32 + TAG {
        return None;
    }
    let eph_pub = XPublic::from(<[u8; 32]>::try_from(&env[..32]).ok()?);
    let from_x = XPublic::from(from_pub.0);
    let ikm = dh_pair(&me.0, &eph_pub, &me.0, &from_x)?;
    let k = envelope_key(&ikm, scope, from, to, generation)?;
    let pt = ChaCha20Poly1305::new(Key::from_slice(&k)).decrypt(Nonce::from_slice(&[0u8; 12]), &env[32..]).ok()?;
    Some(MediaKey(pt.try_into().ok()?))
}

fn frame_nonce(kind: u8, counter: u32) -> [u8; 12] {
    let mut n = [0u8; 12];
    n[0] = kind;
    n[8..].copy_from_slice(&counter.to_be_bytes());
    n
}

fn frame_aad(header: &[u8], sender: Id) -> [u8; 16] {
    let mut a = [0u8; 16];
    a[..8].copy_from_slice(header);
    a[8..].copy_from_slice(&sender.to_be_bytes());
    a
}

/// `[generation BE][counter BE] + ciphertext + tag`. Counter must never repeat for one key and kind.
pub fn seal_frame(key: &MediaKey, generation: u32, counter: u32, kind: u8, sender: Id, plain: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(plain.len() + FRAME_OVERHEAD);
    out.extend_from_slice(&generation.to_be_bytes());
    out.extend_from_slice(&counter.to_be_bytes());
    let aad = frame_aad(&out, sender);
    let ct = ChaCha20Poly1305::new(Key::from_slice(&key.0))
        .encrypt(Nonce::from_slice(&frame_nonce(kind, counter)), Payload { msg: plain, aad: &aad })
        .expect("encrypt");
    out.extend_from_slice(&ct);
    out
}

/// Generation number from a sealed frame's header, to pick the right key before opening.
pub fn frame_generation(ct: &[u8]) -> Option<u32> {
    Some(u32::from_be_bytes(ct.get(..4)?.try_into().ok()?))
}

pub fn open_frame(key: &MediaKey, kind: u8, sender: Id, ct: &[u8]) -> Option<Vec<u8>> {
    if ct.len() < FRAME_OVERHEAD {
        return None;
    }
    let counter = u32::from_be_bytes(ct[4..8].try_into().ok()?);
    let aad = frame_aad(&ct[..HEADER], sender);
    ChaCha20Poly1305::new(Key::from_slice(&key.0))
        .decrypt(Nonce::from_slice(&frame_nonce(kind, counter)), Payload { msg: &ct[HEADER..], aad: &aad })
        .ok()
}

fn code_of(mut members: Vec<(Id, [u8; 32])>) -> String {
    members.sort();
    let mut h = Sha256::new();
    for (id, pk) in &members {
        h.update(id.to_be_bytes());
        h.update(pk);
    }
    let d = h.finalize();
    (0..6)
        .map(|i| format!("{:05}", u32::from_be_bytes(d[i * 4..i * 4 + 4].try_into().unwrap()) % 100_000))
        .collect::<Vec<_>>()
        .join(" ")
}

/// 30-digit code over everyone in the call; identical on every member's screen, independent of order.
pub fn call_code(members: &[(Id, PublicKey)]) -> String {
    code_of(members.iter().map(|(i, p)| (*i, p.0)).collect())
}

/// 30-digit code for two people to compare.
pub fn person_code(a: (Id, &PublicKey), b: (Id, &PublicKey)) -> String {
    code_of(vec![(a.0, a.1.0), (b.0, b.1.0)])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair() -> (DeviceKey, PublicKey) {
        let k = DeviceKey::generate();
        let p = k.public();
        (k, p)
    }

    #[test]
    fn envelope_round_trip() {
        let (a, a_pub) = pair();
        let (b, b_pub) = pair();
        let key = MediaKey::random();
        let env = seal_envelope(&a, 1, 2, &b_pub, &Scope::Voice(9), 7, &key);
        assert_eq!(env.len(), 32 + 32 + 16);
        let got = open_envelope(&b, 1, &a_pub, 2, &Scope::Voice(9), 7, &env).unwrap();
        assert_eq!(got, key);
    }

    #[test]
    fn tampered_or_misaddressed_envelope_fails() {
        let (a, a_pub) = pair();
        let (b, b_pub) = pair();
        let (c, _) = pair();
        let key = MediaKey::random();
        let s = Scope::Voice(9);
        let env = seal_envelope(&a, 1, 2, &b_pub, &s, 7, &key);
        assert!(open_envelope(&b, 1, &a_pub, 2, &s, 7, &env).is_some());
        for i in [0, 31, 32, env.len() - 1] {
            let mut bad = env.clone();
            bad[i] ^= 1;
            assert!(open_envelope(&b, 1, &a_pub, 2, &s, 7, &bad).is_none(), "byte {i}");
        }
        assert!(open_envelope(&c, 1, &a_pub, 2, &s, 7, &env).is_none());
        assert!(open_envelope(&b, 1, &a_pub, 2, &Scope::Stream(9), 7, &env).is_none());
        assert!(open_envelope(&b, 1, &a_pub, 2, &Scope::Voice(8), 7, &env).is_none());
        assert!(open_envelope(&b, 1, &a_pub, 2, &s, 8, &env).is_none());
        assert!(open_envelope(&b, 1, &a_pub, 3, &s, 7, &env).is_none());
        assert!(open_envelope(&b, 4, &a_pub, 2, &s, 7, &env).is_none());
        assert!(open_envelope(&b, 1, &c.public(), 2, &s, 7, &env).is_none());
        assert!(open_envelope(&b, 1, &a_pub, 2, &s, 7, &env[..10]).is_none());
        assert!(open_envelope(&b, 1, &a_pub, 2, &s, 7, &[]).is_none());
    }

    #[test]
    fn frame_round_trip_and_overhead() {
        let key = MediaKey::random();
        let plain = b"opus frame bytes";
        let ct = seal_frame(&key, 5, 6, KIND_VOICE, 42, plain);
        assert_eq!(ct.len(), plain.len() + FRAME_OVERHEAD);
        assert_eq!(frame_generation(&ct), Some(5));
        assert_eq!(&ct[..8], &[0, 0, 0, 5, 0, 0, 0, 6]);
        assert_eq!(open_frame(&key, KIND_VOICE, 42, &ct).unwrap(), plain);
        assert!(open_frame(&key, KIND_VIDEO, 42, &ct).is_none());
        assert!(open_frame(&key, KIND_VOICE, 43, &ct).is_none());
        assert!(open_frame(&MediaKey::random(), KIND_VOICE, 42, &ct).is_none());
        for i in [0, 7, 8, ct.len() - 1] {
            let mut bad = ct.clone();
            bad[i] ^= 1;
            assert!(open_frame(&key, KIND_VOICE, 42, &bad).is_none(), "byte {i}");
        }
        assert!(open_frame(&key, KIND_VOICE, 42, &ct[..10]).is_none());
        assert_eq!(frame_generation(&ct[..3]), None);
    }

    #[test]
    fn codes_are_order_independent_and_sensitive() {
        let a = (1, DeviceKey::generate().public());
        let b = (2, DeviceKey::generate().public());
        let c = (3, DeviceKey::generate().public());
        let code = call_code(&[a.clone(), b.clone(), c.clone()]);
        assert_eq!(code, call_code(&[c.clone(), a.clone(), b.clone()]));
        let c2 = (3, DeviceKey::generate().public());
        assert_ne!(code, call_code(&[a.clone(), b.clone(), c2]));
        let ok = |s: &str| {
            s.len() == 35
                && s.split(' ').count() == 6
                && s.split(' ').all(|g| g.len() == 5 && g.bytes().all(|x| x.is_ascii_digit()))
        };
        assert!(ok(&code), "{code}");
        let p = person_code((a.0, &a.1), (b.0, &b.1));
        assert_eq!(p, person_code((b.0, &b.1), (a.0, &a.1)));
        assert!(ok(&p));
        assert_ne!(p, person_code((a.0, &a.1), (c.0, &c.1)));
    }
}
