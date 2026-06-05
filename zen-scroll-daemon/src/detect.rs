use crate::debug_log;
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, GetAncestor, GetClassNameW, GetForegroundWindow, GetParent, GetWindowRect,
    GetWindowThreadProcessId, IsWindow, WindowFromPoint, GA_ROOT,
};

const LISTVIEW_CLASS: &str = "SysListView32";

pub struct TargetWindow {
    pub process_name: String,
    pub target_hwnd: isize,
    pub listview_hwnd: isize,
    pub chunked_wheel: bool,
    pub cursor_pt: (i32, i32),
}

impl TargetWindow {
    /// 主入口：GetForegroundWindow 鉴定进程名（稳定），WindowFromPoint 获取光标下窗口信息
    pub fn from_hook(pt: POINT) -> Option<Self> {
        // SAFETY: GetForegroundWindow retrieves the foreground window handle.
        let fg = unsafe { GetForegroundWindow() };
        if fg.is_invalid() {
            debug_log!("定位: GetForegroundWindow 无效");
            return None;
        }

        let mut pid: u32 = 0;
        // SAFETY: GetWindowThreadProcessId writes PID for the foreground window.
        unsafe { GetWindowThreadProcessId(fg, Some(&mut pid)); }
        if pid == 0 {
            debug_log!("定位: GetWindowThreadProcessId = 0");
            return None;
        }

        let process_name = get_process_name(pid)?;

        // 光标下的窗口信息用于 ListView + 目标跟踪
        // SAFETY: WindowFromPoint retrieves the window under the cursor.
        let cursor_hwnd = unsafe { WindowFromPoint(pt) };
        let cursor_hwnd = if cursor_hwnd.is_invalid() { fg } else { cursor_hwnd };

        // SAFETY: GetAncestor(GA_ROOT) gets the root parent for window class checks.
        let root = match unsafe { GetAncestor(cursor_hwnd, GA_ROOT) } {
            r if r.is_invalid() => cursor_hwnd,
            r => r,
        };

        let listview = find_listview_under_cursor(cursor_hwnd, root, pt);
        let chunked = needs_chunked_wheel(root);

        debug_log!(
            "定位: 进程='{}', 前台={:?}, 光标句柄={:?}, 列表视图={}",
            process_name,
            fg.0,
            cursor_hwnd.0,
            if listview != 0 { "是" } else { "否" }
        );

        Some(Self {
            process_name,
            target_hwnd: root.0 as isize,
            listview_hwnd: listview,
            chunked_wheel: chunked,
            cursor_pt: (pt.x, pt.y),
        })
    }
}

fn class_name_is(hwnd: HWND, name: &str) -> bool {
    let mut buf = [0u16; 96];
    let len = unsafe { GetClassNameW(hwnd, &mut buf) };
    if len == 0 {
        return false;
    }
    String::from_utf16_lossy(&buf[..len as usize]) == name
}

fn needs_chunked_wheel(hwnd: HWND) -> bool {
    let root = match unsafe { GetAncestor(hwnd, GA_ROOT) } {
        r if r.is_invalid() => hwnd,
        r => r,
    };
    if class_name_is(root, "CabinetWClass") || class_name_is(root, "ExploreWClass") {
        return true;
    }
    if class_name_is(root, "TaskManagerWindow") || class_name_is(root, "TaskManagerWindowClass") {
        return true;
    }
    let mut pid: u32 = 0;
    unsafe { GetWindowThreadProcessId(root, Some(&mut pid)); }
    if pid != 0
        && let Some(name) = get_process_name(pid)
        && name.eq_ignore_ascii_case("Taskmgr.exe")
    {
        return true;
    }
    false
}

fn find_listview_under_cursor(mut hwnd: HWND, root: HWND, pt: POINT) -> isize {
    for _ in 0..5 {
        if hwnd.is_invalid() {
            break;
        }
        if class_name_is(hwnd, LISTVIEW_CLASS) {
            return hwnd.0 as isize;
        }
        if let Ok(parent) = unsafe { GetParent(hwnd) } {
            if parent.is_invalid() || parent == hwnd {
                break;
            }
            hwnd = parent;
        } else {
            break;
        }
    }

    let mut child: HWND = HWND::default();
    loop {
        let next = match unsafe { FindWindowExW(root, child, windows::core::w!("SysListView32"), None) } {
            Ok(h) => h,
            Err(_) => break,
        };
        if next.is_invalid() || next.0.is_null() || next == child {
            break;
        }
        child = next;
        let mut rc = RECT::default();
        if unsafe { GetWindowRect(child, &mut rc) }.is_ok()
            && pt.x >= rc.left && pt.x < rc.right && pt.y >= rc.top && pt.y < rc.bottom
            && unsafe { IsWindow(child).as_bool() }
        {
            return child.0 as isize;
        }
    }

    0
}

pub fn listview_is_valid(hwnd: isize) -> bool {
    if hwnd == 0 {
        return false;
    }
    unsafe { IsWindow(HWND(hwnd as *mut _)).as_bool() }
}

fn get_process_name(pid: u32) -> Option<String> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    let handle = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
        Ok(h) => h,
        Err(e) => {
            debug_log!("检测: OpenProcess({}) 失败: {:?}", pid, e);
            return None;
        }
    };

    let mut buf = vec![0u16; 4096];
    let mut size = buf.len() as u32;

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
        let exe = std::path::Path::new(&name)
            .file_name()?
            .to_string_lossy()
            .into_owned();
        Some(exe)
    } else {
        let err = unsafe { windows::Win32::Foundation::GetLastError() };
        debug_log!("检测: QueryFullProcessImageNameW 失败, GetLastError={}", err.0);
        None
    }
}
