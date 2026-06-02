use core::time::Duration;

use crate::physics::{PhysicsState, ScrollConfig};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollAxis {
    Vertical,
    Horizontal,
    Both,
}

impl ScrollAxis {
    pub fn is_vertical(&self) -> bool {
        matches!(self, Self::Vertical | Self::Both)
    }

    pub fn is_horizontal(&self) -> bool {
        matches!(self, Self::Horizontal | Self::Both)
    }
}

#[derive(Debug, Clone)]
pub struct ScrollBounds {
    pub min: f64,
    pub max: f64,
}

impl ScrollBounds {
    pub fn new(max: f64) -> Self {
        Self { min: 0.0, max: max.max(0.0) }
    }

    pub fn clamp(&self, value: f64) -> f64 {
        value.clamp(self.min, self.max)
    }
}

#[derive(Debug, Clone)]
pub struct Scroller {
    pub config: ScrollConfig,
    pub axis: ScrollAxis,
    pub physics_x: PhysicsState,
    pub physics_y: PhysicsState,
    pub content_width: f64,
    pub content_height: f64,
    pub viewport_width: f64,
    pub viewport_height: f64,
    pub enabled: bool,
}

impl Scroller {
    pub fn new(axis: ScrollAxis) -> Self {
        Self {
            config: ScrollConfig::default(),
            axis,
            physics_x: PhysicsState::new(),
            physics_y: PhysicsState::new(),
            content_width: 0.0,
            content_height: 0.0,
            viewport_width: 0.0,
            viewport_height: 0.0,
            enabled: true,
        }
    }

    pub fn with_config(config: ScrollConfig, axis: ScrollAxis) -> Self {
        Self {
            config,
            axis,
            ..Self::new(axis)
        }
    }

    pub fn scroll_x(&self) -> f64 {
        self.physics_x.offset
    }

    pub fn scroll_y(&self) -> f64 {
        self.physics_y.offset
    }

    pub fn max_scroll_x(&self) -> f64 {
        (self.content_width - self.viewport_width).max(0.0)
    }

    pub fn max_scroll_y(&self) -> f64 {
        (self.content_height - self.viewport_height).max(0.0)
    }

    pub fn tick(&mut self, dt: Duration) -> bool {
        if !self.enabled {
            return false;
        }

        let max_x = self.max_scroll_x();
        let max_y = self.max_scroll_y();

        let mut active = false;

        if self.axis.is_horizontal() {
            if self.physics_x.update(&self.config, max_x, dt) {
                active = true;
            }
        }

        if self.axis.is_vertical() {
            if self.physics_y.update(&self.config, max_y, dt) {
                active = true;
            }
        }

        active
    }

    pub fn scroll_by(&mut self, dx: f64, dy: f64) {
        if !self.enabled {
            return;
        }

        if self.axis.is_horizontal() {
            self.physics_x.apply_delta(&self.config, dx);
        }
        if self.axis.is_vertical() {
            self.physics_y.apply_delta(&self.config, dy);
        }
    }

    pub fn scroll_to(&mut self, x: f64, y: f64) {
        if !self.enabled {
            return;
        }

        let max_x = self.max_scroll_x();
        let max_y = self.max_scroll_y();

        if self.axis.is_horizontal() {
            self.physics_x.snap_to(x.clamp(0.0, max_x));
        }
        if self.axis.is_vertical() {
            self.physics_y.snap_to(y.clamp(0.0, max_y));
        }
    }

    pub fn stop(&mut self) {
        self.physics_x.snap_to(self.physics_x.offset);
        self.physics_y.snap_to(self.physics_y.offset);
    }

    pub fn is_moving(&self) -> bool {
        self.physics_x.is_moving() || self.physics_y.is_moving()
    }

    pub fn is_at_top(&self) -> bool {
        self.physics_y.offset <= 0.0
    }

    pub fn is_at_bottom(&self) -> bool {
        self.physics_y.offset >= self.max_scroll_y()
    }

    pub fn is_at_left(&self) -> bool {
        self.physics_x.offset <= 0.0
    }

    pub fn is_at_right(&self) -> bool {
        self.physics_x.offset >= self.max_scroll_x()
    }

    pub fn progress_x(&self) -> f64 {
        let max = self.max_scroll_x();
        if max <= 0.0 { 0.0 } else { self.physics_x.offset / max }
    }

    pub fn progress_y(&self) -> f64 {
        let max = self.max_scroll_y();
        if max <= 0.0 { 0.0 } else { self.physics_y.offset / max }
    }
}
