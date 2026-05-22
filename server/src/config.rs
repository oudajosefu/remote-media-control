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
    let path = config_path();
    if path.exists() {
        if let Ok(s) = std::fs::read_to_string(&path) {
            if let Ok(cfg) = serde_json::from_str::<PersistedConfig>(&s) {
                if cfg.token.len() >= 32 {
                    return cfg;
                }
            }
        }
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
