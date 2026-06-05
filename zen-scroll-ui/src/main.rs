#![windows_subsystem = "windows"]
use gpui::*;
use std::env;
use std::fs;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

const EVENT_NAME: &str = "ZenScrollConfigChange";

#[repr(C)]
#[derive(Clone, Copy)]
#[allow(clippy::upper_case_acronyms)]
struct HWND(isize);

const WM_APP: u32 = 0x8000;

unsafe extern "system" {
    fn FindWindowW(lpClassName: *const u16, lpWindowName: *const u16) -> HWND;
    fn PostMessageW(hWnd: HWND, Msg: u32, wParam: usize, lParam: isize) -> i32;
    fn GetActiveWindow() -> HWND;
    fn GetModuleHandleW(lpModuleName: *const u16) -> isize;
    fn LoadImageW(
        hInst: isize,
        name: *const u16,
        type_: u32,
        cx: i32,
        cy: i32,
        fuLoad: u32,
    ) -> isize;
    fn ShellExecuteW(
        hwnd: *mut std::ffi::c_void,
        lpOperation: *const u16,
        lpFile: *const u16,
        lpParameters: *const u16,
        lpDirectory: *const u16,
        nShowCmd: i32,
    ) -> isize;
    fn CreateEventW(
        lpEventAttributes: *mut std::ffi::c_void,
        bManualReset: i32,
        bInitialState: i32,
        lpName: *const u16,
    ) -> isize;
    fn WaitForSingleObject(hHandle: isize, dwMilliseconds: u32) -> u32;
    fn ResetEvent(hEvent: isize) -> i32;
}

fn daemon_class() -> Vec<u16> {
    let mut s: Vec<u16> = "ZenScrollTray".encode_utf16().collect();
    s.push(0);
    s
}

fn find_daemon() -> HWND {
    let class = daemon_class();
    // SAFETY: FindWindowW searches for the "ZenScrollTray" window class created by the daemon.
    unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) }
}

fn daemon_is_running() -> bool {
    find_daemon().0 != 0
}

fn launch_daemon() {
    let exe = env::current_exe().ok();
    let daemon_path = exe
        .as_ref()
        .and_then(|p| p.parent())
        .map(|dir| dir.join("zen-scroll-daemon.exe"));
    let Some(path) = daemon_path else { return };
    if !path.exists() {
        return;
    }

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let verb: Vec<u16> = "runas\0".encode_utf16().collect();

    // SAFETY: ShellExecuteW("runas") launches zen-scroll-daemon.exe with admin privileges.
    unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            wide.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            5,
        );
    }

    for _ in 0..20 {
        thread::sleep(Duration::from_millis(200));
        if daemon_is_running() {
            thread::sleep(Duration::from_millis(500));
            break;
        }
    }
}

fn ensure_daemon_running() {
    if !daemon_is_running() {
        launch_daemon();
    }
    if daemon_is_running() {
        signal_daemon();
    }
}

fn config_path() -> PathBuf {
    let base = env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("ZenScroll").join("config.json")
}

fn config_mtime() -> Option<std::time::SystemTime> {
    fs::metadata(config_path()).ok().and_then(|m| m.modified().ok())
}

fn read_config() -> serde_json::Value {
    match fs::read_to_string(config_path()) {
        Ok(s) => serde_json::from_str(&s).unwrap_or(default_config()),
        Err(_) => default_config(),
    }
}

fn default_config() -> serde_json::Value {
    serde_json::json!({
        "enabled": true,
        "speed_preset": 1,
        "custom_profiles": [],
        "debug": false,
        "autostart": false
    })
}

static CONFIG_DIRTY: AtomicBool = AtomicBool::new(false);

fn start_config_watcher() {
    let name: Vec<u16> = EVENT_NAME.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: CreateEventW creates a named manual-reset event for cross-process signaling.
    // bManualReset=TRUE (1) so multiple waits can see the signal.
    let ev = unsafe { CreateEventW(std::ptr::null_mut(), 1, 0, name.as_ptr()) };
    if ev == 0 || ev == -1_isize {
        // 回退到轮询
        thread::spawn(|| {
            let mut last = config_mtime();
            loop {
                thread::sleep(Duration::from_millis(200));
                let mtime = config_mtime();
                if mtime.is_some() && mtime != last {
                    last = mtime;
                    CONFIG_DIRTY.store(true, Ordering::SeqCst);
                }
            }
        });
        return;
    }
    thread::spawn(move || {
        loop {
            // SAFETY: WaitForSingleObject blocks until the event is signaled (daemon writes config).
            unsafe { WaitForSingleObject(ev, u32::MAX); }
            CONFIG_DIRTY.store(true, Ordering::SeqCst);
            // SAFETY: ResetEvent sets the manual-reset event back to non-signaled.
            unsafe { ResetEvent(ev); }
        }
    });
}

fn write_config(v: &serde_json::Value) {
    let path = config_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(s) = serde_json::to_string_pretty(v) {
        let _ = fs::write(&path, &s);
    }
}

