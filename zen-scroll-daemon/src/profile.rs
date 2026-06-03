use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct AppProfile {
    pub name: String,
    pub process_names: Vec<String>,
    pub enabled: bool,
}

pub static BUILTIN_PROFILES: std::sync::LazyLock<Mutex<Vec<AppProfile>>> =
    std::sync::LazyLock::new(|| Mutex::new(vec![
    AppProfile {
        name: "Chrome".into(),
        process_names: vec!["chrome.exe".into(), "msedge.exe".into(), "brave.exe".into(), "opera.exe".into()],
        enabled: true,
    },
    AppProfile {
        name: "Readest".into(),
        process_names: vec!["readest.exe".into()],
        enabled: true,
    },
    AppProfile {
        name: "Firefox".into(),
        process_names: vec!["firefox.exe".into()],
        enabled: true,
    },
]));

pub fn find_profile(process_name: &str) -> Option<AppProfile> {
    let name_lower = process_name.to_lowercase();
    let guard = BUILTIN_PROFILES.lock().ok()?;
    guard.iter().find(|p| {
        p.process_names.contains(&name_lower)
    }).cloned()
}

pub fn apply_custom_profiles(profiles: &[crate::config::ProfileConfig]) {
    if let Ok(mut guard) = BUILTIN_PROFILES.lock() {
        for custom in profiles {
            if let Some(builtin) = guard.iter_mut().find(|p| p.name == custom.name) {
                if !custom.process_names.is_empty() {
                    builtin.process_names = custom.process_names.clone();
                }
                eprintln!("[ZenScroll] Custom profile '{}' updated with {} process(es)",
                    custom.name, builtin.process_names.len());
            } else {
                eprintln!("[ZenScroll] Custom profile '{}' does not match any built-in, ignored", custom.name);
            }
        }
    }
}
