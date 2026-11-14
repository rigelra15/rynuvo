//! Encrypted voice packet format shared by the server relay and the desktop client.
//!
//! Up (client → server): `[kind][session BE u32][seq BE u32]` + ChaCha20-Poly1305(payload), header as AAD.
//! Down (server → client): same header, plaintext = `from_user` (i64 BE) + payload, nonce direction byte 1.

use base64::{Engine, engine::general_purpose::STANDARD as B64};
use chacha20poly1305::{
    ChaCha20Poly1305, Key, KeyInit, Nonce,
    aead::{Aead, Payload},
};

use crate::Id;

pub const UP_AUDIO: u8 = 0xA1;
pub const UP_KEEPALIVE: u8 = 0xA2;
pub const DOWN_AUDIO: u8 = 0xB1;
pub const DOWN_KEEPALIVE_ACK: u8 = 0xB2;
/// Largest packet accepted or produced (fits one UDP datagram on any path).
pub const MAX_PACKET: usize = 1200;
const HEADER: usize = 9;
const TAG: usize = 16;

/// Per-session symmetric key, handed to the client over the authenticated WebSocket.
#[derive(Clone, PartialEq, Eq)]
pub struct VoiceKey(pub [u8; 32]);

impl std::fmt::Debug for VoiceKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("VoiceKey(..)")
    }
}