fn signal_daemon() {
    let hwnd = find_daemon();
    if hwnd.0 != 0 {
        // SAFETY: PostMessageW sends WM_APP to the daemon tray window, which triggers config reload.
        unsafe { PostMessageW(hwnd, WM_APP, 0, 0); }
    }
}

fn set_window_icon() {
    // SAFETY: GetActiveWindow returns the active popup/overlapped window handle.
    let hwnd = unsafe { GetActiveWindow() };
    if hwnd.0 == 0 { return; }
    // SAFETY: GetModuleHandleW(null) gets the current process module handle.
    let hmod = unsafe { GetModuleHandleW(std::ptr::null()) };
    // SAFETY: LoadImageW with RT_ICON type loads the embedded icon resource (ID=1) at 32x32.
    let icon = unsafe { LoadImageW(hmod, std::ptr::from_ref(&1u16).cast(), 1, 32, 32, 0) };
    if icon != 0 {
        // SAFETY: PostMessageW with WM_SETICON sets the window icon for small (0) and big (1) sizes.
        unsafe { PostMessageW(hwnd, 0x0080, 0, icon); }
        unsafe { PostMessageW(hwnd, 0x0080, 1, icon); }
    }
}
const PRESET_NAMES: [&str; 3] = ["慢", "正常", "快"];
const PRESET_LABELS: [&str; 3] = [
    "精准·适合阅读/代码",
    "均衡·日常浏览首选",
    "激进·长文档快速定位",
];

struct ConfigPanel {
    selected: usize,
    enabled: bool,
    debug: bool,
    autostart: bool,
}

impl ConfigPanel {
    fn new() -> Self {
        let cfg = read_config();
        Self {
            selected: cfg["speed_preset"].as_u64().unwrap_or(1) as usize,
            enabled: cfg["enabled"].as_bool().unwrap_or(true),
            debug: cfg["debug"].as_bool().unwrap_or(false),
            autostart: cfg["autostart"].as_bool().unwrap_or(false),
        }
    }

    /// 检查 CONFIG_DIRTY 标记，仅文件变更时才解析 JSON
    fn sync_from_config(&mut self) -> bool {
        if !CONFIG_DIRTY.swap(false, Ordering::SeqCst) {
            return false;
        }
        let cfg = read_config();
        let new_enabled = cfg["enabled"].as_bool().unwrap_or(true);
        let new_selected = cfg["speed_preset"].as_u64().unwrap_or(1) as usize;
        let new_debug = cfg["debug"].as_bool().unwrap_or(false);
        let new_autostart = cfg["autostart"].as_bool().unwrap_or(false);
        if new_enabled != self.enabled
            || new_selected != self.selected
            || new_debug != self.debug
            || new_autostart != self.autostart
        {
            self.enabled = new_enabled;
            self.selected = new_selected;
            self.debug = new_debug;
            self.autostart = new_autostart;
            return true;
        }
        false
    }

    fn save_and_signal(&mut self) {
        let mut cfg = read_config();
        cfg["speed_preset"] = serde_json::json!(self.selected);
        cfg["enabled"] = serde_json::json!(self.enabled);
        cfg["debug"] = serde_json::json!(self.debug);
        cfg["autostart"] = serde_json::json!(self.autostart);
        write_config(&cfg);
        signal_daemon();
    }
}

impl Render for ConfigPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 轮询 config.json 同步外部状态（托盘、其他 UI 实例）
        if self.sync_from_config() {
            cx.notify();
        }
        div()
            .size_full()
            .bg(rgb(0x1a1a2e))
            .flex()
            .flex_col()
            .child(self.header())
            .child(self.preset_picker(cx))
            .child(self.toggle_row(cx))
            .child(self.debug_row(cx))
            .child(self.autostart_row(cx))
            .child(self.status_bar())
    }
}

fn status_color(enabled: bool) -> Rgba {
    if enabled { rgb(0x44ff88) } else { rgb(0x555555) }
}

fn indicator_dot(enabled: bool) -> Div {
    div()
        .w(px(10.0))
        .h(px(10.0))
        .rounded_full()
        .bg(status_color(enabled))
}

fn preset_card(
    i: usize,
    is_sel: bool,
    cx: &mut Context<ConfigPanel>,
) -> Div {
    div()
        .w_full()
        .h(px(72.0))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_1()
        .bg(if is_sel { rgb(0x0f3460) } else { rgb(0x0f1a2e) })
        .border_1()
        .border_color(if is_sel { rgb(0x88ccff) } else { rgb(0x2a2a4a) })
        .rounded(px(6.0))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(0x0f3460)))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this: &mut ConfigPanel, _: &MouseDownEvent, _: &mut Window, cx: &mut Context<ConfigPanel>| {
                this.selected = i;
                this.save_and_signal();
                cx.notify();
            }),
        )
        .child(
            div()
                .text_color(if is_sel { rgb(0x88ccff) } else { rgb(0x666688) })
                .text_2xl()
                .font_weight(FontWeight(700.0))
                .child(PRESET_NAMES[i]),
        )
        .child(
            div()
                .text_color(if is_sel { rgb(0x6688aa) } else { rgb(0x555566) })
                .text_xs()
                .child(PRESET_LABELS[i]),
        )
}

