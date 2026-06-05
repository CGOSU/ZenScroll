use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_MOUSE, MOUSE_EVENT_FLAGS, MOUSEINPUT, SendInput,
};
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, PostMessageW};

use crate::debug_log;
use crate::detect::listview_is_valid;
use zen_scroll_core::physics::{ScrollConfig, adaptive_scroll_factor, smartwheel_friction};

const WHEEL_DELTA: i32 = 120;
const TICK_INTERVAL: Duration = Duration::from_millis(4);
const LVM_FIRST: u32 = 0x1000;
const LVM_SCROLL: u32 = LVM_FIRST + 20;
const INERTIA_CANCEL_MOVE_PX: i32 = 16;

pub static INJECTING: AtomicBool = AtomicBool::new(false);

pub struct InjectGuard;

impl InjectGuard {
    pub fn new() -> Self {
        INJECTING.store(true, Ordering::SeqCst);
        InjectGuard
    }
}

impl Drop for InjectGuard {
    fn drop(&mut self) {
        INJECTING.store(false, Ordering::SeqCst);
    }
}

pub struct SmoothInjector {
    velocity: f64,
    config: ScrollConfig,
    active: bool,
    last_tick: Instant,
    last_scroll_time: Instant,
    fraction: f64,
    scroll_anchor: (i32, i32),
    target_hwnd: isize,
    listview_hwnd: isize,
    chunked_wheel: bool,
}

impl SmoothInjector {
    pub fn new(config: ScrollConfig) -> Self {
        let now = Instant::now();
        Self {
            velocity: 0.0,
            config,
            active: false,
            last_tick: now,
            last_scroll_time: now,
            fraction: 0.0,
            scroll_anchor: (0, 0),
            target_hwnd: 0,
            listview_hwnd: 0,
            chunked_wheel: false,
        }
    }

    pub fn set_config(&mut self, config: ScrollConfig) {
        self.config = config;
    }

    #[allow(dead_code)]
    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn velocity(&self) -> f64 {
        self.velocity
    }

    pub fn feed_wheel(
        &mut self,
        raw_delta: i32,
        pt: (i32, i32),
        target_hwnd: isize,
        listview_hwnd: isize,
        chunked_wheel: bool,
    ) {
        let now = Instant::now();

        let target_changed = target_hwnd != self.target_hwnd
            || chunked_wheel != self.chunked_wheel
            || listview_hwnd != self.listview_hwnd;

        let factor = if !self.active || target_changed {
            debug_log!(
                "注入: 重置 (active={}, target_changed={})",
                self.active,
                target_changed
            );
            self.velocity = 0.0;
            self.fraction = 0.0;
            self.target_hwnd = target_hwnd;
            self.listview_hwnd = listview_hwnd;
            self.chunked_wheel = chunked_wheel;
            self.last_tick = now;
            0.5
        } else {
            let interval = now.duration_since(self.last_scroll_time).as_secs_f64() * 1000.0;
            adaptive_scroll_factor(interval)
        };
        self.scroll_anchor = pt;
        self.last_scroll_time = now;

        let v = raw_delta as f64 * self.config.scroll_accel * factor;
        self.velocity += v;
        self.velocity = self
            .velocity
            .clamp(-self.config.max_velocity, self.config.max_velocity);

        self.active = true;
    }

    pub fn tick(&mut self) -> bool {
        // 惯性取消：鼠标移动超过阈值则停止滚动
        let mut cursor_pt = POINT { x: 0, y: 0 };
        if unsafe { GetCursorPos(&mut cursor_pt) }.is_ok()
            && point_moved_beyond(
                (cursor_pt.x, cursor_pt.y),
                self.scroll_anchor,
                INERTIA_CANCEL_MOVE_PX,
            )
        {
            self.stop();
            return false;
        }

        // 应用摩擦

        let now = Instant::now();
        let dt = now.duration_since(self.last_tick);
        self.last_tick = now;

        let dt_ratio = dt.as_secs_f64() / TICK_INTERVAL.as_secs_f64();
        let raw_send = self.velocity * dt_ratio + self.fraction;
        let send = raw_send as i32;
        self.fraction = raw_send - send as f64;

        self.velocity *= smartwheel_friction(&self.config, self.velocity);

        if self.velocity.abs() < self.config.min_velocity {
            self.velocity = 0.0;
            self.fraction = 0.0;
            self.active = false;
            return false;
        }

        if send != 0 {
            let delta = send.clamp(-WHEEL_DELTA * 4, WHEEL_DELTA * 4);

            if self.listview_hwnd != 0 && listview_is_valid(self.listview_hwnd) {
                let pixels = -(delta / 3);
                let pixels = pixels.clamp(-80, 80);
                if pixels != 0 {
                    // SAFETY: PostMessageW posts a LVM_SCROLL message to the ListView window
                    // for pixel-level scrolling in Explorer/TaskManager.
                    let hwnd = HWND(self.listview_hwnd as *mut _);
                    unsafe {
                        let _ = PostMessageW(hwnd, LVM_SCROLL, WPARAM(0), LPARAM(pixels as isize));
                    }
                }
            } else if self.chunked_wheel {
                // 分块注入：将 >120 的增量拆成 120 的块，兼容 Explorer 等控件
                let mut remainder = delta as f64;
                let input = INPUT {
                    r#type: INPUT_MOUSE,
                    Anonymous: INPUT_0 {
                        mi: MOUSEINPUT {
                            dx: 0,
                            dy: 0,
                            mouseData: 0,
                            dwFlags: MOUSE_EVENT_FLAGS(0x0800u32),
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                };
                while remainder >= WHEEL_DELTA as f64 || remainder <= -(WHEEL_DELTA as f64) {
                    let chunk = if remainder > 0.0 {
                        WHEEL_DELTA
                    } else {
                        -WHEEL_DELTA
                    };
                    remainder -= chunk as f64;
                    let mut input = input;
                    input.Anonymous.mi.mouseData = chunk as u32;
                    let _guard = InjectGuard::new();
                    // SAFETY: SendInput injects a chunked wheel event. INJECTING prevents re-entry.
                    unsafe {
                        SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
                    }
                }
            } else {
                let input = INPUT {
                    r#type: INPUT_MOUSE,
                    Anonymous: INPUT_0 {
                        mi: MOUSEINPUT {
                            dx: 0,
                            dy: 0,
                            mouseData: delta as u32,
                            dwFlags: MOUSE_EVENT_FLAGS(0x0800u32),
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                };
                let _guard = InjectGuard::new();
                // SAFETY: SendInput injects a synthetic mouse wheel event into the system input stream.
                unsafe {
                    SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
                }
            }
        }

        true
    }

    #[allow(dead_code)]
    pub fn stop(&mut self) {
        self.velocity = 0.0;
        self.fraction = 0.0;
        self.active = false;
    }
}

fn point_moved_beyond(a: (i32, i32), b: (i32, i32), threshold: i32) -> bool {
    (a.0 - b.0).abs() > threshold || (a.1 - b.1).abs() > threshold
}
