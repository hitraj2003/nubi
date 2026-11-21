//! Original island.css eq keyframes: 3px -> 12px -> 3px, ease-in-out.
pub fn height(phase: f32, bar: i32) -> f32 {
    let progress = (f64::from(phase) + f64::from(bar.clamp(0, 2)) * 0.33).rem_euclid(1.);
    let half = if progress <= 0.5 {
        progress * 2.
    } else {
        (1. - progress) * 2.
    };
    // Invert cubic-bezier(.42, 0, .58, 1) on the time axis.
    let (mut low, mut high) = (0., 1.);
    for _ in 0..40 {
        let t = (low + high) / 2.;
        let x = 3. * (1. - t) * (1. - t) * t * 0.42 + 3. * (1. - t) * t * t * 0.58 + t * t * t;
        if x < half {
            low = t;
        } else {
            high = t;
        }
    }
    let t = (low + high) / 2.;
    (3. + 9. * (3. * (1. - t) * t * t + t * t * t)) as f32
}

#[cfg(test)]
mod tests {
    #[test]
    fn original_chrome_equalizer_keyframes() {
        let samples: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/equalizer-samples.json")).unwrap();
        for sample in samples.as_array().unwrap() {
            let phase = sample["ms"].as_f64().unwrap() as f32 / 1000.;
            for bar in 0..3 {
                let expected = sample["heights"][bar].as_f64().unwrap() as f32;
                let actual = super::height(phase, bar as i32);
                assert!(
                    (actual - expected).abs() <= 1. / 64. + 0.001,
                    "phase={phase}, bar={bar}: {actual} != {expected}"
                );
            }
        }
        let frames: Vec<_> = (0..1000)
            .map(|ms| {
                (0..3)
                    .map(|bar| super::height(ms as f32 / 1000., bar))
                    .collect::<Vec<_>>()
            })
            .collect();
        let output = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/native/stage3/equalizer-frames.json");
        std::fs::create_dir_all(output.parent().unwrap()).unwrap();
        std::fs::write(output, serde_json::to_vec(&frames).unwrap()).unwrap();
    }
}
