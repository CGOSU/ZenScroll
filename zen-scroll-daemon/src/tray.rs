use std::os::windows::ffi::OsStrExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM, HINSTANCE, POINT, BOOL};
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, DefWindowProcW, DestroyWindow, SetForegroundWindow,
    TrackPopupMenu, AppendMenuW, CreateWindowExW, PostMessageW, DestroyMenu,
    PostQuitMessage, GetCursorPos, FindWindowW, ShowWindow,
    LookupIconIdFromDirectoryEx, CreateIconFromResourceEx,
    WM_APP, WM_COMMAND, WM_DESTROY, WM_LBUTTONUP, WM_RBUTTONUP, WM_CLOSE, SW_SHOW, SW_HIDE,
    WNDCLASSW, CW_USEDEFAULT, HICON, HCURSOR,
    WINDOW_STYLE, WS_EX_TOOLWINDOW,
    TPM_LEFTALIGN, TPM_RIGHTBUTTON,
    MF_STRING, MF_SEPARATOR, MF_GRAYED, MF_BYCOMMAND,
    LR_DEFAULTCOLOR,
    RegisterClassW,
};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NOTIFYICONDATAW, ShellExecuteW,
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
};
use windows::core::PCWSTR;

use crate::config;
use crate::hook;
use crate::log;
use crate::profile;

const WM_TRAY_ICON: u32 = WM_APP + 1;
const CMD_STATUS: u32 = 1000;
const CMD_TOGGLE: u32 = 1001;
const CMD_QUIT: u32 = 1002;
const CMD_LAUNCH_UI: u32 = 1003;

static TRAY_HWND: Mutex<Option<isize>> = Mutex::new(None);
static TRAY_EXIT: AtomicBool = AtomicBool::new(false);

