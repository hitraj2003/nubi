use crate::{native, shape, spring, NotchWindow};
use slint::ComponentHandle;
use std::{cell::Cell, time::Instant};
pub struct Motion {
    springs: [spring::Spring; 4],
    target: [f64; 4],
    moving: bool,
    last: Instant,
    content: spring::Spring,
    view: i32,
    tabs: [spring::Spring; 2],
    tabs_initialized: bool,
    side_anchor: Cell<Option<(i32, f64, f64, f64)>>,
}

impl Motion {
    pub fn new() -> Self {
        Self {
            springs: shape::CLOSED.map(spring::Spring::new),
            target: shape::CLOSED,
            moving: false,
            last: Instant::now(),
            content: spring::Spring::new(1.),
            view: 1,
            tabs: [spring::Spring::new(0.); 2],
            tabs_initialized: false,
            side_anchor: Cell::new(None),
        }
    }
    pub fn target(&mut self, view: i32, target: [f64; 4], reduced: bool) {
        if reduced {
            for spring in &mut self.tabs {
                spring.jump();
            }
        }
        if target == self.target && view == self.view {
            if reduced && self.moving {
                for spring in &mut self.springs {
                    spring.jump();
                }
                self.content.jump();
            }
            return;
        }
        let growing = target[0] * target[1] > self.target[0] * self.target[1] + 1.;
        for (spring, value) in self.springs.iter_mut().zip(target) {
            spring.to(value, if growing { spring::OPEN } else { spring::CLOSE });
            if reduced {
                spring.jump();
            }
        }
        if view != self.view {
            self.content.x = if reduced { 1. } else { 0. };
            self.content.to(1., spring::CONTENT);
            if reduced {
                self.content.jump();
            }
        }
        if !self.moving {
            self.last = Instant::now();
        }
        self.target = target;
        self.view = view;
        self.moving = true;
    }
    pub fn tab_target(&mut self, x: f64, width: f64, reduced: bool) {
        if self.tabs_initialized && self.tabs.map(|spring| spring.target) == [x, width] {
            return;
        }
        for (spring, value) in self.tabs.iter_mut().zip([x, width]) {
            spring.to(value, spring::SNAPPY);
            if reduced || !self.tabs_initialized {
                spring.jump();
            }
        }
        self.tabs_initialized = true;
        if !self.moving {
            self.last = Instant::now();
        }
        self.moving = true;
    }
    pub fn active(&self) -> bool {
        self.moving
    }
    pub fn tick(&mut self, now: Instant) -> bool {
        if !self.moving {
            return false;
        }
        let dt = now.saturating_duration_since(self.last).as_secs_f64();
        self.last = now;
        self.moving = false;
        for spring in &mut self.springs {
            self.moving |= spring.step(dt);
        }
        self.moving |= self.content.step(dt);
        for spring in &mut self.tabs {
            self.moving |= spring.step(dt);
        }
        true
    }
    fn side_expansion(&self, edge: i32, placement: f64, span: f64) -> f64 {
        let expansion = if edge >= 2 && self.view == 3 {
            let top = match self.side_anchor.get() {
                Some((saved_edge, saved_placement, saved_span, top))
                    if saved_edge == edge && saved_placement == placement && saved_span == span =>
                {
                    top
                }
                _ => {
                    let top = (span - self.target[0]) / 2. + placement;
                    self.side_anchor.set(Some((edge, placement, span, top)));
                    top
                }
            };
            shape::bottom_first_offset(top, self.springs[0].x, self.springs[3].x, span, placement)
        } else {
            self.side_anchor.set(None);
            0.
        };
        if let Some((edge, placement, span, top)) = self.side_anchor.get() {
            // Keep any upward boundary correction when a shorter tab follows.
            let visible_top = span / 2. + placement + expansion - self.springs[0].x / 2.;
            self.side_anchor
                .set(Some((edge, placement, span, top.min(visible_top))));
        }
        expansion
    }
    pub fn draw(&self, ui: &NotchWindow) {
        // Preserve subpixel movement. The path and native hit test share these
        // values; rounding every frame freezes the slow part of the spring.
        let values = self.springs.map(|spring| spring.x);
        let expansion = self.side_expansion(
            ui.get_edge(),
            f64::from(ui.get_offset()),
            f64::from(ui.window().size().height) / f64::from(ui.window().scale_factor()),
        );
        ui.set_expansion_offset(expansion as f32);
        native::set_expansion_offset(expansion);
        ui.set_along(values[0] as f32);
        ui.set_depth(values[1] as f32);
        ui.set_corner(values[2] as f32);
        ui.set_ear(values[3] as f32);
        ui.set_opened(self.view == 3);
        ui.set_view_mode(self.view);
        ui.set_content_opacity(self.content.x.clamp(0., 1.) as f32);
        ui.set_tab_x(self.tabs[0].x as f32);
        ui.set_tab_width(self.tabs[1].x as f32);
        native::set_shape(shape::Shape(values));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn top(motion: &Motion, edge: i32, placement: f64, span: f64) -> f64 {
        span / 2. + placement + motion.side_expansion(edge, placement, span)
            - motion.springs[0].x / 2.
    }

    #[test]
    fn side_tabs_keep_header_and_pointer_fixed_through_animated_reversals() {
        for edge in [2, 3] {
            for scale in [1., 1.25, 1.5, 1.75] {
                let span = 1040. / scale;
                let mut motion = Motion::new();
                motion.target(3, [188., 360., 32., 20.], true);
                let expected = top(&motion, edge, -80., span);
                for height in [300., 110., 420., 188.] {
                    motion.target(3, [height, 360., 32., 20.], false);
                    let start = motion.last;
                    for frame in 1..=30 {
                        motion
                            .tick(start + std::time::Duration::from_secs_f64(frame as f64 / 120.));
                        let actual = top(&motion, edge, -80., span);
                        assert!(
                            (actual - expected).abs() < 1e-8,
                            "edge {edge}, scale {scale}"
                        );
                        let scene = shape::Scene {
                            edge,
                            offset: -80. + motion.side_expansion(edge, -80., span),
                            width: 420.,
                            height: span,
                        };
                        let x = if edge == 2 { 100. } else { 320. };
                        assert!(scene.contains(
                            shape::Shape(motion.springs.map(|s| s.x)),
                            x,
                            expected + 28.
                        ));
                    }
                }
            }
        }
    }

    #[test]
    fn side_boundary_correction_does_not_reverse_when_tab_shrinks() {
        for edge in [2, 3] {
            let mut motion = Motion::new();
            motion.target(3, [188., 360., 32., 20.], true);
            let original = top(&motion, edge, 300., 860.);
            motion.target(3, [500., 360., 32., 20.], true);
            let corrected = top(&motion, edge, 300., 860.);
            assert!(corrected < original);
            assert!((corrected + 500. - 840.).abs() < 1e-8);
            motion.target(3, [110., 360., 32., 20.], true);
            assert!((top(&motion, edge, 300., 860.) - corrected).abs() < 1e-8);
            motion.target(1, shape::CLOSED, true);
            assert_eq!(motion.side_expansion(edge, 300., 860.), 0.);
            assert!(motion.side_anchor.get().is_none());
            motion.target(3, [188., 360., 32., 20.], true);
            assert!((top(&motion, edge, 0., 860.) - 336.).abs() < 1e-8);
            assert_eq!(motion.side_expansion(0, 0., 860.), 0.);
            assert!(motion.side_anchor.get().is_none());
        }
    }

    #[test]
    fn enabling_reduced_motion_settles_an_active_transition() {
        let mut motion = Motion::new();
        motion.target(3, shape::OPENED, false);
        motion.tick(Instant::now() + std::time::Duration::from_millis(32));
        assert_ne!(motion.springs[0].x, shape::OPENED[0]);
        motion.target(3, shape::OPENED, true);
        assert_eq!(motion.springs.map(|spring| spring.x), shape::OPENED);
        assert_eq!(motion.content.x, 1.);
    }

    #[test]
    fn tab_reversal_preserves_velocity_and_reduced_motion_settles_it() {
        let mut motion = Motion::new();
        motion.tab_target(0., 60., false);
        motion.tab_target(160., 80., false);
        let start = motion.last;
        motion.tick(start + std::time::Duration::from_millis(48));
        let velocity = motion.tabs[0].v;
        assert!(velocity > 0.);
        motion.tab_target(0., 60., false);
        assert_eq!(motion.tabs[0].v, velocity);
        motion.target(3, shape::OPENED, true);
        assert_eq!(motion.tabs.map(|spring| spring.x), [0., 60.]);
        motion.tick(start + std::time::Duration::from_millis(64));
        assert!(!motion.active());
    }

    #[test]
    fn spring_tail_keeps_subpixel_motion_until_settled() {
        let mut motion = Motion::new();
        motion.target(3, shape::OPENED, false);
        let start = motion.last;
        let mut changed = 0;
        let mut rounded_duplicates = 0;
        for frame in 1..=120 {
            let previous = motion.springs[0].x;
            motion.tick(start + std::time::Duration::from_secs_f64(frame as f64 / 120.));
            let current = motion.springs[0].x;
            changed += usize::from(current != previous);
            rounded_duplicates += usize::from(
                current != previous && (current * 1.25).round() == (previous * 1.25).round(),
            );
        }
        assert!(changed > 110);
        assert!(
            rounded_duplicates > 40,
            "physical-pixel rounding hides the slow spring tail"
        );
    }
}
