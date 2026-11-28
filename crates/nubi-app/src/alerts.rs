//! Ordinary side/bottom alert queue from ui/overlay.js; no approval-card migration.
use crate::{
    NativeAlert, Theme,
    alert_popup::Popup,
    host::{NativeHost, UiEvent},
    singleton::Action,
    spring::{self, Preset, Spring},
};
use nubi_core::{host::with, state::Alert};
use slint::ComponentHandle;
use std::{
    collections::{BTreeSet, VecDeque},
    time::Instant,
};

#[derive(Default)]
struct Queue {
    ready: VecDeque<Alert>,
    delayed: Vec<(u64, Alert)>,
    current: Option<u64>,
    next_at: u64,
}
enum Change {
    Show(Alert),
    Close,
    None,
}
impl Queue {
    fn push(&mut self, alert: Alert, now: u64) {
        if matches!(alert.kind.as_str(), "permission" | "question") {
            self.delayed.push((now + 400, alert));
        } else {
            self.ready.push_back(alert);
        }
    }
    fn tick(&mut self, now: u64, pending: &BTreeSet<String>) -> Change {
        let mut retained = Vec::new();
        for (due, alert) in self.delayed.drain(..) {
            if now < due {
                retained.push((due, alert));
            } else if !alert
                .session_key
                .as_ref()
                .is_some_and(|key| pending.contains(key))
            {
                self.ready.push_back(alert);
            }
        }
        self.delayed = retained;
        if self
            .current
            .is_some_and(|since| now.saturating_sub(since) >= 4500)
        {
            self.current = None;
            self.next_at = now + 600;
            return Change::Close;
        }
        if self.current.is_none() && now >= self.next_at {
            if let Some(alert) = self.ready.pop_front() {
                self.current = Some(now);
                return Change::Show(alert);
            }
        }
        Change::None
    }
}

