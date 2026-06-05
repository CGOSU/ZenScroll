#![windows_subsystem = "windows"]

use gpui::*;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use std::thread;
use std::time::Duration;

use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HWND};
use windows::Win32::System::Console::{AllocConsole, GetConsoleWindow};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, SetForegroundWindow, SetProcessDPIAware, ShowWindow,
    SPI_GETWHEELSCROLLLINES, SPI_SETWHEELSCROLLLINES, SPIF_UPDATEINIFILE, SystemParametersInfoW,
    SW_HIDE, SW_SHOW,
};
use windows::core::PCWSTR;

mod config;
mod detect;
mod hook;
mod log;
mod profile;
mod smoother;
mod tray;

const TICK_INTERVAL: Duration = Duration::from_millis(4);
const WHEEL_PAGESCROLL: u32 = 0;

static ORIGINAL_SCROLL_LINES: AtomicU32 = AtomicU32::new(3);
static GPUI_HWND: AtomicIsize = AtomicIsize::new(0);

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
            let _ = SystemParametersInfoW(SPI_SETWHEELSCROLLLINES, 0, Some(ptr), SPIF_UPDATEINIFILE);
        }
        eprintln!("[ZenScroll] 已恢复系统滚轮为'翻页'模式");
    }
}

fn sync_console_state() {
    // SAFETY: GetConsoleWindow retrieves the console window handle (null if none).
    let console = unsafe { GetConsoleWindow() };
    if console.0.is_null() {
        if log::DEBUG_ENABLED.load(Ordering::Relaxed) {
            // SAFETY: AllocConsole creates a new console for debug output.
            unsafe { let _ = AllocConsole(); }
        }
    } else {
        // SAFETY: ShowWindow shows or hides the console based on debug state.
        unsafe {
            let _ = ShowWindow(console, if log::DEBUG_ENABLED.load(Ordering::Relaxed) { SW_SHOW } else { SW_HIDE });
        }
    }
}

fn injection_loop() {
    loop {
        if tray::should_exit() {
            break;
        }
        if let Ok(mut state) = hook::HOOK_STATE.lock() {
            state.injector.tick();
        }
        thread::sleep(TICK_INTERVAL);
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
        if let Ok(guard) = config::DAEMON_CONFIG.lock() {
            Self {
                selected: guard.speed_preset,
                enabled: guard.enabled,
                debug: guard.debug,
                autostart: guard.autostart,
            }
        } else {
            Self {
                selected: 1,
                enabled: true,
                debug: false,
                autostart: false,
            }
        }
    }

    /// 直接从 DAEMON_CONFIG 内存同步，无需文件或 IPC
    fn sync_from_config(&mut self) -> bool {
        if let Ok(guard) = config::DAEMON_CONFIG.lock() {
            let new_enabled = guard.enabled;
            let new_selected = guard.speed_preset;
            let new_debug = guard.debug;
            let new_autostart = guard.autostart;
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
        }
        false
    }

    fn save_and_signal(&mut self) {
        let cfg = {
            let mut guard = config::DAEMON_CONFIG.lock().unwrap();
            guard.enabled = self.enabled;
            guard.speed_preset = self.selected;
            guard.debug = self.debug;
            guard.autostart = self.autostart;
            guard.clone()
        };
        config::save(&cfg);
        // 同步 HOOK_STATE 使切换立即生效
        if let Ok(mut state) = hook::HOOK_STATE.lock() {
            state.enabled = cfg.enabled;
        }
        // 同步托盘提示文字
        tray::sync_tip();
        log::set_debug(cfg.debug);
        sync_console_state();
        config::sync_autostart(cfg.autostart);
    }
}

