mod config;
mod detect;
mod hook;
mod log;
mod profile;
mod smoother;
mod tray;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, TranslateMessage, DispatchMessageW};

const TICK_INTERVAL: Duration = Duration::from_millis(8);

static RUNNING: AtomicBool = AtomicBool::new(true);

fn main() {
    let daemon_cfg = config::load();
    log::set_debug(daemon_cfg.debug);
    if !daemon_cfg.custom_profiles.is_empty() {
        eprintln!("[ZenScroll] Loaded {} custom profiles", daemon_cfg.custom_profiles.len());
        profile::apply_custom_profiles(&daemon_cfg.custom_profiles);
    }

    if let Err(e) = hook::install_hook() {
        eprintln!("[ZenScroll] Failed to install hook: {}", e);
        return;
    }

    {
        if let Ok(mut state) = hook::HOOK_STATE.lock() {
            state.enabled = daemon_cfg.enabled;
        }
    }

    let _tray_hwnd = tray::create_tray_window();

    println!("[ZenScroll] System scroll optimizer started (enabled={})", daemon_cfg.enabled);
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
