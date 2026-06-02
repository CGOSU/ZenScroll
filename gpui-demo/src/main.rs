use gpui::*;
use zen_scroll_core::physics::ScrollConfig;

#[derive(Clone)]
struct SliderParam {
    label: &'static str,
    value: f64,
    min: f64,
    max: f64,
    step: f64,
}

impl SliderParam {
    fn new(label: &'static str, value: f64, min: f64, max: f64, step: f64) -> Self {
        Self { label, value, min, max, step }
    }

    fn ratio(&self) -> f64 {
        (self.value - self.min) / (self.max - self.min)
    }

    #[allow(dead_code)]
    fn set_from_ratio(&mut self, r: f64) {
        let raw = self.min + r.clamp(0.0, 1.0) * (self.max - self.min);
        self.value = (raw / self.step).round() * self.step;
    }
}

struct AppProfile {
    name: &'static str,
    vals: [f64; 5],
}

static PROFILES: &[AppProfile] = &[
    AppProfile { name: "Chrome",  vals: [0.94, 0.85, 1.5, 200.0, 0.30] },
    AppProfile { name: "Readest", vals: [0.96, 0.90, 1.0, 120.0, 0.20] },
    AppProfile { name: "Firefox", vals: [0.93, 0.85, 1.3, 180.0, 0.40] },
];

struct ConfigPanel {
    selected: usize,
    enabled: bool,
    sliders: [SliderParam; 5],
}

const SLIDER_DEFS: [(&str, f64, f64, f64); 5] = [
    ("摩擦力 Friction",      0.80, 0.99, 0.01),
    ("回弹力 Bounce",        0.50, 1.00, 0.01),
    ("加速度 Accel",         0.10, 3.00, 0.10),
    ("最大速度 MaxV",        30.0, 400., 5.00),
    ("最小速度 MinV",        0.05, 2.00, 0.05),
];

impl ConfigPanel {
    fn new() -> Self {
        let p = &PROFILES[0];
        Self {
            selected: 0,
            enabled: true,
            sliders: Self::make_sliders(&p.vals),
        }
    }

    fn make_sliders(vals: &[f64; 5]) -> [SliderParam; 5] {
        let mut s = [
            SliderParam::new("", 0.0, 0.0, 0.0, 0.0),
            SliderParam::new("", 0.0, 0.0, 0.0, 0.0),
            SliderParam::new("", 0.0, 0.0, 0.0, 0.0),
            SliderParam::new("", 0.0, 0.0, 0.0, 0.0),
            SliderParam::new("", 0.0, 0.0, 0.0, 0.0),
        ];
        for i in 0..5 {
            let (label, lo, hi, step) = SLIDER_DEFS[i];
            s[i] = SliderParam::new(label, vals[i], lo, hi, step);
        }
        s
    }

    fn select_profile(&mut self, idx: usize) {
        if idx != self.selected && idx < PROFILES.len() {
            self.selected = idx;
            self.sliders = Self::make_sliders(&PROFILES[idx].vals);
        }
    }

    fn current_config(&self) -> ScrollConfig {
        ScrollConfig {
            friction: self.sliders[0].value,
            bounce_tension: self.sliders[1].value,
            scroll_accel: self.sliders[2].value,
            max_velocity: self.sliders[3].value,
            min_velocity: self.sliders[4].value,
            ..Default::default()
        }
    }
}

impl Render for ConfigPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(0x1a1a2e))
            .flex()
            .flex_col()
            .child(self.header())
            .child(self.profile_selector(cx))
            .child(self.params_panel(cx))
            .child(self.status_bar())
    }
}

