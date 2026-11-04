//! Floating ring window, after yonathanamir/claude-usage-meter (MIT, commit 260e4f2):
//! an 84 px square per provider (radius 30 + 12 margin), circles 50 px apart, default spot at the
//! right edge vertically centred, drag anywhere and on release snap to the nearer side edge with a
//! 250 ms OutCubic slide, position saved. Tooltip window to the right (flips left), 8 px gap,
//! y = ring y - 20.
use crate::app::{App, Shared};
use std::time::Duration;
use tauri::{Emitter, Manager, PhysicalPosition, PhysicalSize};

pub const SIDE: f64 = 84.0;
pub const STEP: f64 = 50.0;
pub const EDGE_MARGIN: f64 = 12.0;
pub const TIP_W: f64 = 260.0;

fn settings(app: &App) -> crate::settings::Settings {
    let st = app.state::<Shared>();
    let g = st.0.lock().unwrap_or_else(|e| e.into_inner());
    g.settings.clone()
}

fn work_area(app: &App, idx: usize) -> Option<(i32, i32, i32, i32, f64)> {
    let mons = app.available_monitors().ok()?;
    let m = mons.get(idx).or(mons.first())?;
    let wa = m.work_area();
    Some((wa.position.x, wa.position.y, wa.size.width as i32, wa.size.height as i32, m.scale_factor()))
}

fn size(_app: &App, scale: f64) -> (i32, i32) {
    // One ring in every case: with both providers it is split, Claude left and Codex right.
    let _ = STEP;
    ((SIDE * scale).round() as i32, (SIDE * scale).round() as i32)
}

pub fn place(app: &App) {
    let Some(w) = app.get_webview_window("ring") else { return };
    let s = settings(app);
    if s.display_mode != "ring" {
        let _ = w.hide();
        if let Some(t) = app.get_webview_window("ringtip") {
            let _ = t.emit_to(t.label(), "ringtip_hide", ());
            let _ = t.hide();
        }
        return;
    }
    let Some((ax, ay, aw, ah, scale)) = work_area(app, s.monitor) else { return };
    let (ww, wh) = size(app, scale);
    let m = (EDGE_MARGIN * scale).round() as i32;
    // A saved spot is reused only while it is still on this work area.
    let (x, y) = match s.ring_pos {
        Some((x, y)) if x + ww > ax && x < ax + aw && y + wh > ay && y < ay + ah => (x, y),
        _ => (ax + aw - ww - m, ay + (ah - wh) / 2),
    };
    let _ = w.set_size(PhysicalSize::new(ww as u32, wh as u32));
    let _ = w.set_position(PhysicalPosition::new(x, y));
    let _ = w.show();
}

/// Drag: put the window's top-left at (x, y), physical screen px.
#[tauri::command]
pub fn ring_move(app: App, x: i32, y: i32) {
    if let Some(w) = app.get_webview_window("ring") {
        let _ = w.set_position(PhysicalPosition::new(x, y));
    }
}

/// Release after a drag: snap to the nearer side edge, clamp y, slide 250 ms OutCubic, save.
#[tauri::command]
pub fn ring_drop(app: App) {
    let Some(w) = app.get_webview_window("ring") else { return };
    let Some((ax, ay, aw, ah, scale)) = work_area(&app, settings(&app).monitor) else { return };
    let Ok(p) = w.outer_position() else { return };
    let (ww, wh) = size(&app, scale);
    let m = (EDGE_MARGIN * scale).round() as i32;
    let tx = if p.x + ww / 2 < ax + aw / 2 { ax + m } else { ax + aw - ww - m };
    let ty = p.y.clamp(ay + m, (ay + ah - wh - m).max(ay + m));
    let (sx, sy) = (p.x as f64, p.y as f64);
    std::thread::spawn(move || {
        let steps = 16;
        for i in 1..=steps {
            let t = i as f64 / steps as f64;
            let e = 1.0 - (1.0 - t).powi(3); // OutCubic
            let nx = (sx + (tx as f64 - sx) * e).round() as i32;
            let ny = (sy + (ty as f64 - sy) * e).round() as i32;
            let _ = w.set_position(PhysicalPosition::new(nx, ny));
            std::thread::sleep(Duration::from_millis(250 / steps as u64));
        }
        let st = app.state::<Shared>();
        let mut g = st.0.lock().unwrap_or_else(|e| e.into_inner());
        g.settings.ring_pos = Some((tx, ty));
        let _ = crate::settings::save(&g.settings);
    });
}

/// Show or hide the tooltip beside the ring for one provider and mode.
#[tauri::command]
pub fn ring_tip(app: App, show: bool, provider: String, mode: String, height: f64) {
    let Some(t) = app.get_webview_window("ringtip") else { return };
    if !show {
        let _ = t.emit_to(t.label(), "ringtip_hide", ());
            let _ = t.hide();
        return;
    }
    let Some(w) = app.get_webview_window("ring") else { return };
    let Some((ax, ay, aw, ah, scale)) = work_area(&app, settings(&app).monitor) else { return };
    let (Ok(p), Ok(sz)) = (w.outer_position(), w.outer_size()) else { return };
    let (tw, th) = ((TIP_W * scale).round() as i32, (height.max(80.0) * scale).round() as i32);
    let gap = (8.0 * scale).round() as i32;
    let mut x = p.x + sz.width as i32 + gap;
    if x + tw > ax + aw {
        x = p.x - tw - gap;
    }
    let y = (p.y - (20.0 * scale) as i32).clamp(ay, (ay + ah - th).max(ay));
    let _ = t.set_size(PhysicalSize::new(tw as u32, th as u32));
    let _ = t.set_position(PhysicalPosition::new(x, y));
    let _ = t.emit_to(t.label(), "ringtip", serde_json::json!({ "provider": provider, "mode": mode }));
    let _ = t.show();
}
