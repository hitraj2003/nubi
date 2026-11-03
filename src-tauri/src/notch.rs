//! Edge notch window: placement, sizes and click-through, ported from vinzdg/codenotch
//! (windows/codenotch/src/main.rs, MIT, commit 6522e9f). Sizes and the work-area placement are
//! codenotch's; the drag, carry and orb features are not ported.
use crate::app::{App, Shared};
use crate::settings::Settings;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{Emitter, Manager, PhysicalPosition, PhysicalSize};
use windows_sys::Win32::Foundation::POINT;
use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

/// Logical width of the upright notch window (codenotch `NOTCH_W`).
pub const NOTCH_W: f64 = 360.0;
/// Logical long side (codenotch `NOTCH_LONG`).
pub const NOTCH_LONG: f64 = 650.0;

/// Interactive area reported by the page, in physical px relative to the window: x, y, w, h.
static HOT: Mutex<Option<[f64; 4]>> = Mutex::new(None);

pub fn edge_is_vertical(edge: &str) -> bool {
    matches!(edge, "left" | "right")
}

pub fn edge_or_right(v: &str) -> String {
    if ["left", "right", "top", "bottom"].contains(&v) {
        v.to_string()
    } else {
        "right".into()
    }
}

/// codenotch `notch_window_size`: upright = 360 x 650, flat = 650 x 650.
pub fn window_size(edge: &str) -> (f64, f64) {
    if edge_is_vertical(edge) {
        (NOTCH_W, NOTCH_LONG)
    } else {
        (NOTCH_LONG, NOTCH_LONG)
    }
}

/// codenotch `edge_origin`: top-left corner for an edge, centred at `ratio` along it, inside the
/// work area so the taskbar never covers the notch.
pub fn edge_origin(area: (i32, i32, i32, i32), edge: &str, ww: i32, wh: i32, ratio: f64) -> (i32, i32) {
    let (ax, ay, aw, ah) = area;
    let along = |span: i32, len: i32| -> i32 {
        let v = (span as f64 * ratio - len as f64 / 2.0).round() as i32;
        v.clamp(0, (span - len).max(0))
    };
    match edge {
        "left" => (ax, ay + along(ah, wh)),
        "top" => (ax + along(aw, ww), ay),
        "bottom" => (ax + along(aw, ww), ay + ah - wh),
        _ => (ax + aw - ww, ay + along(ah, wh)),
    }
}

pub fn place(app: &App, s: &Settings) {
    let Some(w) = app.get_webview_window("notch") else { return };
    if s.display_mode != "notch" {
        let _ = w.hide();
        return;
    }
    let mons = app.available_monitors().unwrap_or_default();
    let Some(m) = mons.get(s.monitor).or(mons.first()) else { return };
    let edge = edge_or_right(&s.edge);
    let scale = m.scale_factor();
    let (mut lw, mut lh) = window_size(&edge);
    // Side-by-side request cards need room: 256 per card beyond the first (max 3 across).
    let waiting = {
        let st = app.state::<Shared>();
        let g = st.0.lock().unwrap_or_else(|e| e.into_inner());
        g.pending.values().filter(|p| g.settings.wants(&p.provider)).count()
    };
    if s.notch_requests_layout == "side" && waiting > 1 {
        let extra = 256.0 * (waiting.min(3) - 1) as f64;
        if edge_is_vertical(&edge) { lw += extra } else { lw = lw.max(256.0 * waiting.min(3) as f64 + 40.0); lh += 0.0 }
    }
    let (ww, wh) = ((lw * scale).round() as i32, (lh * scale).round() as i32);
    let wa = m.work_area();
    let mut area = (wa.position.x, wa.position.y, wa.size.width as i32, wa.size.height as i32);
    if area.2 <= 0 || area.3 <= 0 {
        area = (m.position().x, m.position().y, m.size().width as i32, m.size().height as i32);
    }
    let (x, y) = edge_origin(area, &edge, ww, wh, s.notch_along.clamp(0.0, 1.0));
    let _ = w.set_size(PhysicalSize::new(ww as u32, wh as u32));
    let _ = w.set_position(PhysicalPosition::new(x, y));
    let _ = w.set_ignore_cursor_events(true);
    let _ = w.show();
    let _ = w.emit("notch_edge", &edge);
}

