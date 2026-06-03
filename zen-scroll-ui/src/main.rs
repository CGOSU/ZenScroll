#![windows_subsystem = "windows"]
use gpui::*;
use std::env;
use std::fs;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

#[repr(C)]
struct HWND(isize);

const WM_APP: u32 = 0x8000;

unsafe extern "system" {
    fn FindWindowW(lpClassName: *const u16, lpWindowName: *const u16) -> HWND;
    fn PostMessageW(hWnd: HWND, Msg: u32, wParam: usize, lParam: isize) -> i32;
    fn ShellExecuteW(
        hwnd: *mut std::ffi::c_void,
        lpOperation: *const u16,
        lpFile: *const u16,
        lpParameters: *const u16,
        lpDirectory: *const u16,
        nShowCmd: i32,
    ) -> isize;
}

fn daemon_class() -> Vec<u16> {
    let mut s: Vec<u16> = "ZenScrollTray".encode_utf16().collect();
    s.push(0);
    s
}

fn find_daemon() -> HWND {
    let class = daemon_class();
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
        "debug": false
    })
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
        unsafe {
            PostMessageW(hwnd, WM_APP, 0, 0);
        }
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
}

impl ConfigPanel {
    fn new() -> Self {
        let cfg = read_config();
        Self {
            selected: cfg["speed_preset"].as_u64().unwrap_or(1) as usize,
            enabled: cfg["enabled"].as_bool().unwrap_or(true),
        }
    }

    fn save_and_signal(&self) {
        let mut cfg = read_config();
        cfg["speed_preset"] = serde_json::json!(self.selected);
        cfg["enabled"] = serde_json::json!(self.enabled);
        write_config(&cfg);
        signal_daemon();
    }
}

impl Render for ConfigPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(0x1a1a2e))
            .flex()
            .flex_col()
            .child(self.header())
            .child(self.preset_picker(cx))
            .child(self.toggle_row(cx))
            .child(self.status_bar())
    }
}

impl ConfigPanel {
    fn header(&self) -> impl IntoElement {
        div()
            .h(px(48.0))
            .bg(rgb(0x16213e))
            .flex()
            .items_center()
            .justify_between()
            .px_4()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .w(px(10.0))
                            .h(px(10.0))
                            .rounded_full()
                            .bg(if self.enabled {
                                rgb(0x44ff88)
                            } else {
                                rgb(0x555555)
                            }),
                    )
                    .child(
                        div()
                            .text_color(rgb(0x88ccff))
                            .font_weight(FontWeight(700.0))
                            .text_lg()
                            .child("ZenScroll"),
                    ),
            )
            .child(
                div()
                    .text_color(if self.enabled {
                        rgb(0x44ff88)
                    } else {
                        rgb(0x555555)
                    })
                    .text_sm()
                    .child(if self.enabled {
                        format!("{} · 运行中", PRESET_NAMES[self.selected])
                    } else {
                        "已停止".into()
                    }),
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
            .children((0..3).map(|i| {
                let is_sel = i == self.selected;
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
                        cx.listener(
                            move |this: &mut Self,
                                  _: &MouseDownEvent,
                                  _: &mut Window,
                                  cx: &mut Context<Self>| {
                                this.selected = i;
                                this.save_and_signal();
                                cx.notify();
                            },
                        ),
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
            }))
    }

    fn toggle_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .h(px(44.0))
            .flex()
            .items_center()
            .justify_center()
            .gap_3()
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(
                    |this: &mut Self,
                     _: &MouseDownEvent,
                     _: &mut Window,
                     cx: &mut Context<Self>| {
                        this.enabled = !this.enabled;
                        this.save_and_signal();
                        cx.notify();
                    },
                ),
            )
            .child(
                div()
                    .w(px(44.0))
                    .h(px(24.0))
                    .bg(if self.enabled {
                        rgb(0x44ff88)
                    } else {
                        rgb(0x444444)
                    })
                    .rounded_full()
                    .relative()
                    .child(
                        div()
                            .w(px(20.0))
                            .h(px(20.0))
                            .rounded_full()
                            .bg(rgb(0xffffff))
                            .absolute()
                            .left(if self.enabled { px(22.0) } else { px(2.0) })
                            .top(px(2.0)),
                    ),
            )
            .child(
                div()
                    .text_color(if self.enabled {
                        rgb(0x44ff88)
                    } else {
                        rgb(0x888888)
                    })
                    .text_sm()
                    .child(if self.enabled {
                        "已启用"
                    } else {
                        "已禁用"
                    }),
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
                c.friction,
                c.smartwheel_friction_max,
                c.bounce_tension,
                c.scroll_accel,
                c.max_velocity,
                c.min_velocity
            )))
    }
}

fn main() {
    ensure_daemon_running();
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(320.0), px(420.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| ConfigPanel::new()),
        )
        .unwrap();
        cx.activate(true);
    });
}

