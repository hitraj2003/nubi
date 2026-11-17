//! The top-edge cubic outline from ui/island.js (k = 0.62).
pub const WINDOW_WIDTH: f64 = 720.0;
pub const WINDOW_HEIGHT: f64 = 700.0;
pub const CLOSED: [f64; 4] = [145.0, 44.0, 16.0, 8.0];
// #v-open: 12px top + 32px bar + 8px gap + three 40px rows + 16px bottom.
#[cfg(test)]
pub const OPENED: [f64; 4] = [620.0, 188.0, 32.0, 20.0];

#[derive(Clone, Copy, Debug)]
pub struct Shape(pub [f64; 4]);

#[derive(Clone, Copy, Default)]
pub struct Scene {
    pub edge: i32,
    pub offset: f64,
    pub width: f64,
    pub height: f64,
}
impl Scene {
    pub fn contains(self, shape: Shape, x: f64, y: f64) -> bool {
        let [along, depth, corner, ear] = shape.0;
        let radius = corner.min(depth / 2.).min(along / 2.);
        let ear = ear.min((depth - radius).max(0.));
        let vertical = self.edge >= 2;
        let span = if vertical { self.height } else { self.width };
        let room = ((span - along - 2. * ear) / 2.).max(0.);
        let offset = self.offset.clamp(-room, room);
        let (u, v) = match self.edge {
            0 => (x, y),
            1 => (x, self.height - y),
            2 => (y, x),
            _ => (y, self.width - x),
        };
        v <= depth && shape.contains(u - span / 2. - offset + WINDOW_WIDTH / 2., v)
    }
}

impl Shape {
    /// Clockwise polygon, shared by hit testing and native validation.
    /// Rendering uses the original cubics, not this approximation.
    pub fn polygon(self) -> Vec<(f64, f64)> {
        let [a, d, r, e] = self.0;
        let d = d.max(1.0);
        let r = r.min(d / 2.0).min(a / 2.0);
        let e = e.min((d - r).max(0.0));
        let width = a + 2.0 * e;
        let origin = (WINDOW_WIDTH - width) / 2.0;
        let k = 0.62;
        let mut points = vec![(0.0, 0.0), (width, 0.0)];
        let mut cubic = |start: (f64, f64), c1: (f64, f64), c2: (f64, f64), end: (f64, f64)| {
            points.push(start);
            for i in 1..=24 {
                let t = f64::from(i) / 24.0;
                let u = 1.0 - t;
                points.push((
                    u.powi(3) * start.0
                        + 3.0 * u * u * t * c1.0
                        + 3.0 * u * t * t * c2.0
                        + t.powi(3) * end.0,
                    u.powi(3) * start.1
                        + 3.0 * u * u * t * c1.1
                        + 3.0 * u * t * t * c2.1
                        + t.powi(3) * end.1,
                ));
            }
        };
        cubic(
            (width, 0.0),
            (width - e * k, 0.0),
            (width - e, e * (1.0 - k)),
            (width - e, e),
        );
        cubic(
            (width - e, d - r),
            (width - e, d - r * (1.0 - k)),
            (width - e - r * (1.0 - k), d),
            (width - e - r, d),
        );
        cubic(
            (e + r, d),
            (e + r * (1.0 - k), d),
            (e, d - r * (1.0 - k)),
            (e, d - r),
        );
        cubic((e, e), (e, e * (1.0 - k)), (e * k, 0.0), (0.0, 0.0));
        points.into_iter().map(|(x, y)| (x + origin, y)).collect()
    }

    pub fn contains(self, x: f64, y: f64) -> bool {
        if y < 0.0 || y >= self.0[1] {
            return false;
        }
        let polygon = self.polygon();
        let mut inside = false;
        let mut previous = *polygon.last().unwrap();
        for point in polygon {
            if (point.1 > y) != (previous.1 > y)
                && x < (previous.0 - point.0) * (y - point.1) / (previous.1 - point.1) + point.0
            {
                inside = !inside;
            }
            previous = point;
        }
        inside
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transparent_margins_and_curved_corners_do_not_intercept() {
        let shape = Shape(OPENED);
        assert!(shape.contains(360.0, 100.0));
        assert!(shape.contains(31.0, 0.0)); // Concave ear at screen edge.
        assert!(!shape.contains(0.0, 0.0));
        assert!(!shape.contains(51.0, 187.0)); // Bottom left rounded corner.
        assert!(!shape.contains(360.0, 200.0));
        assert!(!Shape(CLOSED).contains(200.0, 20.0));
    }

    #[test]
    fn four_edges_map_hit_tests_at_negative_monitor_origins_and_offsets() {
        for edge in 0..4 {
            let (width, height) = if edge >= 2 {
                (420., 860.)
            } else {
                (720., 700.)
            };
            for offset in [-500., -50., 0., 50., 500.] {
                let scene = Scene {
                    edge,
                    width,
                    height,
                    offset,
                };
                let span = if edge >= 2 { height } else { width };
                let shift = offset.clamp(-(span - 161.) / 2., (span - 161.) / 2.);
                let u = span / 2. + shift;
                let point = match edge {
                    0 => (u, 20.),
                    1 => (u, height - 20.),
                    2 => (20., u),
                    _ => (width - 20., u),
                };
                assert!(scene.contains(Shape(CLOSED), point.0, point.1));
                let outside = match edge {
                    0 => (u, 50.),
                    1 => (u, height - 50.),
                    2 => (50., u),
                    _ => (width - 50., u),
                };
                assert!(!scene.contains(Shape(CLOSED), outside.0, outside.1));
            }
        }
    }
}
