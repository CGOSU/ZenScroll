use std::sync::Mutex;
use zen_scroll_core::physics::ScrollConfig;

#[derive(Debug, Clone)]
pub struct AppProfile {
    pub name: &'static str,
    pub process_names: &'static [&'static str],
    pub config: ScrollConfig,
    pub enabled: bool,
}

pub static BUILTIN_PROFILES: std::sync::LazyLock<Mutex<Vec<AppProfile>>> =
    std::sync::LazyLock::new(|| Mutex::new(vec![
    AppProfile {
        name: "Chrome",
        process_names: &["chrome.exe", "msedge.exe", "brave.exe", "opera.exe"],
        config: ScrollConfig {
            friction: 0.94,
            bounce_tension: 0.85,
            min_velocity: 0.3,
            max_velocity: 200.0,
            scroll_accel: 1.5,
            deceleration_rate: 0.998,
            max_bounce_distance: 150.0,
        },
        enabled: true,
    },
    AppProfile {
        name: "Readest",
        process_names: &["readest.exe"],
        config: ScrollConfig {
            friction: 0.96,
            bounce_tension: 0.90,
            min_velocity: 0.2,
            max_velocity: 120.0,
            scroll_accel: 1.0,
            deceleration_rate: 0.999,
            max_bounce_distance: 80.0,
        },
        enabled: true,
    },
    AppProfile {
        name: "Firefox",
        process_names: &["firefox.exe"],
        config: ScrollConfig {
            friction: 0.93,
            bounce_tension: 0.85,
            min_velocity: 0.4,
            max_velocity: 180.0,
            scroll_accel: 1.3,
            deceleration_rate: 0.998,
            max_bounce_distance: 150.0,
        },
        enabled: true,
    },
]));

pub fn find_profile(process_name: &str) -> Option<AppProfile> {
    let name_lower = process_name.to_lowercase();
    let guard = BUILTIN_PROFILES.lock().ok()?;
    guard.iter().find(|p| {
        p.process_names
            .iter()
            .any(|pn| name_lower == *pn)
    }).cloned()
}

pub fn apply_custom_profiles(profiles: &[crate::config::ProfileConfig]) {
    if let Ok(mut guard) = BUILTIN_PROFILES.lock() {
        for custom in profiles {
            if let Some(builtin) = guard.iter_mut().find(|p| p.name == custom.name) {
                builtin.config = ScrollConfig::from(custom);
                eprintln!("[ZenScroll] Override profile: {} (μ={} ξ={} a={} V={} v={})",
                    custom.name, custom.friction, custom.bounce_tension,
                    custom.scroll_accel, custom.max_velocity, custom.min_velocity);
            } else {
                eprintln!("[ZenScroll] Custom profile '{}' does not match any built-in, ignored", custom.name);
            }
        }
    }
}

#[allow(dead_code)]
pub fn get_enabled_profiles() -> Vec<AppProfile> {
    BUILTIN_PROFILES.lock()
        .ok()
        .map(|g| g.iter().filter(|p| p.enabled).cloned().collect())
        .unwrap_or_default()
}
