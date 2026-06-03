use crate::debug_log;
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

pub struct TargetWindow {
    #[allow(dead_code)]
    pub hwnd: isize,
    pub process_name: String,
}

impl TargetWindow {
    pub fn foreground() -> Option<Self> {
        // SAFETY: GetForegroundWindow retrieves the handle to the foreground window. Returns invalid handle if none.
        let hwnd = unsafe { GetForegroundWindow() };

        if hwnd.is_invalid() {
            debug_log!("检测: GetForegroundWindow 无效");
            return None;
        }
        debug_log!("检测: 前台窗口句柄 = {:?}", hwnd);

        let mut pid: u32 = 0;
        // SAFETY: hwnd is from GetForegroundWindow. GetWindowThreadProcessId writes the PID into pid.
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)); }
        if pid == 0 {
            debug_log!("检测: GetWindowThreadProcessId = 0");
            return None;
        }
        debug_log!("检测: PID={}", pid);

        let process_name = get_process_name(pid).unwrap_or_else(|| {
            debug_log!("检测: get_process_name({}) 失败", pid);
            "unknown".into()
        });
        debug_log!("检测: 进程名='{}'", process_name);

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

    debug_log!("检测: 调用 OpenProcess({})", pid);
    // SAFETY: OpenProcess with PROCESS_QUERY_LIMITED_INFORMATION opens a handle for querying the process name.
    let handle = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
        Ok(h) => h,
        Err(e) => {
            debug_log!("检测: OpenProcess({}) 失败: {:?}", pid, e);
            return None;
        }
    };
    debug_log!("检测: OpenProcess 成功");

    let mut buf = vec![0u16; 4096];
    let mut size = buf.len() as u32;

    debug_log!("检测: 调用 QueryFullProcessImageNameW(..., WIN32, ...)");
    // SAFETY: handle is from OpenProcess. buf has 4096 elements which is sufficient for a Win32 path.
    // PROCESS_NAME_FORMAT(0) = WIN32 format. size is in/out parameter populated by the kernel.
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
        debug_log!("检测: 完整路径 = '{}'", name);
        let exe = std::path::Path::new(&name)
            .file_name()?
            .to_string_lossy()
            .into_owned();
        debug_log!("检测: exe = '{}'", exe);
        Some(exe)
    } else {
        // SAFETY: GetLastError retrieves the calling thread's last-error code after QueryFullProcessImageNameW failed.
        let err = unsafe { windows::Win32::Foundation::GetLastError() };
        debug_log!("检测: QueryFullProcessImageNameW 失败, GetLastError={}", err.0);
        None
    }
}