impl VoiceKey {
    pub fn random() -> Self {
        let mut k = [0u8; 32];
        getrandom::fill(&mut k).expect("system randomness");
        VoiceKey(k)
    }
    pub fn to_b64(&self) -> String {
        B64.encode(self.0)
    }
    pub fn from_b64(s: &str) -> Option<Self> {
        B64.decode(s).ok()?.try_into().ok().map(VoiceKey)
    }
    fn cipher(&self) -> ChaCha20Poly1305 {
        ChaCha20Poly1305::new(Key::from_slice(&self.0))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum WireError {
    Malformed,
    Auth,
}

#[derive(Debug)]
pub struct Up {
    pub kind: u8,
    pub session: u32,
    pub seq: u32,
    pub payload: Vec<u8>,
}

#[derive(Debug)]
pub struct Down {
    pub kind: u8,
    pub seq: u32,
    pub from_user: Id,
    pub payload: Vec<u8>,
}

fn header(kind: u8, session: u32, seq: u32) -> [u8; HEADER] {
    let mut h = [0u8; HEADER];
    h[0] = kind;
    h[1..5].copy_from_slice(&session.to_be_bytes());
    h[5..9].copy_from_slice(&seq.to_be_bytes());
    h
}

fn nonce(dir: u8, session: u32, seq: u32) -> [u8; 12] {
    let mut n = [0u8; 12];
    n[0] = dir;
    n[4..8].copy_from_slice(&session.to_be_bytes());
    n[8..12].copy_from_slice(&seq.to_be_bytes());
    n
}

fn seal(key: &VoiceKey, dir: u8, kind: u8, session: u32, seq: u32, plain: &[u8]) -> Vec<u8> {
    let h = header(kind, session, seq);
    let ct = key
        .cipher()
        .encrypt(Nonce::from_slice(&nonce(dir, session, seq)), Payload { msg: plain, aad: &h })
        .expect("chacha20poly1305 encrypt");
    let mut out = Vec::with_capacity(HEADER + ct.len());
    out.extend_from_slice(&h);
    out.extend_from_slice(&ct);
    out
}

fn open(key: &VoiceKey, dir: u8, pkt: &[u8]) -> Result<(u8, u32, u32, Vec<u8>), WireError> {
    if pkt.len() < HEADER + TAG || pkt.len() > MAX_PACKET {
        return Err(WireError::Malformed);
    }
    let kind = pkt[0];
    let session = u32::from_be_bytes(pkt[1..5].try_into().unwrap());
    let seq = u32::from_be_bytes(pkt[5..9].try_into().unwrap());
    let plain = key
        .cipher()
        .decrypt(Nonce::from_slice(&nonce(dir, session, seq)), Payload { msg: &pkt[HEADER..], aad: &pkt[..HEADER] })
        .map_err(|_| WireError::Auth)?;
    Ok((kind, session, seq, plain))
}

pub fn seal_up(key: &VoiceKey, kind: u8, session: u32, seq: u32, payload: &[u8]) -> Vec<u8> {
    seal(key, 0, kind, session, seq, payload)
}

pub fn open_up(key: &VoiceKey, pkt: &[u8]) -> Result<Up, WireError> {
    let (kind, session, seq, payload) = open(key, 0, pkt)?;
    if kind != UP_AUDIO && kind != UP_KEEPALIVE {
        return Err(WireError::Malformed);
    }
    Ok(Up { kind, session, seq, payload })
}

pub fn seal_down(key: &VoiceKey, kind: u8, session: u32, seq: u32, from_user: Id, payload: &[u8]) -> Vec<u8> {
    let mut plain = Vec::with_capacity(8 + payload.len());
    plain.extend_from_slice(&from_user.to_be_bytes());
    plain.extend_from_slice(payload);
    seal(key, 1, kind, session, seq, &plain)
}

pub fn open_down(key: &VoiceKey, pkt: &[u8]) -> Result<Down, WireError> {
    let (kind, _session, seq, plain) = open(key, 1, pkt)?;
    if (kind != DOWN_AUDIO && kind != DOWN_KEEPALIVE_ACK) || plain.len() < 8 {
        return Err(WireError::Malformed);
    }
    let from_user = Id::from_be_bytes(plain[..8].try_into().unwrap());
    Ok(Down { kind, seq, from_user, payload: plain[8..].to_vec() })
}

/// Session id of an up packet without decrypting it. None for STUN, down packets, or bad sizes.
pub fn peek_up(pkt: &[u8]) -> Option<(u8, u32)> {
    if pkt.len() < HEADER + TAG || pkt.len() > MAX_PACKET || (pkt[0] != UP_AUDIO && pkt[0] != UP_KEEPALIVE) {
        return None;
    }
    Some((pkt[0], u32::from_be_bytes(pkt[1..5].try_into().unwrap())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> VoiceKey {
        VoiceKey([7u8; 32])
    }

    #[test]
    fn up_roundtrip() {
        let p = seal_up(&key(), UP_AUDIO, 42, 9, b"opus");
        assert_eq!(peek_up(&p), Some((UP_AUDIO, 42)));
        let u = open_up(&key(), &p).unwrap();
        assert_eq!((u.kind, u.session, u.seq, u.payload.as_slice()), (UP_AUDIO, 42, 9, &b"opus"[..]));
    }

    #[test]
    fn down_roundtrip() {
        let p = seal_down(&key(), DOWN_AUDIO, 42, 3, -5, b"frame");
        let d = open_down(&key(), &p).unwrap();
        assert_eq!((d.kind, d.seq, d.from_user, d.payload.as_slice()), (DOWN_AUDIO, 3, -5, &b"frame"[..]));
    }

    #[test]
    fn tampered_rejected() {
        let mut p = seal_up(&key(), UP_AUDIO, 1, 1, b"hello");
        let last = p.len() - 1;
        p[last] ^= 1;
        assert!(open_up(&key(), &p).is_err());
        let mut h = seal_up(&key(), UP_AUDIO, 1, 1, b"hello");
        h[5] ^= 1; // header is authenticated
        assert!(open_up(&key(), &h).is_err());
    }

    #[test]
    fn wrong_key_rejected() {
        let p = seal_up(&key(), UP_AUDIO, 1, 1, b"x");
        assert!(open_up(&VoiceKey([8u8; 32]), &p).is_err());
    }

    #[test]
    fn up_does_not_open_as_down() {
        let p = seal_up(&key(), UP_AUDIO, 1, 1, b"12345678x");
        assert!(open_down(&key(), &p).is_err());
    }

    #[test]
    fn peek_rejects_stun_and_oversize() {
        let mut stun = vec![0x00, 0x01, 0, 0];
        stun.extend_from_slice(&[0u8; 40]);
        assert_eq!(peek_up(&stun), None);
        let big = vec![UP_AUDIO; MAX_PACKET + 1];
        assert_eq!(peek_up(&big), None);
        assert_eq!(peek_up(&[UP_AUDIO, 0, 0, 0, 1]), None, "too short");
        let down = seal_down(&key(), DOWN_AUDIO, 1, 1, 1, b"x");
        assert_eq!(peek_up(&down), None, "down kinds are not up packets");
    }

    #[test]
    fn key_b64_roundtrip() {
        let k = VoiceKey::random();
        assert_eq!(VoiceKey::from_b64(&k.to_b64()), Some(k.clone()));
        assert_eq!(VoiceKey::from_b64("short"), None);
    }
}
