use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEINPUT, MOUSE_EVENT_FLAGS,
};

use zen_scroll_core::physics::ScrollConfig;

const WHEEL_DELTA: i32 = 120;
const TICK_INTERVAL: Duration = Duration::from_millis(8);

pub static INJECTING: AtomicBool = AtomicBool::new(false);

pub struct SmoothInjector {
    velocity: f64,
    config: ScrollConfig,
    active: bool,
    last_tick: Instant,
}

impl SmoothInjector {
    pub fn new(config: ScrollConfig) -> Self {
        Self {
            velocity: 0.0,
            config,
            active: false,
            last_tick: Instant::now(),
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

    pub fn feed_wheel(&mut self, raw_delta: i32) {
        let v = raw_delta as f64 * self.config.scroll_accel;
        self.velocity += v;
        self.velocity = self
            .velocity
            .clamp(-self.config.max_velocity, self.config.max_velocity);
        self.active = true;
        self.last_tick = Instant::now();
    }

    pub fn tick(&mut self) -> bool {
        if !self.active {
            return false;
        }

        let now = Instant::now();
        let dt = now.duration_since(self.last_tick);
        self.last_tick = now;

        let dt_ratio = dt.as_secs_f64() / TICK_INTERVAL.as_secs_f64();
        let send = (self.velocity * dt_ratio) as i32;

        self.velocity *= self.config.friction;

        if self.velocity.abs() < self.config.min_velocity {
            self.velocity = 0.0;
            self.active = false;
            return false;
        }

        if send != 0 {
            let delta = send.clamp(-WHEEL_DELTA * 4, WHEEL_DELTA * 4);

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

            INJECTING.store(true, Ordering::SeqCst);
            unsafe {
                SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
            }
            INJECTING.store(false, Ordering::SeqCst);
        }

        true
    }

    #[allow(dead_code)]
    pub fn stop(&mut self) {
        self.velocity = 0.0;
        self.active = false;
    }
}
