//! Shared edge placement, dragging and click-through for the island and notification windows.
//! Edge placement follows vinzdg/codenotch (MIT, commit 6522e9f).
use crate::app::{App, Shared};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{Emitter, Manager};
use windows_sys::Win32::Foundation::POINT;
use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

/// Interactive area reported by the notification page, in physical px: x, y, w, h.
static OVL_HOT: Mutex<Option<[f64; 4]>> = Mutex::new(None);
/// Same for the top island (display mode "island").
static ISL_HOT: Mutex<Option<[f64; 4]>> = Mutex::new(None);

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
    crate::island::place(&app, &s);
}

/// End of a drag: save where the notch was left.
#[tauri::command]
pub fn notch_drag_end(app: App) {
    let st = app.state::<Shared>();
    let g = st.0.lock().unwrap_or_else(|e| e.into_inner());
    let _ = crate::settings::save(&g.settings);
}

/// Island page reports its interactive rect (pill and request cards), or None when empty.
#[tauri::command]
pub fn report_overlay_hot(rect: Option<[f64; 4]>) {
    *OVL_HOT.lock().unwrap() = rect;
}

/// Top island page reports its interactive rect (the shape at its current size).
#[tauri::command]
pub fn report_island_hot(rect: Option<[f64; 4]>) {
    *ISL_HOT.lock().unwrap() = rect;
}

/// Watch the system cursor: over the hot rect the window takes clicks, elsewhere it lets them
/// through. Tells the page when the pointer arrives or leaves (fold on hover).
pub fn start_watch(app: App) {
    watch(app.clone(), "island", &ISL_HOT, "island_pointer", Some("island"));
    watch(app, "overlay", &OVL_HOT, "overlay_pointer", None);
}

/// Watch the system cursor for one window: over its hot rect the window takes clicks, elsewhere
/// they pass through. Tells the page when the pointer arrives or leaves.
fn watch(app: App, label: &'static str, hot: &'static Mutex<Option<[f64; 4]>>, event: &'static str, only_mode: Option<&'static str>) {
    std::thread::spawn(move || {
        let mut inside = false;
        loop {
            std::thread::sleep(Duration::from_millis(if only_mode == Some("island") { 16 } else { 50 }));
            let Some(w) = app.get_webview_window(label) else { continue };
            if !w.is_visible().unwrap_or(false) {
                inside = false;
                continue;
            }
            if let Some(m) = only_mode {
                let st = app.state::<Shared>();
                let g = st.0.lock().unwrap_or_else(|e| e.into_inner());
                if g.settings.display_mode != m {
                    continue;
                }
            }
            let mut p = POINT { x: 0, y: 0 };
            if unsafe { GetCursorPos(&mut p) } == 0 {
                continue;
            }
            let Ok(pos) = w.outer_position() else { continue };
            let now = hot.lock().unwrap().map_or(false, |[x, y, ww, hh]| {
                let (cx, cy) = ((p.x - pos.x) as f64, (p.y - pos.y) as f64);
                cx >= x - 4.0 && cy >= y - 4.0 && cx < x + ww + 4.0 && cy < y + hh + 4.0
            });
            if now != inside {
                inside = now;
                let _ = w.set_ignore_cursor_events(!now);
                let _ = w.emit_to(w.label(), event, now);
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
