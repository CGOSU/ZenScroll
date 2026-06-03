mod config;
mod detect;
mod hook;
mod log;
mod profile;
mod smoother;
mod tray;

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, SPI_GETWHEELSCROLLLINES, SPI_SETWHEELSCROLLLINES,
    SPIF_UPDATEINIFILE, SystemParametersInfoW, TranslateMessage,
};

const TICK_INTERVAL: Duration = Duration::from_millis(8);
const WHEEL_PAGESCROLL: u32 = 0;

static RUNNING: AtomicBool = AtomicBool::new(true);
static ORIGINAL_SCROLL_LINES: AtomicU32 = AtomicU32::new(3);

fn save_and_override_scroll_lines() {
    unsafe {
        let mut lines: u32 = 0;
        let ptr = &mut lines as *mut u32 as *mut core::ffi::c_void;
        if SystemParametersInfoW(SPI_GETWHEELSCROLLLINES, 0, Some(ptr), SPIF_UPDATEINIFILE).is_ok()
        {
            ORIGINAL_SCROLL_LINES.store(lines, Ordering::SeqCst);
            if lines == WHEEL_PAGESCROLL {
                eprintln!("[ZenScroll] System scroll was 'one page', overriding to 1 line");
                let val: u32 = 1;
                let ptr = &val as *const u32 as *mut core::ffi::c_void;
                let _ = SystemParametersInfoW(
                    SPI_SETWHEELSCROLLLINES,
                    1,
                    Some(ptr),
                    SPIF_UPDATEINIFILE,
                );
            } else {
                eprintln!("[ZenScroll] System scroll lines = {}, keeping as-is", lines);
            }
        }
    }
}

fn restore_scroll_lines() {
    let original = ORIGINAL_SCROLL_LINES.load(Ordering::SeqCst);
    if original == WHEEL_PAGESCROLL {
        unsafe {
            let val: u32 = WHEEL_PAGESCROLL;
            let ptr = &val as *const u32 as *mut core::ffi::c_void;
            let _ = SystemParametersInfoW(
                SPI_SETWHEELSCROLLLINES,
                0,
                Some(ptr),
                SPIF_UPDATEINIFILE,
            );
        }
        eprintln!("[ZenScroll] Restored system scroll to 'one page'");
    }
}

fn main() {
    save_and_override_scroll_lines();
    config::reload();

    if let Ok(cfg) = config::DAEMON_CONFIG.lock() {
        log::set_debug(cfg.debug);
        if !cfg.custom_profiles.is_empty() {
            eprintln!("[ZenScroll] Loaded {} custom profiles", cfg.custom_profiles.len());
            profile::apply_custom_profiles(&cfg.custom_profiles);
        }
    }

    if let Err(e) = hook::install_hook() {
        eprintln!("[ZenScroll] Failed to install hook: {}", e);
        restore_scroll_lines();
        return;
    }

    {
        if let Ok(mut state) = hook::HOOK_STATE.lock() {
            state.enabled = config::is_enabled();
        }
    }

    let _tray_hwnd = tray::create_tray_window();

    println!("[ZenScroll] System scroll optimizer started (enabled={})", config::is_enabled());
    println!("[ZenScroll] Right-click tray icon to control");

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        println!("\n[ZenScroll] Shutting down...");
        r.store(false, Ordering::SeqCst);
        tray::signal_quit();
    })
    .expect("Failed to set Ctrl+C handler");

    let inject_handle = thread::spawn(move || {
        injection_loop();
    });

    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
        }
    }

    restore_scroll_lines();
    tray::destroy_tray();
    inject_handle.join().ok();
}

fn injection_loop() {
    loop {
        if !RUNNING.load(Ordering::SeqCst) || tray::should_exit() {
            break;
        }

        if let Ok(mut state) = hook::HOOK_STATE.lock() {
            state.injector.tick();
        }

        thread::sleep(TICK_INTERVAL);
    }
}
