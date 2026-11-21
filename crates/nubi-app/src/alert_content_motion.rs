//! Content tween copied numerically from ui/motion.js enter/leave.
use crate::spring::{Preset, Spring, CLOSE, CONTENT};

pub struct ContentMotion {
    pub opacity: Spring,
    pub y: Spring,
    pub scale: Spring,
    pub blur: Spring,
}
impl ContentMotion {
    pub fn hidden() -> Self {
        Self {
            opacity: Spring::new(0.),
            y: Spring::new(-6.),
            scale: Spring::new(0.96),
            blur: Spring::new(6.),
        }
    }
    pub fn enter(&mut self) {
        *self = Self::hidden();
        self.opacity.to(
            1.,
            Preset {
                response: 0.3,
                damping: 1.,
            },
        );
        self.y.to(0., CONTENT);
        self.scale.to(1., CONTENT);
        self.blur.to(
            0.,
            Preset {
                response: 0.3,
                damping: 1.,
            },
        );
    }
    pub fn leave(&mut self) {
        self.opacity = Spring::new(1.);
        self.y = Spring::new(0.);
        self.scale = Spring::new(1.);
        self.blur = Spring::new(0.);
        self.opacity.to(
            0.,
            Preset {
                response: 0.252,
                damping: 1.,
            },
        );
        self.y.to(-4., CLOSE);
        self.scale.to(0.96, CLOSE);
        self.blur.to(
            6.,
            Preset {
                response: 0.252,
                damping: 1.,
            },
        );
    }
    pub fn step(&mut self, dt: f64) {
        for spring in [
            &mut self.opacity,
            &mut self.y,
            &mut self.scale,
            &mut self.blur,
        ] {
            spring.step(dt);
        }
    }
    pub fn jump(&mut self) {
        for spring in [
            &mut self.opacity,
            &mut self.y,
            &mut self.scale,
            &mut self.blur,
        ] {
            spring.jump();
        }
    }
}
