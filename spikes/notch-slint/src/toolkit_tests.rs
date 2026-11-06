use super::*;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Clipboard, Key, Platform, PlatformError, WindowAdapter, WindowEvent};

#[derive(Default)]
struct TestPlatform {
    clipboard: Rc<RefCell<String>>,
}

impl Platform for TestPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer))
    }
    fn set_clipboard_text(&self, text: &str, _: Clipboard) {
        *self.clipboard.borrow_mut() = text.into();
    }
    fn clipboard_text(&self, _: Clipboard) -> Option<String> {
        Some(self.clipboard.borrow().clone())
    }
}

fn key(window: &slint::Window, text: impl Into<slint::SharedString>) {
    let text = text.into();
    window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    window.dispatch_event(WindowEvent::KeyReleased { text });
}

fn control(window: &slint::Window, text: &str) {
    window.dispatch_event(WindowEvent::KeyPressed {
        text: Key::Control.into(),
    });
    key(window, text);
    window.dispatch_event(WindowEvent::KeyReleased {
        text: Key::Control.into(),
    });
}

#[test]
fn date_and_time_reject_invalid_and_accept_boundaries() {
    for value in [
        "2026-11-06",
        "2000-02-29",
        "2024-02-29",
        "9999-12-31",
        "0001-01-01",
    ] {
        assert!(valid_date(value), "{value}");
    }
    for value in [
        "1900-02-29",
        "2026-02-29",
        "2026-04-31",
        "2026-00-03",
        "0000-01-01",
        "2026-11-00",
        "2026-1-06",
        "２０２６-11-06",
        "💡-00-01",
        "",
        "2026-13-01",
    ] {
        assert!(!valid_date(value), "{value}");
    }
    for value in ["00:00", "23:59", "18:00"] {
        assert!(valid_time(value), "{value}");
    }
    for value in ["24:00", "23:60", "1:00", "18.00", "💡:", "", "-1:00"] {
        assert!(!valid_time(value), "{value}");
    }
}

