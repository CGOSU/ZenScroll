use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEINPUT, MOUSE_EVENT_FLAGS,
};

use zen_scroll_core::physics::{adaptive_scroll_factor, smartwheel_friction, ScrollConfig};

const WHEEL_DELTA: i32 = 120;
const TICK_INTERVAL: Duration = Duration::from_millis(8);

pub static INJECTING: AtomicBool = AtomicBool::new(false);

pub struct SmoothInjector {
    velocity: f64,
    config: ScrollConfig,
    active: bool,
    last_tick: Instant,
    last_scroll_time: Instant,
    fraction: f64,
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
        let now = Instant::now();
        let factor = if self.active {
            let interval = now.duration_since(self.last_scroll_time).as_secs_f64() * 1000.0;
            adaptive_scroll_factor(interval)
        } else {
            0.5
        };
        self.last_scroll_time = now;

        let v = raw_delta as f64 * self.config.scroll_accel * factor;
        self.velocity += v;
        self.velocity = self
            .velocity
            .clamp(-self.config.max_velocity, self.config.max_velocity);

        if !self.active {
            self.last_tick = now;
        }
        self.active = true;
    }

    pub fn tick(&mut self) -> bool {
        if !self.active {
            return false;
        }

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
            // SAFETY: SendInput injects a synthetic mouse wheel event into the system input stream.
            // The INJECTING flag prevents the WH_MOUSE_LL hook from processing our own injected events.
            unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32); }
            INJECTING.store(false, Ordering::SeqCst);
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
