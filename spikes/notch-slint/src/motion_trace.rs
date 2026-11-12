// Harness for comparison against the actual repository motion.js engine.
mod spring;

fn main() {
    let mut spring = spring::Spring::new(180.0);
    spring.to(620.0, spring::OPEN);
    for frame in 0..240 {
        if frame == 6 { spring.to(180.0, spring::CLOSE); }
        if frame == 10 { spring.to(620.0, spring::OPEN); }
        if frame == 70 { spring.to(180.0, spring::CLOSE); }
        let dt = [0.016, 0.008, 0.033, 0.100][frame % 4];
        let moving = spring.step(dt);
        println!("{frame},{:.17},{:.17},{moving}", spring.x, spring.v);
    }
}
