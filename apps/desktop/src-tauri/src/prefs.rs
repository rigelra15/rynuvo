//! Local preferences (audio devices, voice threshold, volumes, shortcuts, autostart), one JSON file per profile.
use std::{collections::HashMap, io, path::Path};

use rynuvo_proto::Id;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Prefs {
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    pub vad_threshold: f32,
    pub volumes: HashMap<Id, f32>,
    pub shortcut_mute: String,
    pub shortcut_deafen: String,
    pub autostart: bool,
    /// Default Go Live quality: 720p30, 1080p60, 1440p60 or native.
    pub stream_preset: String,
    /// Listen-together music level, 0–1.
    pub music_volume: f32,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            input_device: None,
            output_device: None,
            vad_threshold: 0.5,
            volumes: HashMap::new(),
            shortcut_mute: "CmdOrCtrl+Shift+M".into(),
            shortcut_deafen: "CmdOrCtrl+Shift+D".into(),
            autostart: false,
            stream_preset: "1080p60".into(),
            music_volume: 0.5,
        }
    }
}

/// Missing or unreadable file → defaults.
pub fn load(path: &Path) -> Prefs {
    std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// Write to a temp file, then rename over the old one, so a crash never leaves half a file.
pub fn save(path: &Path, p: &Prefs) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(p).map_err(io::Error::other)?)?;
    std::fs::rename(&tmp, path)
}

/// The loaded preferences and where they live; every change is saved right away.
pub struct PrefsStore {
    path: std::path::PathBuf,
    cur: std::sync::Mutex<Prefs>,
}

impl PrefsStore {
    pub fn open(path: std::path::PathBuf) -> Self {
        let cur = std::sync::Mutex::new(load(&path));
        PrefsStore { path, cur }
    }

    pub fn get(&self) -> Prefs {
        self.cur.lock().unwrap().clone()
    }

    pub fn update(&self, f: impl FnOnce(&mut Prefs)) -> Prefs {
        let mut g = self.cur.lock().unwrap();
        f(&mut g);
        if let Err(e) = save(&self.path, &g) {
            tracing::warn!("saving prefs: {e}");
        }
        g.clone()
    }
}

impl Prefs {
    pub fn audio(&self) -> crate::voice::ctl::AudioPrefs {
        crate::voice::ctl::AudioPrefs {
            input: self.input_device.clone(),
            output: self.output_device.clone(),
            threshold: self.vad_threshold,
            volumes: self.volumes.clone(),
            music_volume: self.music_volume,
        }
    }
}
