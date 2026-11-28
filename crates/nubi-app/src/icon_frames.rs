//! Bake rotation into the artwork because the software renderer ignores item transforms.
use slint::{Image, SharedString};
use std::{cell::RefCell, collections::HashMap};

type Frames = HashMap<(SharedString, i32), Image>;
thread_local! {
    static FRAMES: RefCell<Frames> = RefCell::default();
    static BELL_HOVER: Vec<Image> = bell_frames(160.);
    static BELL_CLICK: Vec<Image> = bell_frames(600.);
}

// Bell Button v1.0.0 by Kokonut UI (MIT). Samples the original 240 Hz
// pendulum simulation for playback through the software renderer.
pub fn bell(ring: bool, frame: i32) -> Image {
    let frames = if ring { &BELL_CLICK } else { &BELL_HOVER };
    frames.with(|frames| frames.get(frame as usize).cloned().unwrap_or_default())
}

fn bell_frames(kick: f64) -> Vec<Image> {
    const STEP: f64 = 1. / 240.;
    let (mut bell, mut bell_speed, mut clapper, mut clapper_speed) = (0., kick, 0., 0.);
    let mut waves = [(0_i32, 0_f64); 2];
    let mut frames = vec![Image::default()];
    for frame in 1..=240 {
        for _ in 0..4 {
            bell_speed += (-bell * 380. - bell_speed * 5.) * STEP;
            bell += bell_speed * STEP;
            clapper_speed += (-clapper * 150. - clapper_speed * 4.) * STEP;
            clapper += clapper_speed * STEP;
            let relative: f64 = clapper - bell;
            if relative.abs() <= 13. {
                continue;
            }
            let side = relative.signum();
            let impact = (clapper_speed - bell_speed) * side;
            clapper = bell + side * 13.;
            if impact <= 0. {
                continue;
            }
            clapper_speed -= side * impact * 1.45;
            bell_speed += side * impact * 0.18;
            if impact > 260. {
                waves[usize::from(side > 0.)] = (frame, (0.45 + impact / 1600. * 0.55).min(1.));
            }
        }
        let energy =
            bell.abs() + bell_speed.abs() * 0.02 + clapper.abs() + clapper_speed.abs() * 0.02;
        if energy < 0.05 {
            break;
        }
        let mut arcs = String::new();
        for ((start, peak), (side, path)) in waves.into_iter().zip([
            (-1., "M2.5 4.5C1.3 6.1.7 7.9.7 10"),
            (1., "M21.5 4.5c1.2 1.6 1.8 3.4 1.8 5.5"),
        ]) {
            let progress = f64::from(frame - start) / (60. * 0.42);
            if peak > 0. && progress < 1. {
                // CSS cubic-bezier(0.23, 1, 0.32, 1), resolved at elapsed time.
                let (mut low, mut high) = (0., 1.);
                for _ in 0..12 {
                    let t = (low + high) / 2.;
                    let u = 1. - t;
                    let x = 3. * u * u * t * 0.23 + 3. * u * t * t * 0.32 + t * t * t;
                    if x < progress {
                        low = t;
                    } else {
                        high = t;
                    }
                }
                let remaining = (1_f64 - (low + high) / 2.).powi(3);
                let opacity = peak * remaining;
                let offset = side * 3. * (1. - remaining);
                arcs.push_str(&format!(r#"<path d="{path}" opacity="{opacity:.3}" transform="translate({offset:.2})"/>"#));
            }
        }
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" viewBox="-4 -4 32 32">{arcs}
            <path transform="rotate({clapper:.2} 12 2.5)" d="M10.27 21a2 2 0 0 0 3.46 0"/>
            <path transform="rotate({bell:.2} 12 2.5)" d="M3.26 15.33A1 1 0 0 0 4 17h16a1 1 0 0 0 .74-1.67C19.41 13.96 18 12.5 18 8A6 6 0 0 0 6 8c0 4.5-1.41 5.96-2.74 7.33"/>
            </svg>"#
        );
        frames.push(Image::load_from_svg_data(svg.as_bytes()).unwrap_or_default());
    }
    frames
}

pub fn rotated(artwork: SharedString, frame: i32) -> Image {
    let svg = match artwork.as_str() {
        "notch-refresh-usage" => include_str!("../assets/notch-refresh-usage.svg"),
        _ => return Image::default(),
    };
    let frame = frame.rem_euclid(180);
    FRAMES.with_borrow_mut(|frames| {
        frames
            .entry((artwork, frame))
            .or_insert_with(|| {
                let (header, body) = svg.split_once('>').expect("icon SVG header");
                let body = body
                    .trim_end()
                    .strip_suffix("</svg>")
                    .expect("icon SVG body");
                // Give rotating strokes four units of clearance around the original canvas.
                let header = header.replace("0 0 24 24", "-4 -4 32 32");
                let degrees = frame * 2;
                let svg =
                    format!("{header}><g transform=\"rotate({degrees} 12 12)\">{body}</g></svg>");
                Image::load_from_svg_data(svg.as_bytes()).unwrap_or_default()
            })
            .clone()
    })
}
