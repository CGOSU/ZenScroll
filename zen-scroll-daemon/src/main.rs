mod config;
mod detect;
mod hook;
mod log;
mod profile;
mod smoother;
mod tray;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread;
use std::time::Duration;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, SetProcessDPIAware, SPI_GETWHEELSCROLLLINES,
    SPI_SETWHEELSCROLLLINES, SPIF_UPDATEINIFILE, SystemParametersInfoW, TranslateMessage,
};

const TICK_INTERVAL: Duration = Duration::from_millis(4);
const WHEEL_PAGESCROLL: u32 = 0;

static RUNNING: AtomicBool = AtomicBool::new(true);
static ORIGINAL_SCROLL_LINES: AtomicU32 = AtomicU32::new(3);

fn save_and_override_scroll_lines() {
    let mut lines: u32 = 0;
    let ptr = &mut lines as *mut u32 as *mut core::ffi::c_void;
    // SAFETY: SystemParametersInfoW queries the system wheel scroll lines. ptr points to a valid u32.
    let ok = unsafe {
        SystemParametersInfoW(SPI_GETWHEELSCROLLLINES, 0, Some(ptr), SPIF_UPDATEINIFILE).is_ok()
    };
    if ok {
        ORIGINAL_SCROLL_LINES.store(lines, Ordering::SeqCst);
        if lines == WHEEL_PAGESCROLL {
            eprintln!("[ZenScroll] 系统滚轮为'翻页'模式，已覆盖为 1 行");
            let val: u32 = 1;
            let ptr = &val as *const u32 as *mut core::ffi::c_void;
            // SAFETY: SystemParametersInfoW sets the wheel scroll lines to 1. ptr is a valid u32.
            let _ = unsafe {
                SystemParametersInfoW(SPI_SETWHEELSCROLLLINES, 1, Some(ptr), SPIF_UPDATEINIFILE)
            };
        } else {
            eprintln!("[ZenScroll] 系统滚轮行数 = {}，保持不变", lines);
        }
    }
}

fn restore_scroll_lines() {
    let original = ORIGINAL_SCROLL_LINES.load(Ordering::SeqCst);
    if original == WHEEL_PAGESCROLL {
        let val: u32 = WHEEL_PAGESCROLL;
        let ptr = &val as *const u32 as *mut core::ffi::c_void;
        // SAFETY: SystemParametersInfoW restores the system wheel scroll lines to WHEEL_PAGESCROLL.
        unsafe {
            let _ =
                SystemParametersInfoW(SPI_SETWHEELSCROLLLINES, 0, Some(ptr), SPIF_UPDATEINIFILE);
        }
        eprintln!("[ZenScroll] 已恢复系统滚轮为'翻页'模式");
    }
}

fn main() {
    // SAFETY: SetProcessDPIAware makes the process system DPI aware so that GetCursorPos
    // returns physical pixel coordinates matching MSLLHOOKSTRUCT.pt.
    unsafe { let _ = SetProcessDPIAware(); }
    save_and_override_scroll_lines();
    config::reload();

    if let Ok(cfg) = config::DAEMON_CONFIG.lock() {
        log::set_debug(cfg.debug);
        if !cfg.custom_profiles.is_empty() {
            eprintln!(
                "[ZenScroll] 已加载 {} 个自定义配置",
                cfg.custom_profiles.len()
            );
            profile::apply_custom_profiles(&cfg.custom_profiles);
        }
    }

    if let Err(e) = hook::install_hook() {
        eprintln!("[ZenScroll] 安装钩子失败: {}", e);
        restore_scroll_lines();
        return;
    }

    {
        if let Ok(mut state) = hook::HOOK_STATE.lock() {
            state.enabled = config::is_enabled();
        }
    }

    let _tray_hwnd = tray::create_tray_window();

    println!(
        "[ZenScroll v{}] 系统滚轮优化已启动 (启用={})",
        env!("CARGO_PKG_VERSION"),
        config::is_enabled()
    );
    println!("[ZenScroll] 右键托盘图标控制");

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        println!("\n[ZenScroll] 正在关闭...");
        r.store(false, Ordering::SeqCst);
        tray::signal_quit();
    })
    .expect("设置 Ctrl+C 处理器失败");

    let inject_handle = thread::spawn(move || {
        injection_loop();
    });

    let mut msg = MSG::default();
    // SAFETY: Standard Windows message pump. GetMessageW blocks until a message arrives;
    // TranslateMessage/DispatchMessageW process and dispatch it to the window procedure.
    unsafe {
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