/// Drag along the edge: the pointer's screen position (physical px) becomes the notch's centre.
#[tauri::command]
pub fn notch_drag(app: App, x: f64, y: f64) {
    let s = {
        let st = app.state::<Shared>();
        let mut g = st.0.lock().unwrap_or_else(|e| e.into_inner());
        let mons = app.available_monitors().unwrap_or_default();
        let Some(m) = mons.get(g.settings.monitor).or(mons.first()) else { return };
        let wa = m.work_area();
        let (ax, ay, aw, ah) = (wa.position.x as f64, wa.position.y as f64, wa.size.width as f64, wa.size.height as f64);
        let edge = edge_or_right(&g.settings.edge);
        let r = if edge_is_vertical(&edge) { (y - ay) / ah.max(1.0) } else { (x - ax) / aw.max(1.0) };
        g.settings.notch_along = r.clamp(0.0, 1.0);
        g.settings.clone()
    };
    place(&app, &s);
}

/// End of a drag: save where the notch was left.
#[tauri::command]
pub fn notch_drag_end(app: App) {
    let st = app.state::<Shared>();
    let g = st.0.lock().unwrap_or_else(|e| e.into_inner());
    let _ = crate::settings::save(&g.settings);
}

/// Page reports its interactive rect (pill, plus card when open).
#[tauri::command]
pub fn report_hot(rect: Option<[f64; 4]>) {
    *HOT.lock().unwrap() = rect;
}

/// Watch the system cursor: over the hot rect the window takes clicks, elsewhere it lets them
/// through. Tells the page when the pointer arrives or leaves (fold on hover).
pub fn start_watch(app: App) {
    std::thread::spawn(move || {
        let mut inside = false;
        loop {
            std::thread::sleep(Duration::from_millis(50));
            let Some(w) = app.get_webview_window("notch") else { continue };
            if !w.is_visible().unwrap_or(false) {
                continue;
            }
            let mode_ok = {
                let st = app.state::<Shared>();
                let g = st.0.lock().unwrap_or_else(|e| e.into_inner());
                g.settings.display_mode == "notch"
            };
            if !mode_ok {
                continue;
            }
            let mut p = POINT { x: 0, y: 0 };
            if unsafe { GetCursorPos(&mut p) } == 0 {
                continue;
            }
            let Ok(pos) = w.outer_position() else { continue };
            let now = HOT.lock().unwrap().map_or(false, |[x, y, ww, hh]| {
                let (cx, cy) = ((p.x - pos.x) as f64, (p.y - pos.y) as f64);
                cx >= x - 4.0 && cy >= y - 4.0 && cx < x + ww + 4.0 && cy < y + hh + 4.0
            });
            if now != inside {
                inside = now;
                let _ = w.set_ignore_cursor_events(!now);
                let _ = w.emit("notch_pointer", now);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn right_edge_centred_in_work_area() {
        // 1920x1040 work area (taskbar 40 px), window 360x650
        assert_eq!(edge_origin((0, 0, 1920, 1040), "right", 360, 650, 0.5), (1560, 195));
    }

    #[test]
    fn bottom_edge_sits_above_taskbar() {
        assert_eq!(edge_origin((0, 0, 1920, 1040), "bottom", 650, 650, 0.5), (635, 390));
    }

    #[test]
    fn ratio_is_clamped_inside() {
        assert_eq!(edge_origin((0, 0, 1920, 1040), "left", 360, 650, 1.0), (0, 390));
    }
}
