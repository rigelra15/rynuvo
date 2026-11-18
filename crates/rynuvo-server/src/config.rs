use serde::Deserialize;
use std::{
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub bind: SocketAddr,
    pub public_url: String,
    pub public_ip: IpAddr,
    pub data_dir: PathBuf,
    #[serde(default = "default_stun_ports")]
    pub stun_ports: [u16; 2],
    #[serde(default = "default_warn")]
    pub disk_warn_pct: u8,
    #[serde(default = "default_block")]
    pub disk_block_pct: u8,
    #[serde(default = "default_max_upload")]
    pub max_upload_bytes: u64,
    /// Monthly data budget for the relay in GB (in + out); 0 = unlimited.
    #[serde(default = "default_quota_gb")]
    pub quota_gb: u64,
    /// Daily SQLite snapshots kept in `<data_dir>/backups`; 0 turns backups off.
    #[serde(default = "default_backups")]
    pub backups_keep: usize,
    /// Firebase service account key (JSON) for push notifications to phones; unset = no pushes.
    #[serde(default)]
    pub fcm_service_account: Option<PathBuf>,
}

fn default_quota_gb() -> u64 {
    400
}
fn default_backups() -> usize {
    7
}

fn default_stun_ports() -> [u16; 2] {
    [3478, 3479]
}
fn default_warn() -> u8 {
    85
}
fn default_block() -> u8 {
    92
}
fn default_max_upload() -> u64 {
    5_242_880
}

impl Config {
    pub fn load(path: &Path) -> anyhow::Result<Config> {
        let s = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&s)?)
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("rynuvo.db")
    }

    pub fn media_dir(&self) -> PathBuf {
        self.data_dir.join("media")
    }

    pub fn backups_dir(&self) -> PathBuf {
        self.data_dir.join("backups")
    }

    pub fn downloads_dir(&self) -> PathBuf {
        self.data_dir.join("downloads")
    }
}
