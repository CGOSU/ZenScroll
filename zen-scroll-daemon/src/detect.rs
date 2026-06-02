use crate::debug_log;
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

pub struct TargetWindow {
    #[allow(dead_code)]
    pub hwnd: isize,
    pub process_name: String,
}

impl TargetWindow {
    pub fn foreground() -> Option<Self> {
        let hwnd = unsafe { GetForegroundWindow() };

        if hwnd.is_invalid() {
            debug_log!("detect: GetForegroundWindow = INVALID");
            return None;
        }
        debug_log!("detect: foreground HWND = {:?}", hwnd);

        let mut pid: u32 = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
        }
        if pid == 0 {
            debug_log!("detect: GetWindowThreadProcessId = 0");
            return None;
        }
        debug_log!("detect: PID={}", pid);

        let process_name = get_process_name(pid).unwrap_or_else(|| {
            debug_log!("detect: get_process_name({}) FAILED", pid);
            "unknown".into()
        });
        debug_log!("detect: process_name='{}'", process_name);

        Some(Self {
            hwnd: hwnd.0 as isize,
            process_name,
        })
    }
}

fn get_process_name(pid: u32) -> Option<String> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    debug_log!("detect: calling OpenProcess({})", pid);
    let handle = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
        Ok(h) => h,
        Err(e) => {
            debug_log!("detect: OpenProcess({}) failed: {:?}", pid, e);
            return None;
        }
    };
    debug_log!("detect: OpenProcess OK");

    let mut buf = vec![0u16; 4096];
    let mut size = buf.len() as u32;

    debug_log!("detect: calling QueryFullProcessImageNameW(..., WIN32, ...)");
    let result = unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_FORMAT(0),
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut size,
        )
    };

    if result.is_ok() {
        let name = OsString::from_wide(&buf[..size as usize])
            .to_string_lossy()
            .into_owned();
        debug_log!("detect: full path = '{}'", name);
        let exe = std::path::Path::new(&name)
            .file_name()?
            .to_string_lossy()
            .into_owned();
        debug_log!("detect: exe = '{}'", exe);
        Some(exe)
    } else {
        let err = unsafe { windows::Win32::Foundation::GetLastError() };
        debug_log!("detect: QueryFullProcessImageNameW failed, GetLastError={}", err.0);
        None
    }
}
