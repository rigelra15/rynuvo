use std::{collections::HashMap, sync::Mutex};

/// Where device tokens live, keyed by server base URL.
pub trait SecretStore: Send + Sync {
    fn get(&self, server: &str) -> Option<String>;
    fn set(&self, server: &str, token: &str);
    fn clear(&self, server: &str);
    /// This device's end-to-end encryption private key (base64), in the entry `<server>#e2ee`.
    /// `Ok(None)`: there is no entry. `Err`: the keychain could not be read (the entry may exist).
    fn get_e2ee(&self, server: &str) -> Result<Option<String>, String> {
        Ok(self.get(&e2ee_entry(server)))
    }
    fn set_e2ee(&self, server: &str, b64: &str) {
        self.set(&e2ee_entry(server), b64)
    }
}

fn e2ee_entry(server: &str) -> String {
    format!("{server}#e2ee")
}

/// OS keychain (macOS Keychain / Windows Credential Manager).
pub struct KeyringStore {
    service: String,
}

impl KeyringStore {
    /// `profile` lets two local instances keep separate identities (env `RYNUVO_PROFILE`).
    pub fn new(profile: Option<&str>) -> Self {
        let service = match profile {
            Some(p) if !p.is_empty() => format!("Rynuvo-{p}"),
            _ => "Rynuvo".to_string(),
        };
        Self { service }
    }

    fn entry(&self, server: &str) -> Option<keyring::Entry> {
        keyring::Entry::new(&self.service, server).ok()
    }
}

impl SecretStore for KeyringStore {
    fn get(&self, server: &str) -> Option<String> {
        self.entry(server)?.get_password().ok()
    }
    fn get_e2ee(&self, server: &str) -> Result<Option<String>, String> {
        let entry = keyring::Entry::new(&self.service, &e2ee_entry(server)).map_err(|e| e.to_string())?;
        match entry.get_password() {
            Ok(p) => Ok(Some(p)),
            Err(keyring::Error::NoEntry) => Ok(None),
            // Never format this one: it carries the stored bytes.
            Err(keyring::Error::BadEncoding(_)) => Err("stored value is not text".into()),
            Err(e) => Err(e.to_string()),
        }
    }
    fn set(&self, server: &str, token: &str) {
        if let Some(e) = self.entry(server) {
            if let Err(err) = e.set_password(token) {
                tracing::error!("keychain write failed: {err}");
            }
        }
    }
    fn clear(&self, server: &str) {
        if let Some(e) = self.entry(server) {
            let _ = e.delete_credential();
        }
    }
}

/// In-memory store for tests.
#[derive(Default)]
pub struct MemoryStore(Mutex<HashMap<String, String>>);

impl SecretStore for MemoryStore {
    fn get(&self, server: &str) -> Option<String> {
        self.0.lock().unwrap().get(server).cloned()
    }
    fn set(&self, server: &str, token: &str) {
        self.0.lock().unwrap().insert(server.into(), token.into());
    }
    fn clear(&self, server: &str) {
        self.0.lock().unwrap().remove(server);
    }
}
