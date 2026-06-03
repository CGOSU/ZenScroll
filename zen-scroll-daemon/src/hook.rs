use crate::config;
use crate::debug_log;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, MSG, MSLLHOOKSTRUCT, SetWindowsHookExW, UnhookWindowsHookEx,
    WH_MOUSE_LL, HHOOK,
    WM_MOUSEWHEEL,
};

use crate::detect::TargetWindow;
use crate::profile::find_profile;
use crate::smoother::{INJECTING, SmoothInjector};

pub struct HookState {
    pub injector: SmoothInjector,
    pub enabled: bool,
    pub current_process: String,
}

impl HookState {
    pub fn new() -> Self {
        Self {
            injector: SmoothInjector::new(Default::default()),
            enabled: true,
            current_process: String::new(),
        }
    }
}

pub static HOOK_STATE: std::sync::LazyLock<Mutex<HookState>> =
    std::sync::LazyLock::new(|| Mutex::new(HookState::new()));

pub static HOOK_HANDLE: std::sync::Mutex<Option<isize>> = std::sync::Mutex::new(None);

extern "system" fn low_level_mouse_proc(n_code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if INJECTING.load(Ordering::SeqCst) {
        return unsafe { CallNextHookEx(None, n_code, w_param, l_param) };
    }

    if n_code >= 0 && w_param.0 as u32 == WM_MOUSEWHEEL {
        let raw_delta = unsafe {
            let hook_struct = &*(l_param.0 as *const MSLLHOOKSTRUCT);
            (hook_struct.mouseData >> 16) as i16 as i32
        };

        if raw_delta != 0 {
            if let Ok(mut state) = HOOK_STATE.lock() {
                if !state.enabled {
                    return unsafe { CallNextHookEx(None, n_code, w_param, l_param) };
                }

                if let Some(target) = TargetWindow::foreground() {
                    let profile = find_profile(&target.process_name);

                    if let Some(p) = profile {
                        if p.enabled {
                            state.current_process = p.name.clone();
                            state.injector.set_config(config::current_config());
                            state.injector.feed_wheel(raw_delta);
                            debug_log!(
                                "Hook -> {} (delta={}, V={:.0})",
                                p.name,
                                raw_delta,
                                state.injector.velocity()
                            );
                            return LRESULT(1);
                        } else {
                            debug_log!("{} matched but DISABLED", target.process_name);
                        }
                    } else {
                        debug_log!("No profile for: {}", target.process_name);
                    }
                }
            }
        }
    }

    unsafe { CallNextHookEx(None, n_code, w_param, l_param) }
}

pub fn install_hook() -> Result<(), windows::core::Error> {
    unsafe {
        let hmod: HINSTANCE = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)?.into();
        let hook = SetWindowsHookExW(WH_MOUSE_LL, Some(low_level_mouse_proc), hmod, 0)?;
        if let Ok(mut guard) = HOOK_HANDLE.lock() {
            *guard = Some(hook.0 as isize);
        }
    }

    Ok(())
}

#[allow(dead_code)]
pub fn uninstall_hook() {
    if let Ok(mut guard) = HOOK_HANDLE.lock() {
        if let Some(raw) = guard.take() {
            unsafe {
                let _ = UnhookWindowsHookEx(HHOOK(raw as *mut _));
            }
            eprintln!("[ZenScroll] Hook uninstalled");
        }
    }
}

#[allow(dead_code)]
pub fn run_message_pump() {
    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = windows::Win32::UI::WindowsAndMessaging::TranslateMessage(&msg);
            let _ = windows::Win32::UI::WindowsAndMessaging::DispatchMessageW(&msg);
        }
    }
}

#[allow(dead_code)]
pub fn set_enabled(enabled: bool) {
    if let Ok(mut state) = HOOK_STATE.lock() {
        state.enabled = enabled;
    }
}

#[allow(dead_code)]
pub fn get_state() -> (bool, String) {
    if let Ok(state) = HOOK_STATE.lock() {
        (state.enabled, state.current_process.clone())
    } else {
        (true, String::new())
    }
}
