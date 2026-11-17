//! This device's long-term identity key, kept in the OS keychain next to the device token.

use base64::{Engine, engine::general_purpose::STANDARD as B64};
use rynuvo_proto::e2ee::DeviceKey;

use crate::core::identity::SecretStore;

/// The device key for `server`: the stored one, or a new one that is stored. If the keychain does not
/// keep it, the key still works for this run (verification codes then change every launch).
pub fn load_or_create(store: &dyn SecretStore, server: &str) -> DeviceKey {
    if let Some(k) = store.get_e2ee(server).and_then(|s| decode(&s)) {
        return k;
    }
    let key = DeviceKey::generate();
    let b64 = B64.encode(key.to_bytes());
    store.set_e2ee(server, &b64);
    if store.get_e2ee(server).as_deref() != Some(b64.as_str()) {
        tracing::warn!("e2ee: keychain unavailable, key lasts this run");
    }
    key
}

fn decode(s: &str) -> Option<DeviceKey> {
    let b: [u8; 32] = B64.decode(s).ok()?.try_into().ok()?;
    Some(DeviceKey::from_bytes(b))
}
