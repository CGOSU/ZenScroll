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
}

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
