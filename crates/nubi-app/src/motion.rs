use crate::{native, shape, spring, NotchWindow};
use slint::ComponentHandle;
use std::time::Instant;
pub struct Motion {
    springs: [spring::Spring; 4],
    target: [f64; 4],
    moving: bool,
    last: Instant,
    content: spring::Spring,
    view: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

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
        }
    }
    pub fn target(&mut self, view: i32, target: [f64; 4], reduced: bool) {
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
        true
    }
    pub fn draw(&self, ui: &NotchWindow) {
        // Render and hit-test the same physical-pixel-snapped shape.
        let scale = ui.window().scale_factor() as f64;
        let values = self
            .springs
            .map(|spring| (spring.x * scale).round() / scale);
        ui.set_along(values[0] as f32);
        ui.set_depth(values[1] as f32);
        ui.set_corner(values[2] as f32);
        ui.set_ear(values[3] as f32);
        ui.set_opened(self.view == 3);
        ui.set_view_mode(self.view);
        ui.set_content_opacity(self.content.x.clamp(0., 1.) as f32);
        native::set_shape(shape::Shape(values));
    }
}
