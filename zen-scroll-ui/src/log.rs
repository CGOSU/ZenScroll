use std::sync::atomic::{AtomicBool, Ordering};

pub static DEBUG_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn set_debug(enabled: bool) {
    DEBUG_ENABLED.store(enabled, Ordering::SeqCst);
}

#[macro_export]
macro_rules! debug_log {
    ($($arg:tt)*) => {
        if $crate::log::DEBUG_ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
            let dur = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            let total_ms = dur.as_millis() as u64;
            let h = total_ms / 3_600_000 % 24;
            let m = total_ms / 60_000 % 60;
            let s = total_ms / 1_000 % 60;
            let ms = total_ms % 1_000;
            eprintln!("[{:02}:{:02}:{:02}.{:03}] {}", h, m, s, ms, format!($($arg)*));
        }
    };
}