fn to_wstr(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn launch_ui() {
    let exe = std::env::current_exe().ok();
    let ui_path = exe.as_ref()
        .and_then(|p| p.parent())
        .map(|dir| dir.join("zen-scroll-ui.exe"));
    let Some(path) = ui_path else { return };
    if !path.exists() { return; }

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let verb: Vec<u16> = "open\0".encode_utf16().collect();

    // SAFETY: ShellExecuteW("open") launches zen-scroll-ui.exe as a non-admin process.
    unsafe {
        ShellExecuteW(
            HWND(std::ptr::null_mut()),
            PCWSTR::from_raw(verb.as_ptr()),
            PCWSTR::from_raw(wide.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOW,
        );
    }
}

extern "system" fn tray_window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_TRAY_ICON {
        match lparam.0 as u32 {
            WM_LBUTTONUP => {
                if let Ok(mut state) = hook::HOOK_STATE.lock() {
                    state.enabled = !state.enabled;
                    let mut cfg = config::load();
                    cfg.enabled = state.enabled;
                    config::save(&cfg);
                    config::reload();
                }
                update_tray_tip(hwnd);
            }
            WM_RBUTTONUP => {
                show_context_menu(hwnd);
            }
            _ => {}
        }
        return LRESULT(0);
    }

    if msg == WM_COMMAND {
        let id = (wparam.0 & 0xFFFF) as u32;
        match id {
            CMD_TOGGLE => {
                if let Ok(mut state) = hook::HOOK_STATE.lock() {
                    state.enabled = !state.enabled;
                    let mut cfg = config::load();
                    cfg.enabled = state.enabled;
                    config::save(&cfg);
                    config::reload();
                }
                update_tray_tip(hwnd);
            }
            CMD_LAUNCH_UI => {
                launch_ui();
            }
            CMD_QUIT => {
                TRAY_EXIT.store(true, Ordering::SeqCst);
                // SAFETY: PostMessageW sends WM_CLOSE to the tray window, which triggers WM_DESTROY and cleanup.
            unsafe { let _ = PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0)); }
            }
            _ => {}
        }
        return LRESULT(0);
    }

    if msg == WM_APP {
        config::reload();
        if let Ok(mut state) = hook::HOOK_STATE.lock() {
            state.enabled = config::is_enabled();
            state.injector.set_config(config::current_config());
        }
        if let Ok(guard) = config::DAEMON_CONFIG.lock() {
            profile::apply_custom_profiles(&guard.custom_profiles);
            log::set_debug(guard.debug);
            config::sync_autostart(guard.autostart);
            // SAFETY: GetConsoleWindow retrieves the console window handle (null if none).
            let console = unsafe { windows::Win32::System::Console::GetConsoleWindow() };
            if console.0.is_null() {
                if guard.debug {
                    // SAFETY: AllocConsole creates a new console for debug output.
                    unsafe { let _ = windows::Win32::System::Console::AllocConsole(); }
                }
            } else {
                // SAFETY: ShowWindow shows or hides the console based on debug state.
                unsafe {
                    let _ = ShowWindow(console, if guard.debug { SW_SHOW } else { SW_HIDE });
                }
            }
        }
        eprintln!("[ZenScroll] IPC 配置已重载");
        update_tray_tip(hwnd);
        return LRESULT(0);
    }

    if msg == WM_DESTROY {
        remove_tray_icon(hwnd);
        // SAFETY: PostQuitMessage posts WM_QUIT to the message queue, causing GetMessageW to return false.
        unsafe { PostQuitMessage(0); }
        return LRESULT(0);
    }

    // SAFETY: DefWindowProcW provides default processing for unhandled window messages.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn show_context_menu(hwnd: HWND) {
    // SAFETY: CreatePopupMenu creates a system menu handle. Returns null on failure.
    let menu = unsafe { CreatePopupMenu() };
    let Ok(menu) = menu else { return };
    let enabled = hook::HOOK_STATE.lock().map(|s| s.enabled).unwrap_or(true);

    let status_w = to_wstr(if enabled { " 运行中" } else { " 已停止" });
    let toggle_w = to_wstr(if enabled { "禁用" } else { "启用" });
    let launch_w = to_wstr("控制面板");
    let quit_w = to_wstr("退出");

    // SAFETY: Context menu lifecycle: AppendMenuW populates the menu, GetCursorPos retrieves cursor
    // position for placement, SetForegroundWindow ensures proper menu dismissal on click-away,
    // TrackPopupMenu displays the menu synchronously, DestroyMenu releases the menu resources.
    unsafe {
        let _ = AppendMenuW(menu, MF_STRING | MF_GRAYED | MF_BYCOMMAND, CMD_STATUS as usize, PCWSTR::from_raw(status_w.as_ptr()));
        let _ = AppendMenuW(menu, MF_SEPARATOR | MF_BYCOMMAND, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, MF_STRING | MF_BYCOMMAND, CMD_TOGGLE as usize, PCWSTR::from_raw(toggle_w.as_ptr()));
        let _ = AppendMenuW(menu, MF_SEPARATOR | MF_BYCOMMAND, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, MF_STRING | MF_BYCOMMAND, CMD_LAUNCH_UI as usize, PCWSTR::from_raw(launch_w.as_ptr()));
        let _ = AppendMenuW(menu, MF_SEPARATOR | MF_BYCOMMAND, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, MF_STRING | MF_BYCOMMAND, CMD_QUIT as usize, PCWSTR::from_raw(quit_w.as_ptr()));

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(hwnd);
        let _ = TrackPopupMenu(menu, TPM_LEFTALIGN | TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, None);
        let _ = DestroyMenu(menu);
    }
}

fn remove_tray_icon(hwnd: HWND) {
    let nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        ..Default::default()
    };
    // SAFETY: Shell_NotifyIconW(NIM_DELETE) removes the tray icon from the notification area.
    unsafe { let _ = Shell_NotifyIconW(NIM_DELETE, &nid); }
}

fn update_tray_tip(hwnd: HWND) {
    let enabled = hook::HOOK_STATE.lock().map(|s| s.enabled).unwrap_or(true);
    let proc_name = hook::HOOK_STATE.lock().map(|s| s.current_process.clone()).unwrap_or_default();
    let tip = if proc_name.is_empty() {
        format!("ZenScroll [{}]", if enabled { "运行中" } else { "已停止" })
    } else {
        format!("ZenScroll [{}] - {}", if enabled { "运行中" } else { "已停止" }, proc_name)
    };

    let tip_wide = to_wstr(&tip);

    let mut nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_TIP,
        ..Default::default()
    };
    let len = tip_wide.len().min(128);
    nid.szTip[..len].copy_from_slice(&tip_wide[..len]);
    // SAFETY: Shell_NotifyIconW(NIM_MODIFY) updates the tooltip text for the tray icon.
    unsafe { let _ = Shell_NotifyIconW(NIM_MODIFY, &nid); }
}

