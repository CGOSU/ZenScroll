use crate::plugin::{Plugin, PluginContext};

pub struct Snap {
    positions: Vec<f64>,
    threshold: f64,
    axis: SnapAxis,
}

pub enum SnapAxis {
    Y,
    X,
}

impl Snap {
    pub fn new(positions: Vec<f64>, axis: SnapAxis) -> Self {
        Self {
            positions,
            threshold: 0.25,
            axis,
        }
    }

    pub fn set_positions(&mut self, positions: Vec<f64>) {
        self.positions = positions;
    }

    pub fn nearest_position(&self, current: f64) -> Option<f64> {
        if self.positions.is_empty() {
            return None;
        }
        self.positions
            .iter()
            .min_by(|a, b| {
                let dist_a = (*a - current).abs();
                let dist_b = (*b - current).abs();
                dist_a.partial_cmp(&dist_b).unwrap_or(core::cmp::Ordering::Equal)
            })
            .copied()
    }
}

impl Plugin for Snap {
    fn name(&self) -> &'static str {
        "snap"
    }

    fn on_scroll_end(&mut self, ctx: &mut PluginContext) {
        let current = match self.axis {
            SnapAxis::Y => ctx.scroller.physics_y.offset,
            SnapAxis::X => ctx.scroller.physics_x.offset,
        };

        if let Some(target) = self.nearest_position(current) {
            let dist = (target - current).abs();
            if dist > self.threshold {
                match self.axis {
                    SnapAxis::Y => ctx.scroller.physics_y.snap_to(target),
                    SnapAxis::X => ctx.scroller.physics_x.snap_to(target),
                }
            }
        }
    }
}
