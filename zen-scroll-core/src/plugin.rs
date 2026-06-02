use core::time::Duration;

use crate::scroll::Scroller;

#[derive(Debug)]
pub struct PluginContext<'a> {
    pub scroller: &'a mut Scroller,
    pub dt: Duration,
}

pub trait Plugin: Send {
    fn name(&self) -> &'static str;

    fn on_scroll(&mut self, _ctx: &mut PluginContext) {}

    fn on_scroll_end(&mut self, _ctx: &mut PluginContext) {}

    fn on_resize(&mut self, _ctx: &mut PluginContext) {}

    fn on_enabled_change(&mut self, _enabled: bool) {}
}

pub struct PluginManager {
    plugins: Vec<Box<dyn Plugin>>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    pub fn register(&mut self, plugin: Box<dyn Plugin>) -> &mut Self {
        self.plugins.push(plugin);
        self
    }

    pub fn on_scroll(&mut self, ctx: &mut PluginContext) {
        for plugin in &mut self.plugins {
            plugin.on_scroll(ctx);
        }
    }

    pub fn on_scroll_end(&mut self, ctx: &mut PluginContext) {
        for plugin in &mut self.plugins {
            plugin.on_scroll_end(ctx);
        }
    }

    pub fn on_resize(&mut self, ctx: &mut PluginContext) {
        for plugin in &mut self.plugins {
            plugin.on_resize(ctx);
        }
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Box<dyn Plugin>> {
        self.plugins.iter_mut()
    }
}