pub fn create_tray_window() -> HWND {
    let class_name = to_wstr("ZenScrollTray");
    let class_ptr = PCWSTR::from_raw(class_name.as_ptr());

    let wc = WNDCLASSW {
        style: Default::default(),
        lpfnWndProc: Some(tray_window_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: HINSTANCE(std::ptr::null_mut()),
        hIcon: HICON::default(),
        hCursor: HCURSOR::default(),
        hbrBackground: HBRUSH::default(),
        lpszMenuName: PCWSTR::null(),
        lpszClassName: class_ptr,
    };

    // SAFETY: RegisterClassW registers the "ZenScrollTray" window class. The WNDCLASSW struct is fully initialized.
    unsafe { RegisterClassW(&wc); }

    // SAFETY: CreateWindowExW creates a WS_EX_TOOLWINDOW hidden window for tray message processing.
    let hwnd = match unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class_ptr,
            PCWSTR::null(),
            WINDOW_STYLE(0),
            CW_USEDEFAULT, CW_USEDEFAULT, CW_USEDEFAULT, CW_USEDEFAULT,
            None, None,
            HINSTANCE(std::ptr::null_mut()),
            None,
        )
    } {
        Ok(h) => h,
        Err(_) => {
            eprintln!("[ZenScroll] 创建托盘窗口失败");
            return HWND(std::ptr::null_mut());
        }
    };

    static ICON_DATA: &[u8] = include_bytes!("../assets/icon.ico");
    // SAFETY: LookupIconIdFromDirectoryEx finds the best-matching icon entry in the .ico data.
    // ICON_DATA is a valid .ico file embedded at compile time.
    let entry_off = unsafe {
        LookupIconIdFromDirectoryEx(ICON_DATA.as_ptr(), BOOL::from(true), 32, 32, LR_DEFAULTCOLOR)
    };
    let icon = if entry_off > 0 {
        let data = &ICON_DATA[entry_off as usize..];
        // SAFETY: CreateIconFromResourceEx creates an HICON from the icon entry identified by LookupIconIdFromDirectoryEx.
        // data is a valid icon resource within the statically embedded .ico bytes.
        unsafe {
            CreateIconFromResourceEx(data, BOOL::from(true), 0x00030000, 32, 32, LR_DEFAULTCOLOR)
        }.unwrap_or(HICON::default())
    } else {
        HICON::default()
    };

    let mut nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: WM_TRAY_ICON,
        hIcon: icon,
        ..Default::default()
    };
    let tip_wide = to_wstr("ZenScroll");
    let len = tip_wide.len().min(128);
    nid.szTip[..len].copy_from_slice(&tip_wide[..len]);

    // SAFETY: Shell_NotifyIconW(NIM_ADD) creates the tray icon in the notification area with icon and tooltip.
    unsafe { let _ = Shell_NotifyIconW(NIM_ADD, &nid); }

    if let Ok(mut guard) = TRAY_HWND.lock() {
        *guard = Some(hwnd.0 as isize);
    }

    hwnd
}

pub fn destroy_tray() {
    if let Ok(guard) = TRAY_HWND.lock()
        && let Some(hwnd) = *guard
    {
        // SAFETY: hwnd is a valid window handle stored by create_tray_window().
        let h = HWND(hwnd as *mut _);
        remove_tray_icon(h);
        // SAFETY: DestroyWindow destroys the tray window and sends WM_DESTROY which triggers cleanup.
        unsafe { let _ = DestroyWindow(h); }
    }
}

pub fn signal_quit() {
    TRAY_EXIT.store(true, Ordering::SeqCst);
    if let Ok(guard) = TRAY_HWND.lock()
        && let Some(hwnd) = *guard
    {
        // SAFETY: hwnd is a valid window handle from TRAY_HWND.
        // PostMessageW sends WM_CLOSE which triggers the window to close and the message pump to exit.
        unsafe { let _ = PostMessageW(HWND(hwnd as *mut _), WM_CLOSE, WPARAM(0), LPARAM(0)); }
    }
}

pub fn should_exit() -> bool {
    TRAY_EXIT.load(Ordering::SeqCst)
}

#[allow(dead_code)]
pub fn find_daemon_hwnd() -> Option<isize> {
    let class_name = to_wstr("ZenScrollTray");
    // SAFETY: FindWindowW searches for the "ZenScrollTray" window class created by the daemon.
    let hwnd = unsafe { FindWindowW(PCWSTR::from_raw(class_name.as_ptr()), None) };
    if let Ok(h) = hwnd
        && !h.0.is_null()
    {
        return Some(h.0 as isize);
    }
    None
}

#[allow(dead_code)]
pub fn signal_reload_to(hwnd: isize) {
    // SAFETY: hwnd is a valid daemon tray window handle.
    unsafe { let _ = PostMessageW(HWND(hwnd as *mut _), WM_APP, WPARAM(0), LPARAM(0)); }
}
