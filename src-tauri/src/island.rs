//! Notch (display mode "island"): one black shape on a screen edge (top, bottom, left or right),
//! centred at `notch_along` and draggable along the edge. It sits as a small pill (or a thin
//! tab, when it folds until hovered) and springs open into a panel. On the top edge it also shows
//! alerts and requests; on other edges those go to the top toast. The window is fixed at its
//! largest size and transparent; only the shape takes clicks (see notch::report_island_hot).
use crate::app::App;
use crate::notch::{edge_or_right, edge_origin, edge_is_vertical};
use crate::settings::Settings;
use tauri::{Emitter, Manager, PhysicalPosition, PhysicalSize};

/// Logical window size along and across the edge: the open shape's largest size plus room for
/// its ears and overshoot.
pub const ALONG: f64 = 720.0;
pub const DEPTH: f64 = 700.0;
/// On a side edge the open panel stands up (360 wide, tall), so the window is narrow and tall.
pub const SIDE_DEPTH: f64 = 420.0;
pub const SIDE_ALONG: f64 = 860.0;

pub fn window_size(edge: &str) -> (f64, f64) {
    if edge_is_vertical(edge) {
        (SIDE_DEPTH, SIDE_ALONG)
    } else {
        (ALONG, DEPTH)
    }
}

/// Last shift sent to the page, for a page that loads after it was sent.
static OFFSET: std::sync::Mutex<f64> = std::sync::Mutex::new(0.0);

#[tauri::command]
pub fn island_offset() -> f64 {
    *OFFSET.lock().unwrap()
}

pub fn place(app: &App, s: &Settings) {
    let Some(w) = app.get_webview_window("island") else { return };
    if s.display_mode != "island" {
        let _ = w.hide();
        return;
    }
    let mons = app.available_monitors().unwrap_or_default();
    let Some(m) = mons.get(s.monitor).or(mons.first()) else { return };
    let edge = edge_or_right(&s.edge);
    let scale = m.scale_factor();
    let (lw, lh) = window_size(&edge);
    let (ww, wh) = ((lw * scale).round() as i32, (lh * scale).round() as i32);
    // Top edge: the whole monitor (the shape hangs from the screen edge). Others: the work area,
    // so the taskbar never covers it.
    let area = if edge == "top" {
        (m.position().x, m.position().y, m.size().width as i32, m.size().height as i32)
    } else {
        let wa = m.work_area();
        if wa.size.width > 0 && wa.size.height > 0 {
            (wa.position.x, wa.position.y, wa.size.width as i32, wa.size.height as i32)
        } else {
            (m.position().x, m.position().y, m.size().width as i32, m.size().height as i32)
        }
    };
    let ratio = s.notch_along.clamp(0.0, 1.0);
    let (x, y) = edge_origin(area, &edge, ww, wh, ratio);
    // The window stays on screen, so near the ends it cannot centre on the wanted spot. The page
    // shifts the shape inside the window by this much (logical px along the edge) instead.
    let vertical = edge_is_vertical(&edge);
    let (start, span, win, at) = if vertical { (area.1, area.3, wh, y) } else { (area.0, area.2, ww, x) };
    let want = start as f64 + span as f64 * ratio;
    let offset = (want - (at as f64 + win as f64 / 2.0)) / scale;
    let _ = w.set_size(PhysicalSize::new(ww as u32, wh as u32));
    let _ = w.set_position(PhysicalPosition::new(x, y));
    let _ = w.set_shadow(false);
    if !w.is_visible().unwrap_or(false) {
        // Click-through until the pointer reaches the shape. Not reset while visible, so a drag
        // (which calls this on every move) keeps the window taking the pointer.
        let _ = w.set_ignore_cursor_events(true);
    }
    let _ = w.show();
    let _ = w.emit_to(w.label(), "island_edge", &edge);
    *OFFSET.lock().unwrap() = offset;
    let _ = w.emit_to(w.label(), "island_offset", offset);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn side_window_is_deeper_than_tall_panel() {
        let (d, a) = window_size("right");
        assert!(d >= 400.0 && a > d);
    }

    #[test]
    fn top_centre_on_second_monitor() {
        assert_eq!(edge_origin((1920, 0, 2560, 1440), "top", 720, 540, 0.5), (1920 + 920, 0));
    }
}