fn toggle_knob(enabled: bool) -> Div {
    div()
        .w(px(20.0))
        .h(px(20.0))
        .rounded_full()
        .bg(rgb(0xffffff))
        .absolute()
        .left(if enabled { px(22.0) } else { px(2.0) })
        .top(px(2.0))
}

impl ConfigPanel {
    fn header(&self) -> impl IntoElement {
        let color = status_color(self.enabled);
        let status_text = if self.enabled {
            format!("{} · 运行中", PRESET_NAMES[self.selected])
        } else {
            "已停止".into()
        };

        div()
            .h(px(48.0))
            .bg(rgb(0x16213e))
            .flex()
            .items_center()
            .justify_between()
            .px_4()
            .child(
                div().flex().items_center().gap_2()
                    .child(indicator_dot(self.enabled))
                    .child(
                        div().text_color(rgb(0x88ccff)).font_weight(FontWeight(700.0)).text_lg()
                            .child("ZenScroll"),
                    ),
            )
            .child(
                div().text_color(color).text_sm().child(status_text),
            )
    }

    fn preset_picker(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_4()
            .px_8()
            .children((0..3).map(|i| preset_card(i, i == self.selected, cx)))
    }

    fn toggle_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let toggle_bg = if self.enabled { rgb(0x44ff88) } else { rgb(0x444444) };
        let label_color = if self.enabled { rgb(0x44ff88) } else { rgb(0x888888) };
        let label = if self.enabled { "已启用" } else { "已禁用" };

        div()
            .h(px(44.0))
            .flex()
            .items_center()
            .justify_center()
            .gap_3()
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this: &mut Self, _: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>| {
                    this.enabled = !this.enabled;
                    this.save_and_signal();
                    cx.notify();
                }),
            )
            .child(
                div().w(px(44.0)).h(px(24.0)).bg(toggle_bg).rounded_full().relative()
                    .child(toggle_knob(self.enabled)),
            )
            .child(
                div().text_color(label_color).text_sm().child(label),
            )
    }

    fn debug_row(&self, cx: &mut Context<Self>) -> impl IntoElement {        let toggle_bg = if self.debug { rgb(0x6688cc) } else { rgb(0x333344) };
        let label_color = if self.debug { rgb(0x88aadd) } else { rgb(0x666677) };
        let label = if self.debug { "调试日志：开" } else { "调试日志：关" };

        div()
            .h(px(32.0))
            .flex()
            .items_center()
            .justify_center()
            .gap_2()
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this: &mut Self, _: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>| {
                    this.debug = !this.debug;
                    this.save_and_signal();
                    cx.notify();
                }),
            )
            .child(
                div().w(px(28.0)).h(px(16.0)).bg(toggle_bg).rounded_full().relative()
                    .child(
                        div()
                            .w(px(12.0)).h(px(12.0))
                            .rounded_full()
                            .bg(rgb(0xffffff))
                            .absolute()
                            .left(if self.debug { px(14.0) } else { px(2.0) })
                            .top(px(2.0)),
                    ),
            )
            .child(
                div().text_color(label_color).text_xs().child(label),
            )
    }

    fn autostart_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let toggle_bg = if self.autostart { rgb(0x44ff88) } else { rgb(0x444444) };
        let label_color = if self.autostart { rgb(0x44ff88) } else { rgb(0x888888) };
        let label = if self.autostart { "开机自启：开" } else { "开机自启：关" };

        div()
            .h(px(32.0))
            .flex()
            .items_center()
            .justify_center()
            .gap_2()
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this: &mut Self, _: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>| {
                    this.autostart = !this.autostart;
                    this.save_and_signal();
                    cx.notify();
                }),
            )
            .child(
                div().w(px(28.0)).h(px(16.0)).bg(toggle_bg).rounded_full().relative()
                    .child(
                        div()
                            .w(px(12.0)).h(px(12.0))
                            .rounded_full()
                            .bg(rgb(0xffffff))
                            .absolute()
                            .left(if self.autostart { px(14.0) } else { px(2.0) })
                            .top(px(2.0)),
                    ),
            )
            .child(
                div().text_color(label_color).text_xs().child(label),
            )
    }

    fn status_bar(&self) -> impl IntoElement {
        let c = &zen_scroll_core::physics::PRESETS[self.selected];
        div()
            .h(px(28.0))
            .bg(rgb(0x0f1520))
            .flex()
            .items_center()
            .justify_center()
            .px_4()
            .child(div().text_color(rgb(0x555577)).text_xs().child(format!(
                "μ={:.2} sm={:.3} ξ={:.2} a={:.1} V={:.0} v={:.2}",
                c.friction, c.smartwheel_friction_max, c.bounce_tension,
                c.scroll_accel, c.max_velocity, c.min_velocity
            )))
    }
}

fn main() {
    start_config_watcher();
    ensure_daemon_running();
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(320.0), px(486.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some(format!("ZenScroll v{}", env!("CARGO_PKG_VERSION")).into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_, cx| cx.new(|_| ConfigPanel::new()),
        )
        .unwrap();
        set_window_icon();
        cx.activate(true);
    });
}





