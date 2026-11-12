//! Numerical port of ui/motion.js, including velocity-preserving retargets.
use std::f64::consts::TAU;

#[derive(Clone, Copy, Debug)]
pub struct Preset {
    pub response: f64,
    pub damping: f64,
}

pub const OPEN: Preset = Preset {
    response: 0.5,
    damping: 0.75,
};
pub const CLOSE: Preset = Preset {
    response: 0.42,
    damping: 0.9,
};

#[derive(Clone, Copy, Debug)]
pub struct Spring {
    pub x: f64,
    pub v: f64,
    pub target: f64,
    k: f64,
    c: f64,
}

impl Spring {
    pub fn new(value: f64) -> Self {
        let mut spring = Self {
            x: value,
            v: 0.0,
            target: value,
            k: 0.0,
            c: 0.0,
        };
        spring.to(value, OPEN);
        spring
    }

    pub fn to(&mut self, target: f64, preset: Preset) {
        let w = TAU / preset.response;
        self.k = w * w;
        self.c = 2.0 * preset.damping * w;
        self.target = target;
    }

    pub fn jump(&mut self) {
        self.x = self.target;
        self.v = 0.0;
    }

    pub fn step(&mut self, dt: f64) -> bool {
        let mut t = dt.min(0.064);
        while t > 0.0 {
            let h = t.min(0.004);
            let acceleration = -self.k * (self.x - self.target) - self.c * self.v;
            self.v += acceleration * h;
            self.x += self.v * h;
            t -= h;
        }
        if self.v.abs() < 0.01 && (self.x - self.target).abs() < 0.01 {
            self.jump();
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reversal_preserves_velocity_and_settles() {
        let mut spring = Spring::new(180.0);
        spring.to(620.0, OPEN);
        for _ in 0..6 {
            spring.step(0.016);
        }
        let velocity = spring.v;
        assert!(velocity > 0.0);
        spring.to(180.0, CLOSE);
        assert_eq!(spring.v, velocity);
        for _ in 0..200 {
            spring.step(0.016);
        }
        assert_eq!(spring.x, 180.0);
        assert_eq!(spring.v, 0.0);
    }

    #[test]
    fn late_frames_are_capped_at_64_ms() {
        let mut late = Spring::new(44.0);
        late.to(188.0, OPEN);
        let mut capped = late;
        late.step(1.0);
        capped.step(0.064);
        assert_eq!(late.x, capped.x);
        assert_eq!(late.v, capped.v);
    }
}
