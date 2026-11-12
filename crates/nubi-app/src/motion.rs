use crate::{native, shape, spring, NotchWindow};
use std::time::Instant;
pub struct Motion {
    springs: [spring::Spring; 4],
    opened: bool,
    moving: bool,
    last: Instant,
    open_depth: f64,
}

impl Motion {
    pub fn new(opened: bool) -> Self {
        let values = if opened { shape::OPENED } else { shape::CLOSED };
        Self {
            springs: values.map(spring::Spring::new),
            opened,
            moving: false,
            last: Instant::now(),
            open_depth: shape::OPENED[1],
        }
    }

    pub fn target(&mut self, opened: bool, reduced: bool) {
        if opened == self.opened {
            return;
        }
        let mut target = if opened { shape::OPENED } else { shape::CLOSED };
        if opened {
            target[1] = self.open_depth;
        }
        let growing = target[0] * target[1] > self.springs[0].target * self.springs[1].target + 1.0;
        let preset = if growing { spring::OPEN } else { spring::CLOSE };
        for (spring, value) in self.springs.iter_mut().zip(target) {
            spring.to(value, preset);
            if reduced {
                spring.jump();
            }
        }
        // motion.js kick() preserves the running frame clock during a retarget.
        if !self.moving {
            self.last = Instant::now();
        }
        self.opened = opened;
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
        true
    }

    pub fn tab(&mut self, tab: i32, reduced: bool) {
        self.open_depth = if tab == 2 { 180.0 } else { 188.0 };
        if self.opened {
            self.springs[1].to(self.open_depth, spring::OPEN);
            if reduced {
                self.springs[1].jump();
            }
            if !self.moving {
                self.last = Instant::now();
            }
            self.moving = true;
        }
    }

    pub fn draw(&self, ui: &NotchWindow) {
        let values = self.springs.map(|s| s.x);
        ui.set_along(values[0] as f32);
        ui.set_depth(values[1] as f32);
        ui.set_corner(values[2] as f32);
        ui.set_ear(values[3] as f32);
        ui.set_opened(self.opened);
        native::set_shape(shape::Shape(values));
    }
}
