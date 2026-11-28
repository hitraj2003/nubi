//! Cache the software-rendered content once, preserving glyph rasterization while scaling.
use crate::{NativeAlert, Theme};
use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer};

#[derive(Default)]
pub struct ContentImages {
    cache: Option<Cache>,
}
struct Cache {
    key: String,
    frames: Vec<Image>,
    icons: Vec<Image>,
    width: f32,
    height: f32,
    offset_x: f32,
    offset_y: f32,
}
impl ContentImages {
    pub fn prepare(&mut self, ui: &NativeAlert) -> Result<(), String> {
        let scale = ui.global::<Theme>().get_scale();
        let key = serde_json::to_string(&(
            ui.get_heading().as_str(),
            ui.get_message().as_str(),
            ui.get_kind().as_str(),
            ui.global::<Theme>().get_dark(),
            scale,
            ui.window().size().width,
            ui.get_natural_width(),
            ui.get_natural_height(),
        ))
        .map_err(|error| error.to_string())?;
        if self.cache.as_ref().is_none_or(|cache| cache.key != key) {
            let prepared_at = std::time::Instant::now();
            ui.set_capture_content(true);
            let position = (ui.get_hit_x() * scale, ui.get_hit_y() * scale);
            let dimensions = (ui.get_hit_width() * scale, ui.get_hit_height() * scale);
            let result = ui.window().take_snapshot();
            ui.set_capture_content(false);
            let snapshot = result.map_err(|error| error.to_string())?;
            let x = position.0.floor().max(0.) as u32;
            let y = position.1.floor().max(0.) as u32;
            let right = (position.0 + dimensions.0)
                .ceil()
                .min(snapshot.width() as f32) as u32;
            let bottom = (position.1 + dimensions.1)
                .ceil()
                .min(snapshot.height() as f32) as u32;
            if right <= x || bottom <= y {
                return Err("invalid alert content capture bounds".into());
            }
            let full = image::RgbaImage::from_raw(
                snapshot.width(),
                snapshot.height(),
                snapshot.as_bytes().to_vec(),
            )
            .ok_or("invalid alert content pixels")?;
            let original = image::imageops::crop_imm(&full, x, y, right - x, bottom - y).to_image();
            let mut text = original.clone();
            let mut icon = original.clone();
            for (x, _, pixel) in text.enumerate_pixels_mut() {
                if (x as f32) < 40. * scale {
                    *pixel = image::Rgba([0; 4]);
                }
            }
            for (x, _, pixel) in icon.enumerate_pixels_mut() {
                if x as f32 >= 40. * scale {
                    *pixel = image::Rgba([0; 4]);
                }
            }
            let mut layers = Vec::new();
            for original in [text, icon] {
                // Blur premultiplied f32 pixels. Rounding colour and alpha to u8 separately
                // before un-premultiplying darkens faint edge pixels into visible halos.
                let mut premul = image::DynamicImage::ImageRgba8(original.clone()).into_rgba32f();
                for pixel in premul.pixels_mut() {
                    let alpha = pixel[3];
                    for channel in 0..3 {
                        pixel[channel] *= alpha;
                    }
                }
                let mut frames = Vec::with_capacity(13);
                for index in 0..13 {
                    let pixels = if index == 0 {
                        original.clone()
                    } else {
                        let blurred = image::imageops::blur(&premul, index as f32 * 0.5 * scale);
                        image::RgbaImage::from_fn(blurred.width(), blurred.height(), |x, y| {
                            let pixel = blurred.get_pixel(x, y);
                            let alpha = pixel[3].clamp(0., 1.);
                            let byte = |value: f32| (value.clamp(0., 1.) * 255.).round() as u8;
                            let alpha_byte = byte(alpha);
                            if alpha_byte == 0 {
                                return image::Rgba([0; 4]);
                            }
                            let colour = |c: f32| byte(c / alpha);
                            image::Rgba([
                                colour(pixel[0]),
                                colour(pixel[1]),
                                colour(pixel[2]),
                                alpha_byte,
                            ])
                        })
                    };
                    frames.push(Image::from_rgba8(
                        SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                            pixels.as_raw(),
                            pixels.width(),
                            pixels.height(),
                        ),
                    ));
                }
                layers.push(frames);
            }
            let icons = layers.pop().unwrap();
            let frames = layers.pop().unwrap();
            if nubi_core::isolation::development() {
                eprintln!(
                    "[nubi dev] alert content cache: {:?}",
                    prepared_at.elapsed()
                );
            }
            self.cache = Some(Cache {
                key,
                frames,
                icons,
                width: (right - x) as f32 / scale,
                height: (bottom - y) as f32 / scale,
                offset_x: (position.0 - x as f32) / scale,
                offset_y: (position.1 - y as f32) / scale,
            });
        }
        if let Some(cache) = &self.cache {
            ui.set_raster_width(cache.width);
            ui.set_raster_height(cache.height);
            ui.set_raster_offset_x(cache.offset_x);
            ui.set_raster_offset_y(cache.offset_y);
            ui.set_has_raster(true);
        }
        self.draw(ui, 6.);
        Ok(())
    }
    pub fn draw(&self, ui: &NativeAlert, blur: f64) {
        if let Some(cache) = &self.cache {
            let index = (blur.clamp(0., 6.) * 2.).round() as usize;
            ui.set_content_image(cache.frames[index].clone());
            ui.set_icon_image(cache.icons[index].clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::platform::{
        software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
        Platform, PlatformError, WindowAdapter,
    };
    use std::rc::Rc;
    struct Offscreen(Rc<MinimalSoftwareWindow>);
    impl Platform for Offscreen {
        fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
            Ok(self.0.clone())
        }
    }
    #[test]
    fn alert_content_cache_preserves_transparency_and_reuses_raster() {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        slint::platform::set_platform(Box::new(Offscreen(window.clone()))).unwrap();
        let ui = NativeAlert::new().unwrap();
        ui.set_heading("Claude · Task complete".into());
        let title_only_height = ui.get_natural_height();
        ui.set_message("Typography comparison".into());
        ui.set_kind("done".into());
        assert!(ui.get_natural_width() < 320.);
        assert!(ui.get_natural_height() > title_only_height);
        ui.set_pill_width(ui.get_natural_width());
        ui.set_pill_height(ui.get_natural_height());
        ui.set_pill_scale(1.);
        ui.set_pill_opacity(1.);
        ui.set_content_opacity(1.);
        ui.show().unwrap();
        window.set_size(slint::PhysicalSize::new(720, 160));
        let mut images = ContentImages::default();
        let started = std::time::Instant::now();
        images.prepare(&ui).unwrap();
        eprintln!("Initial content cache: {:?}", started.elapsed());
        assert!(!ui.get_capture_content());
        assert!(ui.get_has_raster());
        let cache = images.cache.as_ref().unwrap();
        assert_eq!(cache.frames.len(), 13);
        let sharp = cache.frames[0].to_rgba8().unwrap();
        let blurred = cache.frames[12].to_rgba8().unwrap();
        let sharp_count = sharp.as_slice().iter().filter(|pixel| pixel.a > 0).count();
        let blurred_count = blurred
            .as_slice()
            .iter()
            .filter(|pixel| pixel.a > 0)
            .count();
        assert!(sharp_count > 100);
        assert!(blurred_count > sharp_count);
        assert_eq!(sharp.as_slice()[0].a, 0);
        assert!(
            blurred.as_slice().iter().map(|pixel| pixel.a).max()
                < sharp.as_slice().iter().map(|pixel| pixel.a).max()
        );
        for (index, pixel) in blurred.as_slice().iter().enumerate() {
            if index as u32 % blurred.width() > 80 && pixel.a > 0 {
                assert_eq!(
                    (pixel.r, pixel.g, pixel.b),
                    (255, 255, 255),
                    "blur must not add dark halos to white text"
                );
            }
        }
        images.prepare(&ui).unwrap();
        let again = images.cache.as_ref().unwrap().frames[0].to_rgba8().unwrap();
        assert_eq!(sharp.as_slice().as_ptr(), again.as_slice().as_ptr());
        images.draw(&ui, 0.);
        ui.set_content_scale(1.);
        ui.set_content_y(0.);
        let rendered = ui.window().take_snapshot().unwrap();
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/native/alerts/renders");
        std::fs::create_dir_all(&folder).unwrap();
        use image::ImageEncoder;
        let file = std::fs::File::create(folder.join("cached-content-100.png")).unwrap();
        image::codecs::png::PngEncoder::new(file)
            .write_image(
                rendered.as_bytes(),
                rendered.width(),
                rendered.height(),
                image::ExtendedColorType::Rgba8,
            )
            .unwrap();
        ui.hide().unwrap();
    }
}
