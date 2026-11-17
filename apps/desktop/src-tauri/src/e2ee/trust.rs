//! Which device keys I have seen and verified for each user, one JSON file per profile
//! (`trust-<profile>.json`). Only public keys are stored.
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use rynuvo_proto::{Id, e2ee::PublicKey};
use serde::{Deserialize, Serialize};

/// How much to trust a user's current device key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum KeyStatus {
    /// This exact key was compared and marked verified.
    Verified,
    /// Seen before (or the first key ever seen for this user).
    Known,
    /// A key not seen before, for someone with other seen keys but none verified.
    NewDevice,
    /// The user has verified keys and this is not one of them.
    Changed,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct UserKeys {
    #[serde(default)]
    seen: Vec<String>,
    #[serde(default)]
    verified: Vec<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct File {
    #[serde(default)]
    users: HashMap<Id, UserKeys>,
}

pub struct Trust {
    path: Option<PathBuf>,
    data: File,
    /// (user, key) first recorded this voice session for a user who had other seen keys. Not saved.
    fresh: HashSet<(Id, String)>,
}

impl Trust {
    /// Load `path`. Missing file: empty. Corrupt file: empty, with a warning.
    pub fn open(path: PathBuf) -> Self {
        let data = match std::fs::read(&path) {
            Ok(b) => serde_json::from_slice(&b).unwrap_or_else(|e| {
                let mut bad = path.clone().into_os_string();
                bad.push(".bad");
                let kept = std::fs::rename(&path, &bad).is_ok();
                tracing::warn!("trust file {} is unreadable (kept as .bad: {kept}), starting empty: {e}", path.display());
                File::default()
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => File::default(),
            Err(e) => {
                tracing::warn!("reading trust file {}: {e}", path.display());
                File::default()
            }
        };
        Trust { path: Some(path), data, fresh: HashSet::new() }
    }

    /// Not saved anywhere (for tests).
    pub fn in_memory() -> Self {
        Trust { path: None, data: File::default(), fresh: HashSet::new() }
    }

    /// The status of `pk` for `user`, recording it as seen.
    pub fn status(&mut self, user: Id, pk: &PublicKey) -> KeyStatus {
        self.statuses(&[(user, pk.clone())])[0]
    }

    /// Statuses for several keys with a single save.
    pub fn statuses(&mut self, keys: &[(Id, PublicKey)]) -> Vec<KeyStatus> {
        let mut changed = false;
        let out = keys
            .iter()
            .map(|(user, pk)| {
                let b64 = pk.to_b64();
                let u = self.data.users.entry(*user).or_default();
                let status = if u.verified.contains(&b64) {
                    KeyStatus::Verified
                } else if !u.verified.is_empty() {
                    KeyStatus::Changed
                } else if self.fresh.contains(&(*user, b64.clone())) || (!u.seen.is_empty() && !u.seen.contains(&b64)) {
                    KeyStatus::NewDevice
                } else {
                    KeyStatus::Known
                };
                if !u.seen.contains(&b64) {
                    if status == KeyStatus::NewDevice {
                        self.fresh.insert((*user, b64.clone()));
                    }
                    u.seen.push(b64);
                    changed = true;
                }
                status
            })
            .collect();
        if changed {
            self.save();
        }
        out
    }

    /// I left voice: keys first seen during the call are ordinary known keys from now on.
    pub fn end_session(&mut self) {
        self.fresh.clear();
    }

    /// Mark `pk` verified for `user` (and seen).
    pub fn verify(&mut self, user: Id, pk: &PublicKey) {
        let b64 = pk.to_b64();
        let u = self.data.users.entry(user).or_default();
        if !u.seen.contains(&b64) {
            u.seen.push(b64.clone());
        }
        if !u.verified.contains(&b64) {
            u.verified.push(b64);
        }
        self.save();
    }

    /// Write to a temp file, then rename over the old one. A failure is logged; memory stays current.
    fn save(&self) {
        let Some(path) = self.path.as_deref() else { return };
        if let Err(e) = write_atomic(path, &self.data) {
            tracing::warn!("saving trust file {}: {e}", path.display());
        }
    }
}

fn write_atomic(path: &Path, data: &File) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(data).map_err(std::io::Error::other)?)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rynuvo_proto::e2ee::DeviceKey;

    fn pk() -> PublicKey {
        DeviceKey::generate().public()
    }

    #[test]
    fn first_key_is_known() {
        let mut t = Trust::in_memory();
        assert_eq!(t.status(1, &pk()), KeyStatus::Known);
    }

    #[test]
    fn second_key_unverified_is_new_device() {
        let mut t = Trust::in_memory();
        let (a, b) = (pk(), pk());
        t.status(1, &a);
        assert_eq!(t.status(1, &b), KeyStatus::NewDevice);
        assert_eq!(t.status(1, &b), KeyStatus::NewDevice);
        assert_eq!(t.status(1, &a), KeyStatus::Known);
        t.end_session();
        assert_eq!(t.status(1, &b), KeyStatus::Known);
    }

    #[test]
    fn two_new_keys_in_one_read_are_both_new_device() {
        let mut t = Trust::in_memory();
        let (a, b, c) = (pk(), pk(), pk());
        t.status(1, &a);
        let r = t.statuses(&[(1, b.clone()), (1, c.clone())]);
        assert_eq!(r, vec![KeyStatus::NewDevice, KeyStatus::NewDevice]);
        assert_eq!(t.statuses(&[(1, b), (1, c)]), r);
    }

    #[test]
    fn corrupt_file_is_kept_as_bad_and_starts_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("trust-x.json");
        std::fs::write(&path, b"{ nope").unwrap();
        let mut t = Trust::open(path.clone());
        assert_eq!(t.status(1, &pk()), KeyStatus::Known);
        assert_eq!(std::fs::read(dir.path().join("trust-x.json.bad")).unwrap(), b"{ nope");
        // An older .bad is overwritten.
        std::fs::write(&path, b"again").unwrap();
        Trust::open(path);
        assert_eq!(std::fs::read(dir.path().join("trust-x.json.bad")).unwrap(), b"again");
    }

    #[test]
    fn unverified_after_verify_is_changed() {
        let mut t = Trust::in_memory();
        let (a, b) = (pk(), pk());
        t.status(1, &a);
        t.verify(1, &a);
        assert_eq!(t.status(1, &a), KeyStatus::Verified);
        assert_eq!(t.status(1, &b), KeyStatus::Changed);
        assert_eq!(t.status(2, &b), KeyStatus::Known);
    }

    #[test]
    fn two_verified_devices_both_verified() {
        let mut t = Trust::in_memory();
        let (a, b) = (pk(), pk());
        t.verify(1, &a);
        t.verify(1, &b);
        assert_eq!(t.status(1, &a), KeyStatus::Verified);
        assert_eq!(t.status(1, &b), KeyStatus::Verified);
    }

    #[test]
    fn survives_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("trust-x.json");
        let (a, b, c) = (pk(), pk(), pk());
        {
            let mut t = Trust::open(path.clone());
            t.status(1, &a);
            t.verify(1, &a);
            t.status(2, &b);
            t.status(2, &c);
        }
        let mut t = Trust::open(path);
        assert_eq!(t.status(1, &a), KeyStatus::Verified);
        assert_eq!(t.status(1, &b), KeyStatus::Changed);
        assert_eq!(t.status(2, &b), KeyStatus::Known);
        assert_eq!(t.status(2, &c), KeyStatus::Known);
        assert_eq!(t.status(2, &pk()), KeyStatus::NewDevice);
    }
}
