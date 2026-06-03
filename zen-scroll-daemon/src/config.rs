use std::fs;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

use zen_scroll_core::physics::ScrollConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileConfig {
    pub name: String,
    pub friction: f64,
    pub bounce_tension: f64,
    pub scroll_accel: f64,
    pub max_velocity: f64,
    pub min_velocity: f64,
    pub deceleration_rate: f64,
    pub max_bounce_distance: f64,
    pub smartwheel_friction_max: f64,
}

impl From<&ProfileConfig> for ScrollConfig {
    fn from(p: &ProfileConfig) -> Self {
        ScrollConfig {
            friction: p.friction,
            bounce_tension: p.bounce_tension,
            scroll_accel: p.scroll_accel,
            max_velocity: p.max_velocity,
            min_velocity: p.min_velocity,
            deceleration_rate: p.deceleration_rate,
            max_bounce_distance: p.max_bounce_distance,
            smartwheel_friction_max: p.smartwheel_friction_max,
        }
    }
}

impl From<&ScrollConfig> for ProfileConfig {
    fn from(c: &ScrollConfig) -> Self {
        ProfileConfig {
            name: String::new(),
            friction: c.friction,
            bounce_tension: c.bounce_tension,
            scroll_accel: c.scroll_accel,
            max_velocity: c.max_velocity,
            min_velocity: c.min_velocity,
            deceleration_rate: c.deceleration_rate,
            max_bounce_distance: c.max_bounce_distance,
            smartwheel_friction_max: c.smartwheel_friction_max,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub enabled: bool,
    pub custom_profiles: Vec<ProfileConfig>,
    pub selected_profile: String,
    pub debug: bool,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            custom_profiles: Vec::new(),
            selected_profile: String::from("Chrome"),
            debug: false,
        }
    }
}

fn config_dir() -> PathBuf {
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("ZenScroll")
}

fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

pub fn load() -> DaemonConfig {
    let path = config_path();
    match fs::read_to_string(&path) {
        Ok(content) => {
            match serde_json::from_str::<DaemonConfig>(&content) {
                Ok(cfg) => {
                    eprintln!("[ZenScroll] Config loaded from {:?}", path);
                    cfg
                }
                Err(e) => {
                    eprintln!("[ZenScroll] Config parse error: {}, using defaults", e);
                    DaemonConfig::default()
                }
            }
        }
        Err(_) => {
            eprintln!("[ZenScroll] No config file at {:?}, using defaults", path);
            DaemonConfig::default()
        }
    }
}

pub fn save(cfg: &DaemonConfig) {
    let dir = config_dir();
    if let Err(e) = fs::create_dir_all(&dir) {
        eprintln!("[ZenScroll] Failed to create config dir: {}", e);
        return;
    }
    let path = config_path();
    match serde_json::to_string_pretty(cfg) {
        Ok(content) => {
            if let Err(e) = fs::write(&path, &content) {
                eprintln!("[ZenScroll] Failed to write config: {}", e);
            } else {
                eprintln!("[ZenScroll] Config saved to {:?}", path);
            }
        }
        Err(e) => eprintln!("[ZenScroll] Config serialize error: {}", e),
    }
}
