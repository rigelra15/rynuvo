//! This device's long-term identity key, kept in the OS keychain next to the device token.

use base64::{Engine, engine::general_purpose::STANDARD as B64};
use rynuvo_proto::e2ee::DeviceKey;

use crate::core::identity::SecretStore;

/// The device key for `server`: the stored one, or a new one that is stored when there is none.
/// If the keychain can't be read, or holds something that isn't a key, a key for this run is used
/// and nothing is written (the stored identity, if any, is kept). Verification codes then change.
pub fn load_or_create(store: &dyn SecretStore, server: &str) -> DeviceKey {
    match store.get_e2ee(server) {
        Ok(Some(s)) => match decode(&s) {
            Some(k) => return k,
            None => {
                tracing::warn!("e2ee: stored device key is unreadable, using a key for this run (kept as is)");
                return DeviceKey::generate();
            }
        },
        Ok(None) => {}
        Err(e) => {
            tracing::warn!("e2ee: keychain read failed ({e}), using a key for this run");
            return DeviceKey::generate();
        }
    }
    let key = DeviceKey::generate();
    let b64 = B64.encode(key.to_bytes());
    store.set_e2ee(server, &b64);
    if store.get_e2ee(server).ok().flatten().as_deref() != Some(b64.as_str()) {
        tracing::warn!("e2ee: keychain unavailable, key lasts this run");
    }
    key
}

fn decode(s: &str) -> Option<DeviceKey> {
    let b: [u8; 32] = B64.decode(s).ok()?.try_into().ok()?;
    Some(DeviceKey::from_bytes(b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A keychain whose e2ee entry reads as `read`, recording every write.
    struct Fake {
        read: Mutex<Result<Option<String>, String>>,
        writes: Mutex<Vec<String>>,
    }

    impl Fake {
        fn new(read: Result<Option<String>, String>) -> Self {
            Fake { read: Mutex::new(read), writes: Mutex::new(Vec::new()) }
        }
    }

    impl SecretStore for Fake {
        fn get(&self, _: &str) -> Option<String> {
            None
        }
        fn set(&self, _: &str, _: &str) {}
        fn clear(&self, _: &str) {}
        fn get_e2ee(&self, _: &str) -> Result<Option<String>, String> {
            self.read.lock().unwrap().clone()
        }
        fn set_e2ee(&self, _: &str, b64: &str) {
            self.writes.lock().unwrap().push(b64.into());
            *self.read.lock().unwrap() = Ok(Some(b64.into()));
        }
    }

    const S: &str = "http://rynuvo.test";

    #[test]
    fn absent_key_is_generated_and_stored() {
        let f = Fake::new(Ok(None));
        let k = load_or_create(&f, S);
        assert_eq!(*f.writes.lock().unwrap(), vec![B64.encode(k.to_bytes())]);
        assert_eq!(load_or_create(&f, S).public(), k.public(), "the stored key is used next time");
    }

    #[test]
    fn read_error_uses_a_memory_key_and_never_writes() {
        let f = Fake::new(Err("keychain locked".into()));
        let k1 = load_or_create(&f, S);
        let k2 = load_or_create(&f, S);
        assert!(f.writes.lock().unwrap().is_empty(), "a failed read must not overwrite the stored identity");
        assert_ne!(k1.public(), k2.public());
    }

    #[test]
    fn undecodable_value_uses_a_memory_key_and_never_writes() {
        for bad in ["not base64!", "AAAA"] {
            let f = Fake::new(Ok(Some(bad.into())));
            load_or_create(&f, S);
            assert!(f.writes.lock().unwrap().is_empty(), "{bad}: kept as is");
        }
    }
}
