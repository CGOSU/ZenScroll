use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use serde::{Deserialize, Serialize};

use zen_scroll_core::physics::{ScrollConfig, PRESETS, PRESET_NORMAL};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileConfig {
    pub name: String,
    #[serde(default)]
    pub process_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub enabled: bool,
    #[serde(default = "default_speed_preset")]
    pub speed_preset: usize,
    #[serde(default)]
    pub custom_profiles: Vec<ProfileConfig>,
    #[serde(default)]
    pub debug: bool,
}

fn default_speed_preset() -> usize { 1 }

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            speed_preset: 1,
            custom_profiles: Vec::new(),
            debug: false,
        }
    }
}

pub static DAEMON_CONFIG: std::sync::LazyLock<Mutex<DaemonConfig>> =
    std::sync::LazyLock::new(|| Mutex::new(DaemonConfig::default()));

pub fn load() -> DaemonConfig {
    let path = config_path();
    match fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str::<DaemonConfig>(&content) {
            Ok(cfg) => {
                eprintln!("[ZenScroll] 配置已加载: {:?}", path);
                cfg
            }
            Err(e) => {
                eprintln!("[ZenScroll] 配置解析错误: {}，使用默认值", e);
                DaemonConfig::default()
            }
        },
        Err(_) => {
            eprintln!("[ZenScroll] 无配置文件: {:?}，使用默认值", path);
            DaemonConfig::default()
        }
    }
}

pub fn reload() {
    let cfg = load();
    if let Ok(mut guard) = DAEMON_CONFIG.lock() {
        *guard = cfg;
    }
}

pub fn save(cfg: &DaemonConfig) {
    let dir = config_dir();
    if let Err(e) = fs::create_dir_all(&dir) {
        eprintln!("[ZenScroll] 创建配置目录失败: {}", e);
        return;
    }
    let path = config_path();
    match serde_json::to_string_pretty(cfg) {
        Ok(content) => {
            if let Err(e) = fs::write(&path, &content) {
                eprintln!("[ZenScroll] 写入配置失败: {}", e);
            } else {
                eprintln!("[ZenScroll] 配置已保存: {:?}", path);
            }
        }
        Err(e) => eprintln!("[ZenScroll] 配置序列化错误: {}", e),
    }
}

pub fn current_config() -> ScrollConfig {
    if let Ok(guard) = DAEMON_CONFIG.lock() {
        let idx = guard.speed_preset.min(2);
        PRESETS[idx].clone()
    } else {
        PRESET_NORMAL
    }
}

pub fn is_enabled() -> bool {
    if let Ok(guard) = DAEMON_CONFIG.lock() {
        guard.enabled
    } else {
        true
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
