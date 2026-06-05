use std::fs;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::Mutex;
use serde::{Deserialize, Serialize};

use zen_scroll_core::physics::{ScrollConfig, PRESETS, PRESET_NORMAL};

const REG_RUN_PATH: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const REG_VALUE_NAME: &str = "ZenScroll";
const EVENT_NAME: &str = "ZenScrollConfigChange";

unsafe extern "system" {
    fn RegOpenKeyExW(
        hKey: isize,
        lpSubKey: *const u16,
        ulOptions: u32,
        samDesired: u32,
        phkResult: *mut isize,
    ) -> i32;
    fn RegSetValueExW(
        hKey: isize,
        lpValueName: *const u16,
        Reserved: u32,
        dwType: u32,
        lpData: *const u8,
        cbData: u32,
    ) -> i32;
    fn RegDeleteValueW(hKey: isize, lpValueName: *const u16) -> i32;
    fn RegCloseKey(hKey: isize) -> i32;
    fn OpenEventW(dwDesiredAccess: u32, bInheritHandle: i32, lpName: *const u16) -> isize;
    fn SetEvent(hEvent: isize) -> i32;
    fn CloseHandle(hObject: isize) -> i32;
}

const HKEY_CURRENT_USER: isize = -2147483647;
const KEY_SET_VALUE: u32 = 0x0002;
const REG_SZ: u32 = 1;
const ERROR_SUCCESS: i32 = 0;
const EVENT_ALL_ACCESS: u32 = 0x1F0003;

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
    #[serde(default)]
    pub autostart: bool,
}

fn default_speed_preset() -> usize { 1 }

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            speed_preset: 1,
            custom_profiles: Vec::new(),
            debug: false,
            autostart: false,
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
                signal_config_event();
            }
        }
        Err(e) => eprintln!("[ZenScroll] 配置序列化错误: {}", e),
    }
}

fn signal_config_event() {
    let name: Vec<u16> = EVENT_NAME.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: OpenEventW opens the named event created by zen-scroll-ui. If the UI isn't running, it fails silently.
    let ev = unsafe { OpenEventW(EVENT_ALL_ACCESS, 0, name.as_ptr()) };
    if ev == 0 || ev == -1_isize {
        return;
    }
    // SAFETY: SetEvent signals the event, waking the UI's background thread.
    unsafe { SetEvent(ev); }
    // SAFETY: CloseHandle releases the event handle opened by OpenEventW.
    unsafe { CloseHandle(ev); }
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

fn reg_key() -> Option<isize> {
    let wide: Vec<u16> = REG_RUN_PATH.encode_utf16().chain(std::iter::once(0)).collect();
    let mut key: isize = 0;
    // SAFETY: RegOpenKeyExW opens an existing registry key for writing.
    // HKEY_CURRENT_USER is a predefined handle, and KEY_SET_VALUE is sufficient for writing values.
    let rc = unsafe {
        RegOpenKeyExW(HKEY_CURRENT_USER, wide.as_ptr(), 0, KEY_SET_VALUE, &mut key)
    };
    if rc == ERROR_SUCCESS { Some(key) } else { None }
}

fn set_autostart() {
    let Some(key) = reg_key() else {
        eprintln!("[ZenScroll] 无法打开注册表自启动项");
        return;
    };
    let exe = std::env::current_exe().ok();
    let Some(path) = exe else {
        unsafe { RegCloseKey(key); }
        return;
    };
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let name: Vec<u16> = REG_VALUE_NAME.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: RegSetValueExW writes the daemon path as a REG_SZ value under HKCU\...\Run.
    // key is a valid handle from reg_key(), wide is a null-terminated UTF-16 string.
    unsafe {
        RegSetValueExW(key, name.as_ptr(), 0, REG_SZ, wide.as_ptr() as *const u8, (wide.len() * 2) as u32);
        RegCloseKey(key);
    }
    eprintln!("[ZenScroll] 已设置开机自启动");
}

fn unset_autostart() {
    let Some(key) = reg_key() else {
        return;
    };
    let name: Vec<u16> = REG_VALUE_NAME.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: RegDeleteValueW removes the ZenScroll value from the Run key.
    // key is a valid handle from reg_key().
    unsafe {
        RegDeleteValueW(key, name.as_ptr());
        RegCloseKey(key);
    }
    eprintln!("[ZenScroll] 已取消开机自启动");
}

pub fn sync_autostart(autostart: bool) {
    if autostart {
        set_autostart();
    } else {
        unset_autostart();
    }
}
