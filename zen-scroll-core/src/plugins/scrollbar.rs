#[derive(Debug, Clone)]
pub struct ScrollbarConfig {
    pub width: f64,
    pub min_length: f64,
    pub margin: f64,
    pub opacity: f64,
    pub hover_opacity: f64,
    pub fade_delay: f64,
    pub fade_duration: f64,
}

impl Default for ScrollbarConfig {
    fn default() -> Self {
        Self {
            width: 6.0,
            min_length: 20.0,
            margin: 2.0,
            opacity: 0.4,
            hover_opacity: 0.8,
            fade_delay: 1.0,
            fade_duration: 0.3,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScrollbarMetrics {
    pub thumb_x: f64,
    pub thumb_y: f64,
    pub thumb_width: f64,
    pub thumb_height: f64,
    pub track_width: f64,
    pub track_height: f64,
    pub visible: bool,
}

pub struct Scrollbar {
    pub config: ScrollbarConfig,
    pub metrics: Option<ScrollbarMetrics>,
}

impl Scrollbar {
    pub fn new() -> Self {
        Self {
            config: ScrollbarConfig::default(),
            metrics: None,
        }
    }

    pub fn with_config(config: ScrollbarConfig) -> Self {
        Self {
            config,
            metrics: None,
        }
    }

    pub fn calculate(
        &self,
        viewport_w: f64,
        viewport_h: f64,
        content_w: f64,
        content_h: f64,
        scroll_x: f64,
        scroll_y: f64,
    ) -> Option<ScrollbarMetrics> {
        let show_vertical = content_h > viewport_h;
        let show_horizontal = content_w > viewport_w;

        if !show_vertical && !show_horizontal {
            return None;
        }

        let track_h = viewport_h - self.config.margin * 2.0;
        let track_w = viewport_w - self.config.margin * 2.0;

        let thumb_height = (viewport_h / content_h * track_h).max(self.config.min_length);
        let thumb_width = (viewport_w / content_w * track_w).max(self.config.min_length);

        let max_scroll_y = (content_h - viewport_h).max(1.0);
        let max_scroll_x = (content_w - viewport_w).max(1.0);

        let progress_y = scroll_y / max_scroll_y;
        let progress_x = scroll_x / max_scroll_x;

        let thumb_y = progress_y * (track_h - thumb_height);
        let thumb_x = progress_x * (track_w - thumb_width);

        Some(ScrollbarMetrics {
            thumb_x,
            thumb_y,
            thumb_width,
            thumb_height,
            track_width: track_w,
            track_height: track_h,
            visible: true,
        })
    }
}
