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
        // SAFETY: CallNextHookEx passes the event to the next hook in the chain. Required by WH_MOUSE_LL.
        return unsafe { CallNextHookEx(None, n_code, w_param, l_param) };
    }

    if n_code >= 0 && w_param.0 as u32 == WM_MOUSEWHEEL {
        // SAFETY: l_param points to a valid MSLLHOOKSTRUCT when n_code >= 0 and message is WM_MOUSEWHEEL,
        // as documented by the WH_MOUSE_LL hook specification.
        let raw_delta = unsafe {
            let hook_struct = &*(l_param.0 as *const MSLLHOOKSTRUCT);
            (hook_struct.mouseData >> 16) as i16 as i32
        };

        if raw_delta != 0
            && let Ok(mut state) = HOOK_STATE.lock()
        {
            if !state.enabled {
                // SAFETY: CallNextHookEx passes the event through when the daemon is disabled.
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
                            "钩子 -> {} (增量={}, 速度={:.0})",
                            p.name,
                            raw_delta,
                            state.injector.velocity()
                        );
                        return LRESULT(1);
                    } else {
                        debug_log!("{} 已匹配但已禁用", target.process_name);
                    }
                } else {
                    debug_log!("无匹配配置: {}", target.process_name);
                }
            }
        }
    }

    // SAFETY: CallNextHookEx passes unhandled mouse events to the next hook in the chain.
    unsafe { CallNextHookEx(None, n_code, w_param, l_param) }
}

pub fn install_hook() -> Result<(), windows::core::Error> {
    // SAFETY: GetModuleHandleW(null) retrieves the module handle for the current process.
    let hmod_raw = unsafe { windows::Win32::System::LibraryLoader::GetModuleHandleW(None)? };
    let hmod: HINSTANCE = hmod_raw.into();
    // SAFETY: SetWindowsHookExW installs the WH_MOUSE_LL hook with the current module's hmod.
    let hook = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(low_level_mouse_proc), hmod, 0)? };
    if let Ok(mut guard) = HOOK_HANDLE.lock() {
        *guard = Some(hook.0 as isize);
    }

    Ok(())
}

#[allow(dead_code)]
pub fn uninstall_hook() {
    if let Ok(mut guard) = HOOK_HANDLE.lock()
        && let Some(raw) = guard.take()
    {
        // SAFETY: raw is a valid HHOOK handle stored by install_hook. UnhookWindowsHookEx removes the hook.
        unsafe { let _ = UnhookWindowsHookEx(HHOOK(raw as *mut _)); }
        eprintln!("[ZenScroll] 钩子已卸载");
    }
}

#[allow(dead_code)]
pub fn run_message_pump() {
    let mut msg = MSG::default();
    // SAFETY: Standard Windows message pump. GetMessageW blocks until a message arrives;
    // TranslateMessage/DispatchMessageW process and dispatch it to the window procedure.
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = windows::Win32::UI::WindowsAndMessaging::TranslateMessage(&msg);
            windows::Win32::UI::WindowsAndMessaging::DispatchMessageW(&msg);
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