#[test]
fn actual_ui_keyboard_copy_selection_drafts_scrolling_and_themes() {
    let clipboard = Rc::new(RefCell::new(String::new()));
    slint::platform::set_platform(Box::new(TestPlatform {
        clipboard: clipboard.clone(),
    }))
    .unwrap();
    let card = ApprovalWindow::new().unwrap();
    card.show().unwrap();
    card.window().set_size(slint::PhysicalSize::new(480, 390));
    card.window()
        .dispatch_event(WindowEvent::WindowActiveChanged(true));
    let decisions = Rc::new(RefCell::new(Vec::new()));
    let record = decisions.clone();
    card.on_decision(move |v| record.borrow_mut().push(v.to_string()));
    card.invoke_take_focus();
    assert_eq!(card.get_focus_index(), 4, "Safe initial focus is Deny");
    key(card.window(), Key::Return);
    assert_eq!(&*decisions.borrow(), &["deny"]);
    for index in [0, 1, 2, 3, 4, 0] {
        key(card.window(), Key::Tab);
        assert_eq!(card.get_focus_index(), index, "Explicit forward Tab cycle");
    }
    key(card.window(), Key::Backtab);
    assert_eq!(card.get_focus_index(), 4, "Reverse Tab wraps");
    key(card.window(), Key::Tab);
    key(card.window(), Key::Tab);
    key(card.window(), Key::Return);
    assert!(card.get_expanded());
    key(card.window(), Key::Tab);
    key(card.window(), Key::Return);
    assert_eq!(*clipboard.borrow(), card.get_command().as_str());
    key(card.window(), Key::Tab);
    key(card.window(), Key::Return);
    key(card.window(), Key::Escape);
    assert_eq!(&*decisions.borrow(), &["deny", "allow", "deny"]);
    card.invoke_focus_command();
    key(card.window(), "X");
    assert_eq!(
        card.get_command(),
        "cargo test --offline",
        "Read-only command cannot be edited"
    );

    let panel = ToolkitWindow::new().unwrap();
    panel.on_validate_date(|v| valid_date(v.as_str()));
    panel.on_validate_time(|v| valid_time(v.as_str()));
    panel.show().unwrap();
    panel.window().set_size(slint::PhysicalSize::new(940, 720));
    panel
        .window()
        .dispatch_event(WindowEvent::WindowActiveChanged(true));
    panel.invoke_take_focus();
    assert_eq!(panel.get_focused_control(), "inbox");
    for name in [
        "about", "approval", "theme", "list", "editor", "readonly", "date", "time", "stepper",
        "decrease", "increase", "help",
    ] {
        key(panel.window(), Key::Tab);
        assert_eq!(panel.get_focused_control(), name, "Panel Tab order");
    }
    // Return along the real Tab path to edit and validate date/time fields.
    for _ in 0..5 {
        key(panel.window(), Key::Backtab);
    }
    assert_eq!(panel.get_focused_control(), "date");
    control(panel.window(), "a");
    key(panel.window(), "2026-02-30");
    assert!(!panel.get_date_valid());
    control(panel.window(), "a");
    key(panel.window(), "2024-02-29");
    assert!(panel.get_date_valid());
    key(panel.window(), Key::Tab);
    control(panel.window(), "a");
    key(panel.window(), "24:00");
    assert!(!panel.get_time_valid());
    control(panel.window(), "a");
    key(panel.window(), "23:59");
    assert!(panel.get_time_valid());
    panel.invoke_focus_editor();
    control(panel.window(), "a");
    key(panel.window(), "東京 · Native");
    assert_eq!(
        panel.get_draft(),
        "東京 · Native",
        "Unicode insertion; this is not an IME composition test"
    );
    assert!(panel.get_dirty());
    panel.set_page(1);
    panel.set_page(0);
    assert!(panel.get_dirty(), "Navigation preserves draft");
    key(panel.window(), Key::Escape);
    assert!(!panel.get_dirty());
    assert_eq!(panel.get_draft(), "Nubi on this PC");
    panel.invoke_focus_editor();
    control(panel.window(), "a");
    key(panel.window(), "Workstation");
    panel.invoke_focus_help();
    key(panel.window(), Key::Tab); // Discard
    key(panel.window(), Key::Tab); // Save
    key(panel.window(), Key::Return);
    assert_eq!(panel.get_saved(), "Workstation");
    assert!(!panel.get_dirty());
    assert_eq!(
        panel.get_focused_control(),
        "editor",
        "Save restores stable focus after the bar disappears"
    );
    key(panel.window(), "X");
    assert!(panel.get_dirty());
    panel.invoke_focus_help();
    key(panel.window(), Key::Tab); // Discard
    key(panel.window(), Key::Return);
    assert_eq!(panel.get_draft(), "Workstation");
    assert!(!panel.get_dirty());
    panel.invoke_take_focus();
    key(panel.window(), Key::Tab); // About
    key(panel.window(), Key::Return);
    assert_eq!(panel.get_page(), 1);
    key(panel.window(), Key::Tab); // Approval card
    key(panel.window(), Key::Tab); // Theme
    key(panel.window(), Key::Tab); // AboutSlint wrapper
    assert_eq!(
        panel.get_focused_control(),
        "other",
        "Hidden Inbox controls leave Tab order"
    );
    key(panel.window(), Key::Tab);
    assert_eq!(panel.get_focused_control(), "inbox");
    panel.set_page(0);
    panel.invoke_focus_readonly();
    key(panel.window(), Key::Home);
    panel.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::Shift.into(),
    });
    key(panel.window(), Key::RightArrow);
    key(panel.window(), Key::RightArrow);
    panel.window().dispatch_event(WindowEvent::KeyReleased {
        text: Key::Shift.into(),
    });
    control(panel.window(), "c");
    assert_eq!(
        *clipboard.borrow(),
        "C:",
        "Read-only text supports partial keyboard selection"
    );
    control(panel.window(), "a");
    key(panel.window(), "X");
    control(panel.window(), "a");
    control(panel.window(), "c");
    assert_eq!(*clipboard.borrow(), "C:\\Fig\\projects\\G-wt-slint");
    panel.invoke_focus_stepper();
    panel.set_timeout(95);
    key(panel.window(), Key::UpArrow);
    assert_eq!(panel.get_timeout(), 95);
    panel.set_timeout(3);
    key(panel.window(), Key::DownArrow);
    assert_eq!(panel.get_timeout(), 3);
    key(panel.window(), Key::UpArrow);
    assert_eq!(panel.get_timeout(), 4);
    panel.invoke_focus_list();
    key(panel.window(), Key::End);
    assert!(panel.get_list_offset() < -900.0);
    key(panel.window(), Key::Home);
    assert_eq!(panel.get_list_offset(), 0.0);
    panel.window().dispatch_event(WindowEvent::PointerScrolled {
        position: slint::LogicalPosition::new(240.0, 120.0),
        delta_x: 0.0,
        delta_y: -96.0,
    });
    assert!(
        panel.get_list_offset() < 0.0,
        "Mouse wheel scrolls the list"
    );
    key(panel.window(), Key::Home);
    panel.invoke_focus_help();
    assert!(panel.get_tooltip_visible());

    // Render compiled surfaces in both themes for human review.
    for dark in [false, true] {
        let name = if dark { "dark" } else { "light" };
        panel.invoke_set_theme(dark);
        card.global::<Theme>().set_dark(dark);
        card.set_expanded(true);
        save_render(card.window(), &format!("approval-{name}"));
        panel.set_page(0);
        panel.set_draft("Unsaved example".into());
        save_render(panel.window(), &format!("panel-{name}"));
        panel.set_page(1);
        save_render(panel.window(), &format!("about-{name}"));
        let image = panel.window().take_snapshot().unwrap();
        let mut visible_version_pixels = 0;
        for y in 266..296 {
            for x in 480..620 {
                let pixel = image.as_slice()[y * image.width() as usize + x];
                if if dark {
                    pixel.r > 180 && pixel.g > 180 && pixel.b > 180
                } else {
                    pixel.r < 150 && pixel.g < 150 && pixel.b < 150
                } {
                    visible_version_pixels += 1;
                }
            }
        }
        assert!(
            visible_version_pixels > 30,
            "AboutSlint version and URL must remain visible in {name} mode"
        );
    }
}