impl Render for ConfigPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 每帧从共享内存同步外部状态（托盘点击引起的变更）
        if self.sync_from_config() {
            cx.notify();
        }
        cx.notify(); // 持续请求下一帧，确保托盘变更尽快同步到 UI
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

    fn debug_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let toggle_bg = if self.debug { rgb(0x6688cc) } else { rgb(0x333344) };
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
    // 单例检查：命名互斥体防止重复启动
    let singleton_name: Vec<u16> = "ZenScroll_SingletonMutex\0".encode_utf16().collect();
    // SAFETY: CreateMutexW creates/opens a named mutex. GetLastError immediately after distinguishes
    // first creation from existing-object case. HANDLE is Copy and has no Drop — `let _ = handle;`
    // is safe and keeps the handle open (kernel keeps the object alive until process exit).
    if let Ok(mutex) = unsafe {
        CreateMutexW(None, false, PCWSTR::from_raw(singleton_name.as_ptr()))
    } {
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            let title: Vec<u16> = format!("ZenScroll v{}\0", env!("CARGO_PKG_VERSION")).encode_utf16().collect();
            // SAFETY: FindWindowW searches for existing gpui window by title.
            if let Ok(hwnd) = unsafe { FindWindowW(None, PCWSTR::from_raw(title.as_ptr())) }
                && !hwnd.0.is_null()
            {
                // SAFETY: ShowWindow and SetForegroundWindow bring the existing window to front.
                unsafe {
                    let _ = ShowWindow(hwnd, SW_SHOW);
                    let _ = SetForegroundWindow(hwnd);
                }
                eprintln!("[ZenScroll] 已有实例在运行，已激活其窗口");
                return;
            }
            eprintln!("[ZenScroll] 发现前一个实例的互斥体但窗口不存在，可能已崩溃，继续启动");
        }
        let _ = mutex; // keep handle open for process lifetime (HANDLE has no Drop)
    }

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
        config::sync_autostart(cfg.autostart);
    }

    sync_console_state();

    // Thread 2: 安装钩子 + 创建托盘 + 消息泵（必须在同一线程）
    let (pump_tx, pump_rx) = std::sync::mpsc::channel::<()>();
    let pump_handle = thread::spawn(move || {
        // 钩子必须在消息泵所在线程安装，否则 WH_MOUSE_LL 回调无法被泵派发
        if let Err(e) = hook::install_hook() {
            eprintln!("[ZenScroll] 安装钩子失败: {}", e);
            return;
        }
        if let Ok(mut state) = hook::HOOK_STATE.lock() {
            state.enabled = config::is_enabled();
        }
        tray::create_tray_window();
        pump_tx.send(()).ok();
        hook::run_message_pump();
        // 消息泵退出后，先卸载钩子防止残留消息阻塞鼠标输入，再恢复系统设置
        hook::uninstall_hook();
        restore_scroll_lines();
        tray::destroy_tray();
    });

    // 等待线程 2 初始化完成
    pump_rx.recv().ok();
    eprintln!(
        "[ZenScroll v{}] 系统滚轮优化已启动 (启用={})",
        env!("CARGO_PKG_VERSION"),
        config::is_enabled()
    );
    eprintln!("[ZenScroll] 右键托盘图标控制");

    // Thread 3: 物理注入循环（每 4ms 推送）
    let inject_handle = thread::spawn(|| {
        injection_loop();
    });

    // Thread 1 (主线程): gpui 配置面板
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
            |window, cx| {
                // 点击 X 不销毁窗口，仅隐藏（hwnd 缓存后由 Win32 ShowWindow 实现）
                window.on_window_should_close(cx, |_, _| {
                    if tray::should_exit() {
                        true
                    } else {
                        let hwnd_val = GPUI_HWND.load(Ordering::SeqCst);
                        if hwnd_val != 0 {
                            // SAFETY: GPUI_HWND stores the valid HWND of our gpui window.
                            unsafe { let _ = ShowWindow(HWND(hwnd_val as *mut _), SW_HIDE); }
                        }
                        false
                    }
                });
                cx.new(|_| ConfigPanel::new())
            },
        )
        .unwrap();
        cx.activate(true);

        // 缓存 gpui 窗口的 HWND，供 should_close 隐藏和托盘 CMD_LAUNCH_UI 显示
        let title = format!("ZenScroll v{}\0", env!("CARGO_PKG_VERSION"));
        let wide: Vec<u16> = title.encode_utf16().collect();
        // SAFETY: FindWindowW searches by class atom and/or window title.
        // Our gpui window has just been created with the exact title above.
        if let Ok(hwnd) = unsafe { FindWindowW(None, PCWSTR::from_raw(wide.as_ptr())) }
            && !hwnd.0.is_null()
        {
            GPUI_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
        }

        // 仅当用户通过托盘"退出"（TRAY_EXIT=true）关闭窗口时才退出进程；
        // 点击 X 关闭面板只隐藏 UI，程序继续后台运行
        let _keep = cx.on_window_closed(|cx| {
            if tray::should_exit() {
                cx.quit();
            }
        });
    });

    // gpui 退出后（仅由托盘"退出"触发），通知其他线程停止
    tray::signal_quit();
    pump_handle.join().ok();
    inject_handle.join().ok();
}