// NativePopup must be dropped before its Slint window.
struct Surface {
    popup: Option<Popup>,
    ui: NativeAlert,
}
impl Drop for Surface {
    fn drop(&mut self) {
        // Slint retains shown windows internally. Hide before dropping the owned handle.
        let _ = self.ui.hide();
    }
}
pub struct Alerts {
    host: NativeHost,
    queue: Queue,
    surface: Option<Surface>,
    clock: Instant,
    last: Instant,
    shape: [Spring; 4],
    content: crate::alert_content_motion::ContentMotion,
    content_images: crate::alert_content::ContentImages,
    capture_needed: bool,
    content_at: u64,
    icon_since: u64,
    closing: Option<u64>,
}
impl Alerts {
    pub fn new(host: NativeHost) -> Self {
        Self {
            host,
            queue: Queue::default(),
            surface: None,
            clock: Instant::now(),
            last: Instant::now(),
            shape: [120., 0., 0.8, 0.].map(Spring::new),
            content: crate::alert_content_motion::ContentMotion::hidden(),
            content_images: Default::default(),
            capture_needed: true,
            content_at: u64::MAX,
            icon_since: u64::MAX,
            closing: None,
        }
    }
    pub fn push(&mut self, alert: Alert) {
        self.queue
            .push(alert, self.clock.elapsed().as_millis() as u64);
    }
    fn show(&mut self, alert: Alert, now: u64) -> Result<(), String> {
        if self.surface.is_none() {
            let ui = NativeAlert::new().map_err(|e| e.to_string())?;
            let host = self.host.clone();
            ui.on_open_settings(move || {
                let _ = host.post(UiEvent::Activate(Action::Settings));
            });
            self.surface = Some(Surface { popup: None, ui });
        }
        let ui = &self.surface.as_ref().ok_or("alert surface unavailable")?.ui;
        let settings = with(&self.host, |state| state.settings.clone());
        crate::ui::apply_theme(ui.global::<Theme>(), &settings.notch_theme);
        ui.global::<Theme>()
            .set_notch_accent(settings.notch_accent.as_str().into());
        ui.global::<Theme>()
            .set_reduced(settings.reduce_motion || crate::native::system_reduced_motion());
        ui.set_heading(
            format!(
                "{} · {}",
                if alert.provider == "codex" {
                    "Codex"
                } else {
                    "Claude"
                },
                alert.title
            )
            .into(),
        );
        ui.set_message(crate::alert_text::card_body(&alert.body).into());
        ui.set_kind(alert.kind.into());
        if let Some(popup) = &self
            .surface
            .as_ref()
            .ok_or("alert surface unavailable")?
            .popup
        {
            let (width, height, scale) = popup.place(settings.monitor)?;
            ui.global::<Theme>().set_scale(scale);
            ui.window()
                .set_size(slint::PhysicalSize::new(width, height));
        }
        self.last = Instant::now();
        let fade = Preset {
            response: 0.3,
            damping: 1.,
        };
        for (spring, target) in
            self.shape
                .iter_mut()
                .zip([
                    f64::from(ui.get_natural_width()),
                    f64::from(ui.get_natural_height()),
                    1.,
                    1.,
                ])
        {
            spring.to(target, spring::OPEN);
        }
        self.shape[3].to(1., fade);
        self.content = crate::alert_content_motion::ContentMotion::hidden();
        self.capture_needed = true;
        self.content_at = now + 100;
        self.icon_since = now + 100;
        self.closing = None;
        ui.show().map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn tick(&mut self) -> Result<(), String> {
        if self.surface.is_none() && self.queue.ready.is_empty() && self.queue.delayed.is_empty() {
            return Ok(());
        }
        let now = self.clock.elapsed().as_millis() as u64;
        let pending = with(&self.host, |state| {
            state
                .pending
                .values()
                .map(|item| item.session_key.clone())
                .collect()
        });
        match self.queue.tick(now, &pending) {
            Change::Show(alert) => {
                if let Err(error) = self.show(alert.clone(), now) {
                    self.queue.current = None;
                    self.queue.next_at = now + 1000;
                    self.queue.ready.push_front(alert);
                    return Err(error);
                }
            }
            Change::Close => {
                for (spring, target) in self.shape.iter_mut().zip([120., 0., 0.8, 0.]) {
                    spring.to(target, spring::CLOSE);
                }
                self.shape[3].to(
                    0.,
                    Preset {
                        response: 0.3,
                        damping: 1.,
                    },
                );
                self.content.leave();
                self.content_at = u64::MAX;
                self.closing = Some(now);
            }
            Change::None => {}
        }
        if let Some(surface) = self.surface.as_mut() {
            if surface.popup.is_none() {
                // Winit supplies the native handle on a later event-loop turn. Keep
                // this same invisible surface while waiting; never retry by creating windows.
                let Some(handle) = crate::ui::hwnd(surface.ui.window()) else {
                    self.last = Instant::now();
                    return Ok(());
                };
                let popup = Popup::new(handle)?;
                let monitor = with(&self.host, |state| state.settings.monitor);
                let (width, height, scale) = popup.place(monitor)?;
                surface.ui.global::<Theme>().set_scale(scale);
                surface
                    .ui
                    .window()
                    .set_size(slint::PhysicalSize::new(width, height));
                self.shape[0].to(f64::from(surface.ui.get_natural_width()), spring::OPEN);
                self.shape[1].to(f64::from(surface.ui.get_natural_height()), spring::OPEN);
                surface.popup = Some(popup);
                self.last = Instant::now();
            }
        }
        if self.capture_needed {
            if let Some(surface) = &self.surface {
                self.content_images.prepare(&surface.ui)?;
                let ready = self.clock.elapsed().as_millis() as u64;
                self.content_at = ready + 100;
                self.icon_since = ready + 100;
                self.queue.current = Some(ready);
                self.last = Instant::now();
                self.capture_needed = false;
            }
        }
        let instant = Instant::now();
        let dt = instant.duration_since(self.last).as_secs_f64();
        self.last = instant;
        if now >= self.content_at {
            self.content.enter();
            self.content_at = u64::MAX;
        }
        if let Some(surface) = &self.surface {
            let settings = with(&self.host, |state| state.settings.clone());
            let reduced = settings.reduce_motion || crate::native::system_reduced_motion();
            for spring in &mut self.shape {
                if reduced {
                    spring.jump();
                } else {
                    spring.step(dt);
                }
            }
            if reduced {
                self.content.jump();
            } else {
                self.content.step(dt);
            }
            let ui = &surface.ui;
            ui.set_pill_width(self.shape[0].x as f32);
            ui.set_pill_height(self.shape[1].x as f32);
            ui.set_pill_scale(self.shape[2].x as f32);
            ui.set_pill_opacity(self.shape[3].x as f32);
            ui.set_icon_progress(if reduced {
                1.
            } else {
                icon_progress(now.saturating_sub(self.icon_since) as f64 / 300.) as f32
            });
            ui.set_content_opacity(self.content.opacity.x as f32);
            ui.set_content_y(self.content.y.x as f32);
            ui.set_content_scale(self.content.scale.x as f32);
            self.content_images.draw(ui, self.content.blur.x);
            if let Some(popup) = &surface.popup {
                popup.update(if self.queue.current.is_some() {
                    [
                        f64::from(ui.get_hit_x()),
                        f64::from(ui.get_hit_y()),
                        f64::from(ui.get_hit_width()),
                        f64::from(ui.get_hit_height()),
                    ]
                } else {
                    [0.; 4]
                });
            }
            if self
                .closing
                .is_some_and(|since| now.saturating_sub(since) >= 800)
                && self.queue.current.is_none()
                && self.queue.ready.is_empty()
            {
                ui.hide().map_err(|e| e.to_string())?;
                self.surface = None;
                self.closing = None;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn alert(kind: &str) -> Alert {
        Alert {
            ts: 0,
            provider: "claude".into(),
            kind: kind.into(),
            title: "Done".into(),
            body: "Example".into(),
            session_key: Some("claude:one".into()),
            queued: false,
        }
    }
    #[test]
    fn original_overlay_duration_gap_and_request_suppression() {
        let mut queue = Queue::default();
        let pending = BTreeSet::from(["claude:one".into()]);
        queue.push(alert("permission"), 0);
        assert!(matches!(queue.tick(399, &pending), Change::None));
        assert!(matches!(queue.tick(400, &pending), Change::None));
        queue.push(alert("done"), 500);
        queue.push(alert("info"), 501);
        assert!(matches!(queue.tick(500, &pending), Change::Show(_)));
        assert!(matches!(queue.tick(4999, &pending), Change::None));
        assert!(matches!(queue.tick(5000, &pending), Change::Close));
        assert!(matches!(queue.tick(5599, &pending), Change::None));
        assert!(matches!(queue.tick(5600, &pending), Change::Show(_)));
    }
    #[test]
    fn resolved_request_alerts_wait_400_ms_and_normal_alerts_can_overtake() {
        let mut queue = Queue::default();
        let pending = BTreeSet::new();
        queue.push(alert("question"), 0);
        queue.push(alert("done"), 1);
        assert!(matches!(queue.tick(1,&pending),Change::Show(a) if a.kind=="done"));
        queue.tick(400, &pending);
        assert_eq!(queue.ready.front().unwrap().kind, "question");
    }
}

#[cfg(test)]
mod render_tests {
    use super::*;
    use slint::platform::{
        Platform, PlatformError, WindowAdapter, WindowEvent,
        software_renderer::{MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType},
    };
    use std::rc::Rc;
    struct Offscreen(Rc<MinimalSoftwareWindow>);
    impl Platform for Offscreen {
        fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
            Ok(self.0.clone())
        }
    }
    #[test]
    fn deferred_native_handle_retains_one_invisible_surface() {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        slint::platform::set_platform(Box::new(Offscreen(window))).unwrap();
        let (host, _) = NativeHost::from_settings(Default::default());
        let mut manager = Alerts::new(host);
        manager.push(Alert {
            ts: 0,
            provider: "claude".into(),
            kind: "done".into(),
            title: "Done".into(),
            body: "Example".into(),
            session_key: None,
            queued: false,
        });
        for _ in 0..20 {
            manager.tick().unwrap();
        }
        assert!(manager.surface.is_some());
        assert!(manager.surface.as_ref().unwrap().popup.is_none());
        assert!(manager.queue.ready.is_empty());
        assert!(manager.queue.current.is_some());
        assert_eq!(manager.surface.as_ref().unwrap().ui.get_pill_opacity(), 0.);
    }
    #[test]
    fn ordinary_alert_card_wraps_at_four_scales() {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        slint::platform::set_platform(Box::new(Offscreen(window.clone()))).unwrap();
        let ui = NativeAlert::new().unwrap();
        ui.set_heading("Claude · Task complete".into());
        let message = "The native alert card is ready for comparison. This deliberately long message exercises content wrapping within the toast's maximum width.";
        ui.set_message(message.into());
        ui.set_kind("done".into());
        ui.set_pill_scale(1.);
        ui.set_pill_opacity(1.);
        ui.set_content_opacity(1.);
        ui.show().unwrap();
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/native/alerts/renders");
        std::fs::create_dir_all(&folder).unwrap();
        for dark in [true, false] {
            ui.global::<Theme>().set_dark(dark);
            for percent in [100, 125, 150, 175] {
                let scale = percent as f32 / 100.;
                window.dispatch_event(WindowEvent::ScaleFactorChanged {
                    scale_factor: scale,
                });
                ui.global::<Theme>().set_scale(scale);
                let (width, height) = ((720. * scale) as u32, (160. * scale) as u32);
                window.set_size(slint::PhysicalSize::new(width, height));
                let natural_height = ui.get_natural_height();
                assert_eq!(ui.get_natural_width(), 320.);
                assert!(natural_height > 56., "long alert must grow when text wraps");
                ui.set_message(format!("{message}\nExtra lines must be elided.\nThis line must also be elided.").into());
                assert_eq!(ui.get_natural_height(), natural_height);
                ui.set_message(message.into());
                ui.set_pill_width(ui.get_natural_width());
                ui.set_pill_height(natural_height);
                assert_eq!(ui.get_hit_x(), 200.);
                assert_eq!(ui.get_hit_y(), 12.);
                assert_eq!(ui.get_hit_width(), 320.);
                assert_eq!(ui.get_hit_height(), natural_height);
                let mut pixels =
                    vec![PremultipliedRgbaColor::default(); width as usize * height as usize];
                window.request_redraw();
                assert!(window.draw_if_needed(|renderer| {
                    renderer.render(&mut pixels, width as usize);
                }));
                let mut image = image::RgbImage::new(width, height);
                let mut opaque = 0;
                for (index, pixel) in pixels.iter().enumerate() {
                    opaque += usize::from(pixel.alpha == 255);
                    let background = ((255 - u16::from(pixel.alpha)) * 96 / 255) as u8;
                    image.put_pixel(
                        index as u32 % width,
                        index as u32 / width,
                        image::Rgb([
                            pixel.red.saturating_add(background),
                            pixel.green.saturating_add(background),
                            pixel.blue.saturating_add(background),
                        ]),
                    );
                }
                assert!(opaque > (320. * natural_height * scale * scale * 0.7) as usize);
                use image::ImageEncoder;
                let file = std::fs::File::create(folder.join(format!(
                    "{}-{percent}.png",
                    if dark { "dark" } else { "light" }
                )))
                .unwrap();
                image::codecs::png::PngEncoder::new(file)
                    .write_image(
                        image.as_raw(),
                        width,
                        height,
                        image::ExtendedColorType::Rgb8,
                    )
                    .unwrap();
            }
        }
        ui.hide().unwrap();
    }
}

// Invert the original CSS cubic-bezier(.16, 1, .3, 1).
fn icon_progress(time: f64) -> f64 {
    let time = time.clamp(0., 1.);
    let (mut low, mut high) = (0., 1.);
    for _ in 0..32 {
        let t = (low + high) / 2.;
        let x = 3. * (1. - t) * (1. - t) * t * 0.16 + 3. * (1. - t) * t * t * 0.3 + t * t * t;
        if x < time {
            low = t;
        } else {
            high = t;
        }
    }
    1. - (1. - (low + high) / 2.).powi(3)
}
