use crate::plugin::{Plugin, PluginContext};

#[derive(Debug, Clone, PartialEq)]
pub enum LoadStatus {
    Idle,
    Loading,
    Complete,
    Error,
}

pub struct InfiniteScroll {
    threshold: f64,
    status: LoadStatus,
    on_load_more: Option<Box<dyn FnMut() + Send>>,
}

impl InfiniteScroll {
    pub fn new(threshold: f64) -> Self {
        Self {
            threshold,
            status: LoadStatus::Idle,
            on_load_more: None,
        }
    }

    pub fn status(&self) -> &LoadStatus {
        &self.status
    }

    pub fn set_on_load_more<F: FnMut() + Send + 'static>(&mut self, cb: F) {
        self.on_load_more = Some(Box::new(cb));
    }

    pub fn complete(&mut self) {
        self.status = LoadStatus::Idle;
    }

    pub fn set_error(&mut self) {
        self.status = LoadStatus::Error;
    }
}

impl Plugin for InfiniteScroll {
    fn name(&self) -> &'static str {
        "infinite_scroll"
    }

    fn on_scroll(&mut self, ctx: &mut PluginContext) {
        if self.status != LoadStatus::Idle {
            return;
        }

        let scroller = &ctx.scroller;
        let remaining = scroller.content_height - (scroller.scroll_y() + scroller.viewport_height);
        let is_near_bottom = remaining <= self.threshold;

        if is_near_bottom && scroller.physics_y.velocity > 0.0 {
            self.status = LoadStatus::Loading;
            if let Some(ref mut cb) = self.on_load_more {
                cb();
            }
        }
    }
}
