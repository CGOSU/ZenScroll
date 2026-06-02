use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM, HINSTANCE, POINT, BOOL};
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, DefWindowProcW, DestroyWindow, SetForegroundWindow,
    TrackPopupMenu, AppendMenuW, CreateWindowExW, PostMessageW, DestroyMenu,
    PostQuitMessage, GetCursorPos,
    LookupIconIdFromDirectoryEx, CreateIconFromResourceEx,
    WM_APP, WM_COMMAND, WM_DESTROY, WM_LBUTTONUP, WM_RBUTTONUP, WM_CLOSE,
    WNDCLASSW, CW_USEDEFAULT, HICON, HCURSOR,
    WINDOW_STYLE, WS_EX_TOOLWINDOW,
    TPM_LEFTALIGN, TPM_RIGHTBUTTON,
    MF_STRING, MF_SEPARATOR, MF_GRAYED, MF_BYCOMMAND,
    LR_DEFAULTCOLOR,
    RegisterClassW,
};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NOTIFYICONDATAW,
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
};
use windows::core::PCWSTR;

use crate::config;
use crate::hook;

const WM_TRAY_ICON: u32 = WM_APP + 1;
const CMD_STATUS: u32 = 1000;
const CMD_TOGGLE: u32 = 1001;
const CMD_QUIT: u32 = 1002;

static TRAY_HWND: Mutex<Option<isize>> = Mutex::new(None);
static TRAY_EXIT: AtomicBool = AtomicBool::new(false);

fn to_wstr(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe extern "system" fn tray_window_proc(
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
                }
                update_tray_tip(hwnd);
            }
            CMD_QUIT => {
                TRAY_EXIT.store(true, Ordering::SeqCst);
                unsafe {
                    let _ = PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            }
            _ => {}
        }
        return LRESULT(0);
    }

    if msg == WM_DESTROY {
        remove_tray_icon(hwnd);
        unsafe {
            PostQuitMessage(0);
        }
        return LRESULT(0);
    }

    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn show_context_menu(hwnd: HWND) {
    unsafe {
        let menu = CreatePopupMenu().unwrap();
        let enabled = hook::HOOK_STATE.lock().map(|s| s.enabled).unwrap_or(true);

        let status_w = to_wstr(if enabled { " Running" } else { " Stopped" });
        let _ = AppendMenuW(
            menu,
            MF_STRING | MF_GRAYED | MF_BYCOMMAND,
            CMD_STATUS as usize,
            PCWSTR::from_raw(status_w.as_ptr()),
        );

        let _ = AppendMenuW(menu, MF_SEPARATOR | MF_BYCOMMAND, 0, PCWSTR::null());

        let toggle_w = to_wstr(if enabled { "Disable" } else { "Enable" });
        let _ = AppendMenuW(
            menu,
            MF_STRING | MF_BYCOMMAND,
            CMD_TOGGLE as usize,
            PCWSTR::from_raw(toggle_w.as_ptr()),
        );

        let _ = AppendMenuW(menu, MF_SEPARATOR | MF_BYCOMMAND, 0, PCWSTR::null());

        let quit_w = to_wstr("Quit");
        let _ = AppendMenuW(
            menu,
            MF_STRING | MF_BYCOMMAND,
            CMD_QUIT as usize,
            PCWSTR::from_raw(quit_w.as_ptr()),
        );

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(hwnd);
        let _ = TrackPopupMenu(menu, TPM_LEFTALIGN | TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, None);
        let _ = DestroyMenu(menu);
    }
}

fn remove_tray_icon(hwnd: HWND) {
    unsafe {
        let mut nid = NOTIFYICONDATAW::default();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
    }
}

fn update_tray_tip(hwnd: HWND) {
    unsafe {
        let enabled = hook::HOOK_STATE.lock().map(|s| s.enabled).unwrap_or(true);
        let proc_name = hook::HOOK_STATE.lock().map(|s| s.current_process.clone()).unwrap_or_default();
        let status = if enabled { "Running" } else { "Stopped" };
        let tip = if proc_name.is_empty() {
            format!("ZenScroll [{}]", status)
        } else {
            format!("ZenScroll [{}] - {}", status, proc_name)
        };

        let tip_wide = to_wstr(&tip);

        let mut nid = NOTIFYICONDATAW::default();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_TIP;
        let len = tip_wide.len().min(128);
        nid.szTip[..len].copy_from_slice(&tip_wide[..len]);
        let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
    }
}

pub fn create_tray_window() -> HWND {
    unsafe {
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

        RegisterClassW(&wc);

        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class_ptr,
            PCWSTR::null(),
            WINDOW_STYLE(0),
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            None,
            None,
            HINSTANCE(std::ptr::null_mut()),
            None,
        ).unwrap();

        static ICON_DATA: &[u8] = include_bytes!("../assets/icon.ico");
        let entry_off = LookupIconIdFromDirectoryEx(
            ICON_DATA.as_ptr(),
            BOOL::from(true),
            32, 32,
            LR_DEFAULTCOLOR,
        );
        let icon = if entry_off > 0 {
            let data = &ICON_DATA[entry_off as usize..];
            CreateIconFromResourceEx(data, BOOL::from(true), 0x00030000, 32, 32, LR_DEFAULTCOLOR)
                .unwrap_or(HICON::default())
        } else {
            HICON::default()
        };

        let msg_id: u32 = WM_TRAY_ICON;
        let tip_wide = to_wstr("ZenScroll");

        let mut nid = NOTIFYICONDATAW::default();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        nid.uCallbackMessage = msg_id;
        nid.hIcon = icon;
        let len = tip_wide.len().min(128);
        nid.szTip[..len].copy_from_slice(&tip_wide[..len]);

        let _ = Shell_NotifyIconW(NIM_ADD, &nid);

        *TRAY_HWND.lock().unwrap() = Some(hwnd.0 as isize);

        hwnd
    }
}

pub fn destroy_tray() {
    if let Ok(guard) = TRAY_HWND.lock() {
        if let Some(hwnd) = *guard {
            unsafe {
                let hwnd = HWND(hwnd as *mut _);
                remove_tray_icon(hwnd);
                let _ = DestroyWindow(hwnd);
            }
        }
    }
}

pub fn signal_quit() {
    TRAY_EXIT.store(true, Ordering::SeqCst);
    if let Ok(guard) = TRAY_HWND.lock() {
        if let Some(hwnd) = *guard {
            unsafe {
                let hwnd = HWND(hwnd as *mut _);
                let _ = PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
    }
}

pub fn should_exit() -> bool {
    TRAY_EXIT.load(Ordering::SeqCst)
}