impl ConfigPanel {
    fn header(&self) -> impl IntoElement {
        div()
            .h(px(48.0))
            .bg(rgb(0x16213e))
            .flex()
            .items_center()
            .justify_between()
            .px_4()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().w(px(10.0)).h(px(10.0)).rounded_full()
                        .bg(if self.enabled { rgb(0x44ff88) } else { rgb(0x555555) }))
                    .child(div().text_color(rgb(0x88ccff)).font_weight(FontWeight(700.0)).text_lg()
                        .child("ZenScroll Control Panel")),
            )
            .child(
                div().text_color(if self.enabled { rgb(0x44ff88) } else { rgb(0x555555) }).text_sm()
                    .child(if self.enabled { "Running" } else { "Stopped" }),
            )
    }

    fn profile_selector(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .h(px(44.0))
            .flex()
            .items_center()
            .gap_2()
            .px_4()
            .border_b_1()
            .border_color(rgb(0x2a2a4a))
            .child(div().text_color(rgb(0x8888aa)).text_sm().child("Profile:"))
            .children(PROFILES.iter().enumerate().map(|(i, p)| {
                let is_sel = i == self.selected;
                div()
                    .px_3()
                    .py_1()
                    .bg(if is_sel { rgb(0x0f3460) } else { rgb(0x1a1a2e) })
                    .border_1()
                    .border_color(if is_sel { rgb(0x88ccff) } else { rgb(0x2a2a4a) })
                    .rounded_md()
                    .text_color(if is_sel { rgb(0x88ccff) } else { rgb(0x8888aa) })
                    .text_sm()
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, cx.listener(move |this: &mut Self, _: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>| {
                        this.select_profile(i);
                        cx.notify();
                    }))
                    .child(p.name)
            }))
    }

    fn params_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut slider_rows: Vec<AnyElement> = Vec::new();
        for (i, s) in self.sliders.iter().enumerate() {
            slider_rows.push(self.slider_row(i, s, cx).into_any_element());
        }

        div()
            .flex_1()
            .px_4()
            .py_2()
            .child(
                div().flex().flex_col().gap_1()
                    .child(div().text_color(rgb(0x88ccff)).text_sm().font_weight(FontWeight(600.0))
                        .child(format!("Physics — {}", PROFILES[self.selected].name)))
                    .child(div().h(px(4.0)))
                    .children(slider_rows)
                    .child(div().h(px(8.0)))
                    .child(self.toggle_row(cx))
            )
    }

    fn adjust_value(&mut self, idx: usize, dir: f64) {
        let step = self.sliders[idx].step;
        self.sliders[idx].value = (self.sliders[idx].value + dir * step)
            .clamp(self.sliders[idx].min, self.sliders[idx].max);
    }

    fn slider_row(&self, idx: usize, s: &SliderParam, cx: &mut Context<Self>) -> impl IntoElement {
        let ratio = s.ratio();
        let val_str = format_value(s.value, SLIDER_DEFS[idx].3);
        let dec_idx = idx;
        let inc_idx = idx;
        div()
            .h(px(36.0))
            .flex()
            .items_center()
            .gap_2()
            .child(div().w(px(120.0)).text_color(rgb(0xaaaaaa)).text_xs().child(s.label))
            .child(
                btn("-", cx, move |this: &mut Self, cx: &mut Context<Self>| {
                    this.adjust_value(dec_idx, -1.0);
                    cx.notify();
                }),
            )
            .child(
                div()
                    .flex_1()
                    .h(px(20.0))
                    .bg(rgb(0x0f1a2e))
                    .rounded(px(3.0))
                    .relative()
                    .on_scroll_wheel(cx.listener(move |this: &mut Self, e: &ScrollWheelEvent, _w: &mut Window, cx: &mut Context<Self>| {
                        let raw: f64 = e.delta.pixel_delta(px(40.0)).y.into();
                        let steps = (raw / 40.0).round();
                        this.sliders[idx].value = (this.sliders[idx].value + steps * this.sliders[idx].step * 5.0)
                            .clamp(this.sliders[idx].min, this.sliders[idx].max);
                        this.sliders[idx].value = (this.sliders[idx].value / this.sliders[idx].step).round() * this.sliders[idx].step;
                        cx.notify();
                    }))
                    .child(
                        div()
                            .h_full()
                            .w(px((ratio * 100.0) as f32).max(px(4.0)))
                            .bg(rgb(0x88ccff))
                            .rounded(px(3.0))
                            .opacity(0.8),
                    )
            )
            .child(
                btn("+", cx, move |this: &mut Self, cx: &mut Context<Self>| {
                    this.adjust_value(inc_idx, 1.0);
                    cx.notify();
                }),
            )
            .child(div().w(px(50.0)).text_right().text_color(rgb(0x88ccff)).text_xs().font_weight(FontWeight(600.0)).child(val_str))
    }

    fn toggle_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .h(px(36.0))
            .flex()
            .items_center()
            .gap_2()
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, cx.listener(|this: &mut Self, _: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>| {
                this.enabled = !this.enabled;
                cx.notify();
            }))
            .child(
                div()
                    .w(px(40.0))
                    .h(px(22.0))
                    .bg(if self.enabled { rgb(0x44ff88) } else { rgb(0x444444) })
                    .rounded_full()
                    .relative()
                    .child(
                        div()
                            .w(px(18.0))
                            .h(px(18.0))
                            .rounded_full()
                            .bg(rgb(0xffffff))
                            .absolute()
                            .left(if self.enabled { px(20.0) } else { px(2.0) })
                            .top(px(2.0)),
                    ),
            )
            .child(
                div()
                    .text_color(if self.enabled { rgb(0x44ff88) } else { rgb(0x888888) })
                    .text_sm()
                    .child(if self.enabled { "Optimization Enabled" } else { "Optimization Disabled" }),
            )
    }

    fn status_bar(&self) -> impl IntoElement {
        let c = self.current_config();
        div()
            .h(px(28.0))
            .bg(rgb(0x0f1520))
            .flex()
            .items_center()
            .px_4()
            .gap_4()
            .child(
                div().flex().items_center().gap_1()
                    .child(div().w(px(6.0)).h(px(6.0)).rounded_full().bg(rgb(0x44ff88)))
                    .child(div().text_color(rgb(0x666688)).text_xs().child("Daemon ready")),
            )
            .child(
                div().text_color(rgb(0x666688)).text_xs()
                    .child(format!("μ={:.2} ξ={:.2} a={:.1} V={:.0} v={:.2}",
                        c.friction, c.bounce_tension, c.scroll_accel, c.max_velocity, c.min_velocity)),
            )
    }
}

fn btn(label: &'static str, cx: &mut Context<ConfigPanel>, f: impl Fn(&mut ConfigPanel, &mut Context<ConfigPanel>) + 'static) -> impl IntoElement {
    div()
        .w(px(22.0))
        .h(px(22.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(rgb(0x0f3460))
        .rounded(px(3.0))
        .text_color(rgb(0x88ccff))
        .text_xs()
        .font_weight(FontWeight(700.0))
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, cx.listener(move |this: &mut ConfigPanel, _: &MouseDownEvent, _: &mut Window, cx: &mut Context<ConfigPanel>| {
            f(this, cx);
        }))
        .child(label)
}

fn format_value(v: f64, step: f64) -> String {
    if step >= 1.0 { format!("{:.0}", v) }
    else if step >= 0.1 { format!("{:.1}", v) }
    else { format!("{:.2}", v) }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(520.0), px(400.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| ConfigPanel::new()),
        )
        .unwrap();
        cx.activate(true);
    });
}
