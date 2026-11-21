//! Original ui/usage-color.js interpolation, including its serialized precision.
const STOPS: [[f64; 4]; 4] = [
    [25., 0.74, 0.19, 148.],
    [50., 0.64, 0.19, 258.],
    [75., 0.76, 0.17, 412.],
    [100., 0.64, 0.22, 388.],
];
fn lch(percent: f64, soft: bool) -> [f64; 3] {
    let p = if percent.is_nan() {
        0.
    } else {
        percent.clamp(0., 100.)
    };
    let mut i = 0;
    while i < 3 && p > STOPS[i + 1][0] {
        i += 1;
    }
    let [p0, l0, c0, h0] = STOPS[i];
    let [p1, l1, c1, h1] = STOPS[(i + 1).min(3)];
    let t = if p <= p0 {
        0.
    } else if p1 == p0 {
        1.
    } else {
        (p - p0) / (p1 - p0)
    };
    let s = t * t * (3. - 2. * t);
    let (mut l, mut c) = (l0 + (l1 - l0) * s, c0 + (c1 - c0) * s);
    let h = (h0 + (h1 - h0) * s) % 360.;
    if soft {
        l = (l + 0.1).min(0.9);
        c *= 0.62;
    }
    // The browser receives toFixed(3), toFixed(3), toFixed(1), not the raw values.
    [js_decimal(l, 3), js_decimal(c, 3), js_decimal(h, 1)]
}
// JS toFixed rounds exact decimal ties upward. Use the exact binary significand
// rather than multiplying in f64, which could create a false decimal tie.
fn js_decimal(value: f64, digits: u32) -> f64 {
    let bits = value.to_bits();
    let significand = (bits & ((1_u64 << 52) - 1)) | (1_u64 << 52);
    let exponent = ((bits >> 52) & 0x7ff) as i32 - 1023 - 52;
    let factor = 10_u128.pow(digits);
    let scaled = u128::from(significand) * factor;
    let rounded = if exponent >= 0 {
        scaled << exponent
    } else {
        let shift = (-exponent) as u32;
        if shift >= 128 {
            0
        } else {
            (scaled + (1_u128 << (shift - 1))) >> shift
        }
    };
    rounded as f64 / factor as f64
}
pub fn color(percent: f64, soft: bool) -> slint::Color {
    let [l, c, h] = lch(percent, soft);
    let a = c * h.to_radians().cos();
    let b = c * h.to_radians().sin();
    let ll = (l + 0.3963377774 * a + 0.2158037573 * b).powi(3);
    let mm = (l - 0.1055613458 * a - 0.0638541728 * b).powi(3);
    let ss = (l - 0.0894841775 * a - 1.291485548 * b).powi(3);
    let channel = |x: f64| {
        let srgb = if x <= 0.0031308 {
            12.92 * x
        } else {
            1.055 * x.powf(1. / 2.4) - 0.055
        };
        (srgb.clamp(0., 1.) * 255.).round() as u8
    };
    slint::Color::from_rgb_u8(
        channel(4.0767416621 * ll - 3.3077115913 * mm + 0.2309699292 * ss),
        channel(-1.2684380046 * ll + 2.6097574011 * mm - 0.3413193965 * ss),
        channel(-0.0041960863 * ll - 0.7034186147 * mm + 1.707614701 * ss),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_browser_samples_match_fractional_scale() {
        let rows: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/usage-colors.json")).unwrap();
        for row in rows.as_array().unwrap() {
            let percent = row["percent"].as_f64().unwrap();
            let soft = row["soft"].as_bool().unwrap();
            let source: Vec<f64> = row["css"]
                .as_str()
                .unwrap()
                .trim_start_matches("oklch(")
                .trim_end_matches(')')
                .split_whitespace()
                .map(|v| v.parse().unwrap())
                .collect();
            assert_eq!(
                lch(percent, soft).as_slice(),
                source.as_slice(),
                "LCH at {percent} soft={soft}"
            );
            let native = color(f64::from(percent as f32), soft);
            for (index, channel) in [native.red(), native.green(), native.blue()]
                .into_iter()
                .enumerate()
            {
                let expected = row["rgba"][index].as_i64().unwrap();
                assert!(
                    (i64::from(channel) - expected).abs() <= 1,
                    "sRGB at {percent} soft={soft}, channel {index}: {channel} vs {expected}"
                );
            }
        }
        assert_ne!(color(62.1, false), color(62.4, false));
        assert_eq!(js_decimal(0.0625, 3), 0.063);
        assert_eq!(js_decimal(0.125, 2), 0.13);
        let native:Vec<_>=rows.as_array().unwrap().iter().map(|row| {
            let percent=row["percent"].as_f64().unwrap();
            let soft=row["soft"].as_bool().unwrap();
            let value=color(f64::from(percent as f32),soft);
            serde_json::json!({"percent":percent,"soft":soft,"css":row["css"],"native":format!("#{:02x}{:02x}{:02x}",value.red(),value.green(),value.blue())})
        }).collect();
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/native/colors");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join("native-colors.json"),
            serde_json::to_vec(&native).unwrap(),
        )
        .unwrap();
    }
}

#[cfg(test)]
mod wiring_tests {
    use slint::{
        platform::{
            software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
            Platform, PlatformError, WindowAdapter,
        },
        ComponentHandle,
    };
    use std::rc::Rc;
    struct Offscreen;
    impl Platform for Offscreen {
        fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
            Ok(MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer))
        }
    }
    #[test]
    fn usage_colors_are_connected_on_notch_and_settings_creation() {
        slint::platform::set_platform(Box::new(Offscreen)).unwrap();
        let ui = crate::NotchWindow::new().unwrap();
        let (host, _) = crate::host::NativeHost::from_settings(Default::default());
        let _notch = crate::notch::Notch::connect(&ui, host);
        let panel = crate::ParityPanel::new().unwrap();
        crate::panel_logic::connect(&panel);
        for soft in [true, false] {
            for percent in [25.1_f32, 49.9, 62.1, 62.4, 74.9, 99.9] {
                let expected = super::color(f64::from(percent), soft);
                assert_eq!(
                    ui.global::<crate::Fixture>()
                        .invoke_usage_color(percent, soft),
                    expected
                );
                assert_eq!(
                    panel
                        .global::<crate::Fixture>()
                        .invoke_usage_color(percent, soft),
                    expected
                );
            }
        }
    }
}
