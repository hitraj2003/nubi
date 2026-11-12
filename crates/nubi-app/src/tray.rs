use crate::host::NativeHost;
use global_hotkey::{hotkey::HotKey, GlobalHotKeyManager};
use nubi_core::{host::with, settings::Settings};
use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    TrayIcon, TrayIconBuilder,
};

pub struct Tray {
    _icon: TrayIcon,
    dnd: CheckMenuItem,
    sounds: CheckMenuItem,
    styles: Vec<CheckMenuItem>,
}

impl Tray {
    pub fn new(host: &NativeHost) -> Result<Self, Box<dyn std::error::Error>> {
        let menu = Menu::new();
        menu.append(&MenuItem::with_id("open", "Open settings", true, None))?;
        menu.append(&PredefinedMenuItem::separator())?;
        let style = Submenu::new("Change style", true);
        let mut styles = Vec::new();
        for (value, label) in [
            ("island", "Notch"),
            ("ring", "Floating ring"),
            ("taskbar", "Taskbar strip"),
        ] {
            let item = CheckMenuItem::with_id(value, label, true, false, None);
            style.append(&item)?;
            styles.push(item);
        }
        menu.append(&style)?;
        menu.append(&PredefinedMenuItem::separator())?;
        let dnd = CheckMenuItem::with_id("dnd", "Do not disturb", true, false, None);
        let sounds = CheckMenuItem::with_id("sounds", "Sounds", true, false, None);
        menu.append_items(&[
            &dnd,
            &sounds,
            &MenuItem::with_id("refresh", "Refresh usage", true, None),
            &PredefinedMenuItem::separator(),
            &MenuItem::with_id("quit", "Quit", true, None),
        ])?;
        let pixels = image::load_from_memory(include_bytes!("../../../src-tauri/icons/32x32.png"))?
            .into_rgba8();
        let (width, height) = pixels.dimensions();
        let icon = tray_icon::Icon::from_rgba(pixels.into_raw(), width, height)?;
        let tray = TrayIconBuilder::new()
            .with_id("nubi-native")
            .with_icon(icon)
            .with_tooltip(if nubi_core::isolation::development() {
                "Nubi (dev)"
            } else {
                "Nubi"
            })
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()?;
        let result = Self {
            _icon: tray,
            dnd,
            sounds,
            styles,
        };
        result.refresh(host);
        Ok(result)
    }

    pub fn refresh(&self, host: &NativeHost) {
        with(host, |state| {
            self.dnd.set_checked(state.dnd_manual);
            self.sounds.set_checked(state.settings.sounds);
            for (item, mode) in self.styles.iter().zip(["island", "ring", "taskbar"]) {
                item.set_checked(state.settings.display_mode == mode);
            }
        });
    }
}

pub struct Shortcuts {
    manager: GlobalHotKeyManager,
    current: Vec<HotKey>,
    pub launch: Option<u32>,
}

impl Shortcuts {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            manager: GlobalHotKeyManager::new().map_err(|e| e.to_string())?,
            current: Vec::new(),
            launch: None,
        })
    }

    pub fn apply(&mut self, settings: &Settings) -> Result<(), String> {
        let (dnd, launch) = parse_shortcuts(settings)?;
        let next: Vec<_> = [dnd, launch].into_iter().flatten().collect();
        if next == self.current {
            return Ok(());
        }
        self.manager
            .unregister_all(&self.current)
            .map_err(|e| e.to_string())?;
        if let Err(error) = self.manager.register_all(&next) {
            let _ = self.manager.unregister_all(&next);
            let _ = self.manager.register_all(&self.current);
            return Err(error.to_string());
        }
        self.current = next;
        self.launch = launch.map(|hotkey| hotkey.id());
        Ok(())
    }
}

impl Drop for Shortcuts {
    fn drop(&mut self) {
        let _ = self.manager.unregister_all(&self.current);
    }
}

pub fn parse_shortcuts(settings: &Settings) -> Result<(Option<HotKey>, Option<HotKey>), String> {
    let parse = |text: &str| {
        if text.trim().is_empty() {
            Ok(None)
        } else {
            text.parse::<HotKey>().map(Some).map_err(|e| e.to_string())
        }
    };
    let dnd = parse(&settings.dnd_hotkey)?;
    let launch = if settings.launcher_enabled {
        parse(&settings.launch_hotkey)?
    } else {
        None
    };
    if dnd.is_some() && dnd == launch {
        return Err("The two keyboard shortcuts must differ.".into());
    }
    Ok((dnd, launch))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shortcuts_respect_disabled_empty_and_conflicting_settings() {
        let mut settings = Settings::default();
        assert!(parse_shortcuts(&settings).unwrap().1.is_some());
        settings.launch_hotkey = settings.dnd_hotkey.clone();
        assert!(parse_shortcuts(&settings).is_err());
        settings.launcher_enabled = false;
        assert!(parse_shortcuts(&settings).unwrap().1.is_none());
        settings.dnd_hotkey.clear();
        assert!(parse_shortcuts(&settings).unwrap().0.is_none());
    }
}
