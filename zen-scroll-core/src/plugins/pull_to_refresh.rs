use crate::plugin::{Plugin, PluginContext};

#[derive(Debug, Clone, PartialEq)]
pub enum RefreshStatus {
    Idle,
    Pulling(f64),
    Ready,
    Refreshing,
    Complete,
}

pub struct PullToRefresh {
    threshold: f64,
    status: RefreshStatus,
    max_pull_distance: f64,
    on_refresh: Option<Box<dyn FnMut() + Send>>,
}

impl PullToRefresh {
    pub fn new(threshold: f64) -> Self {
        Self {
            threshold,
            status: RefreshStatus::Idle,
            max_pull_distance: threshold * 1.5,
            on_refresh: None,
        }
    }

    pub fn status(&self) -> &RefreshStatus {
        &self.status
    }

    pub fn set_on_refresh<F: FnMut() + Send + 'static>(&mut self, cb: F) {
        self.on_refresh = Some(Box::new(cb));
    }

    pub fn complete(&mut self) {
        self.status = RefreshStatus::Complete;
    }

    pub fn reset(&mut self) {
        self.status = RefreshStatus::Idle;
    }
}

impl Plugin for PullToRefresh {
    fn name(&self) -> &'static str {
        "pull_to_refresh"
    }

    fn on_scroll(&mut self, ctx: &mut PluginContext) {
        let offset = ctx.scroller.physics_y.offset;

        match self.status {
            RefreshStatus::Idle | RefreshStatus::Pulling(_) | RefreshStatus::Ready => {
                if offset < 0.0 {
                    let pull = offset.abs();
                    if pull >= self.threshold {
                        self.status = RefreshStatus::Ready;
                    } else {
                        self.status = RefreshStatus::Pulling(pull);
                    }

                    if pull > self.max_pull_distance {
                        ctx.scroller.physics_y.offset = -self.max_pull_distance;
                        ctx.scroller.physics_y.velocity = 0.0;
                    }
                } else if matches!(self.status, RefreshStatus::Pulling(_)) && offset >= 0.0 {
                    self.status = RefreshStatus::Idle;
                }
            }
            RefreshStatus::Refreshing => {
                ctx.scroller.physics_y.offset = -self.threshold;
                ctx.scroller.physics_y.velocity = 0.0;
            }
            RefreshStatus::Complete => {
                let ease_back = ctx.scroller.physics_y.offset * 0.85;
                ctx.scroller.physics_y.offset = ease_back;
                if ease_back.abs() < 1.0 {
                    ctx.scroller.physics_y.offset = 0.0;
                    self.status = RefreshStatus::Idle;
                }
            }
        }
    }

    fn on_scroll_end(&mut self, ctx: &mut PluginContext) {
        if self.status == RefreshStatus::Ready {
            self.status = RefreshStatus::Refreshing;
            if let Some(ref mut cb) = self.on_refresh {
                cb();
            }
            ctx.scroller.physics_y.offset = -self.threshold;
            ctx.scroller.physics_y.velocity = 0.0;
        }
    }
}
