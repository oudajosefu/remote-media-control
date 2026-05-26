use serde::{Deserialize, Serialize};
use std::path::PathBuf;

fn default_has_shown_pairing_qr() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PersistedConfig {
    pub token: String,
    #[serde(default)]
    pub is_active: bool,
    #[serde(default)]
    pub auto_launch: bool,
    #[serde(default = "default_has_shown_pairing_qr")]
    pub has_shown_pairing_qr: bool,
}

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .expect("cannot determine config directory")
        .join("sofamote")
        .join("config.json")
}

pub fn load_or_create() -> PersistedConfig {
    if let Some(cfg) = read_existing() {
        return cfg;
    }
    let cfg = PersistedConfig {
        token: generate_token(),
        is_active: false,
        auto_launch: false,
        has_shown_pairing_qr: false,
    };
    save(&cfg);
    cfg
}

fn read_existing() -> Option<PersistedConfig> {
    let path = config_path();
    let s = std::fs::read_to_string(&path).ok()?;
    let cfg: PersistedConfig = serde_json::from_str(&s).ok()?;
    (cfg.token.len() >= 32).then_some(cfg)
}

pub fn save(cfg: &PersistedConfig) {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    if let Ok(json) = serde_json::to_string_pretty(cfg) {
        std::fs::write(&path, json).ok();
    }
}

fn generate_token() -> String {
    let bytes: [u8; 16] = rand::random();
    hex::encode(bytes)
}
