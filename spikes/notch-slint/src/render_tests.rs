//! Render the real compiled UI without a desktop or native window.
use super::*;
use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType,
};
use slint::platform::{Platform, PlatformError, WindowAdapter, WindowEvent};
use slint::{ModelRc, VecModel};

struct Offscreen(Rc<MinimalSoftwareWindow>);

impl Platform for Offscreen {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.0.clone())
    }
}

// Composite transparency over gray so the backing window is visible in review images.
fn save_bmp(path: &str, pixels: &[PremultipliedRgbaColor], width: usize, height: usize) {
    let stride = (width * 3 + 3) & !3;
    let mut bytes = vec![0_u8; 54 + stride * height];
    bytes[..2].copy_from_slice(b"BM");
    let length = bytes.len() as u32;
    bytes[2..6].copy_from_slice(&length.to_le_bytes());
    bytes[10..14].copy_from_slice(&54_u32.to_le_bytes());
    bytes[14..18].copy_from_slice(&40_u32.to_le_bytes());
    bytes[18..22].copy_from_slice(&(width as i32).to_le_bytes());
    bytes[22..26].copy_from_slice(&(height as i32).to_le_bytes());
    bytes[26..28].copy_from_slice(&1_u16.to_le_bytes());
    bytes[28..30].copy_from_slice(&24_u16.to_le_bytes());
    for y in 0..height {
        for x in 0..width {
            let pixel = pixels[y * width + x];
            let background = (255 - u16::from(pixel.alpha)) * 96 / 255;
            let offset = 54 + (height - 1 - y) * stride + x * 3;
            bytes[offset..offset + 3].copy_from_slice(&[
                (u16::from(pixel.blue) + background) as u8,
                (u16::from(pixel.green) + background) as u8,
                (u16::from(pixel.red) + background) as u8,
            ]);
        }
    }
    std::fs::create_dir_all("target/layout").unwrap();
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn content_and_clip_stay_at_top_at_100_125_150_percent() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    slint::platform::set_platform(Box::new(Offscreen(window.clone()))).unwrap();
    let ui = NotchWindow::new().unwrap();
    ui.set_ready(true);
    ui.set_sessions(ModelRc::from(Rc::new(VecModel::from(vec![
        Session {
            name: "G · native notch".into(),
            provider: "claude".into(),
            state: "Done".into(),
            context: "48k".into(),
            ago: "2m".into(),
        };
        3
    ]))));
    ui.show().unwrap();
    for percent in [100, 125, 150] {
        let scale = percent as f32 / 100.0;
        window.dispatch_event(WindowEvent::ScaleFactorChanged {
            scale_factor: scale,
        });
        let width = (shape::WINDOW_WIDTH as f32 * scale).round() as usize;
        let height = (shape::WINDOW_HEIGHT as f32 * scale).round() as usize;
        window.set_size(slint::PhysicalSize::new(width as u32, height as u32));
        let mut pixels = vec![PremultipliedRgbaColor::default(); width * height];
        // Reuse the buffer through full and intermediate frames in both directions.
        let halfway = [400.0, 100.0, 24.0, 14.0];
        for (frame, (opened, values)) in [
            (true, shape::OPENED),
            (false, shape::CLOSED),
            (true, halfway),
            (true, shape::OPENED),
            (false, halfway),
            (false, shape::CLOSED),
        ]
        .into_iter()
        .enumerate()
        {
            let mut motion = Motion::new(opened);
            motion.springs = values.map(spring::Spring::new);
            motion.draw(&ui);
            assert!(window.draw_if_needed(|renderer| {
                renderer.set_repaint_buffer_type(if frame == 0 {
                    RepaintBufferType::NewBuffer
                } else {
                    RepaintBufferType::ReusedBuffer
                });
                renderer.render(&mut pixels, width);
            }));
            save_bmp(
                &format!(
                    "target/layout/{percent}-{}-{}.bmp",
                    if opened { "open" } else { "closed" },
                    values[1]
                ),
                &pixels,
                width,
                height,
            );
            let depth = values[1];
            let bottom = (depth as f32 * scale).ceil() as usize;
            assert!(
                pixels[bottom * width..]
                    .iter()
                    .all(|pixel| pixel.alpha == 0),
                "content or stale pixels below outline at {percent}%, opened={opened}"
            );
            let at = |x: f32, y: f32| pixels[(y * scale) as usize * width + (x * scale) as usize];
            assert_eq!(at(360.0, 2.0).alpha, 255);
            if opened {
                // Interior of the Sessions tab must be on the surface near the top.
                if values == shape::OPENED {
                    assert!(at(150.0, 28.0).red > 20, "missing header at {percent}%");
                }
                for y in [72.0, 112.0, 152.0] {
                    if f64::from(y) >= depth {
                        continue;
                    }
                    let dot = at(468.0, y);
                    assert!(
                        dot.green > 150 && dot.alpha == 255,
                        "missing row at {percent}%, y={y}"
                    );
                }
            } else {
                let ring = at(312.5, 11.0);
                assert!(ring.green > 100, "missing session arc at {percent}%");
                let gap = at(312.5, 33.0);
                assert!(
                    gap.green < 80,
                    "session arc must leave its unused remainder visible"
                );
            }
        }
    }
}