fn save_render(window: &slint::Window, name: &str) {
    let image = window.take_snapshot().unwrap();
    let width = image.width() as usize;
    let height = image.height() as usize;
    let stride = (width * 3 + 3) & !3;
    let mut bytes = vec![0_u8; 54 + stride * height];
    bytes[..2].copy_from_slice(b"BM");
    let length = bytes.len() as u32;
    bytes[2..6].copy_from_slice(&length.to_le_bytes());
    bytes[10..14].copy_from_slice(&54_u32.to_le_bytes());
    bytes[14..18].copy_from_slice(&40_u32.to_le_bytes());
    bytes[18..22].copy_from_slice(&image.width().to_le_bytes());
    bytes[22..26].copy_from_slice(&image.height().to_le_bytes());
    bytes[26..28].copy_from_slice(&1_u16.to_le_bytes());
    bytes[28..30].copy_from_slice(&24_u16.to_le_bytes());
    for (i, pixel) in image.as_slice().iter().enumerate() {
        let x = i % width;
        let y = i / width;
        let offset = 54 + (height - 1 - y) * stride + x * 3;
        bytes[offset..offset + 3].copy_from_slice(&[pixel.b, pixel.g, pixel.r]);
    }
    std::fs::create_dir_all("target/h10").unwrap();
    std::fs::write(format!("target/h10/{name}.bmp"), bytes).unwrap();
}
