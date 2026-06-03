use core::time::Duration;

#[derive(Debug, Clone)]
pub struct ScrollConfig {
    pub friction: f64,
    pub bounce_tension: f64,
    pub min_velocity: f64,
    pub max_velocity: f64,
    pub scroll_accel: f64,
    pub deceleration_rate: f64,
    pub max_bounce_distance: f64,
    pub smartwheel_friction_max: f64,
}

pub const PRESET_SLOW: ScrollConfig = ScrollConfig {
    friction: 0.92,
    bounce_tension: 0.90,
    min_velocity: 0.3,
    max_velocity: 80.0,
    scroll_accel: 0.8,
    smartwheel_friction_max: 0.97,
    deceleration_rate: 0.998,
    max_bounce_distance: 150.0,
};

pub const PRESET_NORMAL: ScrollConfig = ScrollConfig {
    friction: 0.94,
    bounce_tension: 0.85,
    min_velocity: 0.3,
    max_velocity: 200.0,
    scroll_accel: 1.5,
    smartwheel_friction_max: 0.985,
    deceleration_rate: 0.998,
    max_bounce_distance: 150.0,
};

pub const PRESET_FAST: ScrollConfig = ScrollConfig {
    friction: 0.95,
    bounce_tension: 0.80,
    min_velocity: 0.5,
    max_velocity: 350.0,
    scroll_accel: 2.5,
    smartwheel_friction_max: 0.992,
    deceleration_rate: 0.998,
    max_bounce_distance: 150.0,
};

pub const PRESETS: [ScrollConfig; 3] = [PRESET_SLOW, PRESET_NORMAL, PRESET_FAST];

impl Default for ScrollConfig {
    fn default() -> Self {
        Self {
            friction: 0.92,
            bounce_tension: 0.85,
            min_velocity: 0.5,
            max_velocity: 150.0,
            scroll_accel: 1.2,
            deceleration_rate: 0.998,
            max_bounce_distance: 150.0,
            smartwheel_friction_max: 0.985,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PhysicsState {
    pub offset: f64,
    pub velocity: f64,
    pub phase: ScrollPhase,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ScrollPhase {
    Idle,
    Momentum,
    Bouncing,
}

impl PhysicsState {
    pub fn new() -> Self {
        Self {
            offset: 0.0,
            velocity: 0.0,
            phase: ScrollPhase::Idle,
        }
    }

    pub fn update(&mut self, config: &ScrollConfig, max_offset: f64, dt: Duration) -> bool {
        let dt_secs = dt.as_secs_f64().min(0.05);

        match self.phase {
            ScrollPhase::Idle => {
                if self.velocity.abs() > config.min_velocity {
                    self.phase = ScrollPhase::Momentum;
                }
            }
            ScrollPhase::Momentum => {
                self.offset += self.velocity * dt_secs * 60.0;
                self.velocity *= config.friction;

                if self.velocity.abs() <= config.min_velocity {
                    self.velocity = 0.0;
                    self.phase = ScrollPhase::Idle;
                }
            }
            ScrollPhase::Bouncing => {
                if self.offset < 0.0 {
                    self.offset *= config.bounce_tension;
                    if self.offset.abs() < 1.0 {
                        self.offset = 0.0;
                        self.velocity = 0.0;
                        self.phase = ScrollPhase::Idle;
                    }
                } else if self.offset > max_offset {
                    let overshoot = self.offset - max_offset;
                    self.offset = max_offset + overshoot * config.bounce_tension;
                    if overshoot.abs() < 1.0 {
                        self.offset = max_offset;
                        self.velocity = 0.0;
                        self.phase = ScrollPhase::Idle;
                    }
                } else {
                    self.phase = ScrollPhase::Idle;
                }
            }
        }

        if self.velocity.abs() > config.min_velocity || self.phase != ScrollPhase::Idle {
            return true;
        }

        false
    }

    pub fn apply_delta(&mut self, config: &ScrollConfig, delta: f64) {
        self.velocity += delta * config.scroll_accel;
        self.velocity = self.velocity.clamp(-config.max_velocity, config.max_velocity);
        self.phase = if self.velocity.abs() > config.min_velocity {
            ScrollPhase::Momentum
        } else {
            ScrollPhase::Idle
        };
    }

    pub fn snap_to(&mut self, target: f64) {
        self.offset = target;
        self.velocity = 0.0;
        self.phase = ScrollPhase::Idle;
    }

    pub fn is_moving(&self) -> bool {
        self.velocity.abs() > 0.0 || self.phase != ScrollPhase::Idle
    }
}

pub fn smartwheel_friction(config: &ScrollConfig, velocity: f64) -> f64 {
    let speed_ratio = (velocity.abs() / config.max_velocity).clamp(0.0, 1.0);
    let weight = speed_ratio.powi(3);
    config.friction
        + (config.smartwheel_friction_max - config.friction) * weight
}

/// Maps the time between scroll events to a velocity multiplier.
/// Fast scrolling (short interval) → higher factor → longer jump per notch.
/// Slow scrolling (long interval) → lower factor → precise short jump.
pub fn adaptive_scroll_factor(interval_ms: f64) -> f64 {
    const FAST_MS: f64 = 30.0;
    const SLOW_MS: f64 = 300.0;
    const MIN_FACTOR: f64 = 0.3;
    const MAX_FACTOR: f64 = 3.0;

    if interval_ms >= SLOW_MS {
        MIN_FACTOR
    } else if interval_ms <= FAST_MS {
        MAX_FACTOR
    } else {
        let t = (interval_ms - FAST_MS) / (SLOW_MS - FAST_MS);
        MAX_FACTOR + (MIN_FACTOR - MAX_FACTOR) * t
    }
}
