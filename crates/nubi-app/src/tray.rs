use crate::host::NativeHost;
use global_hotkey::{hotkey::HotKey, GlobalHotKeyManager};
use nubi_core::{host::with, settings::Settings};
use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    TrayIcon, TrayIconBuilder,
};

pub struct Tray {
    _icon: TrayIcon,
    dnd: CheckMenuItem,
    sounds: CheckMenuItem,
}

impl Tray {
    pub fn new(host: &NativeHost) -> Result<Self, Box<dyn std::error::Error>> {
        let menu = Menu::new();
        menu.append(&MenuItem::with_id("open", "Open settings", true, None))?;
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
        let pixels = image::load_from_memory(include_bytes!("../assets/32x32.png"))?
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
        };
        result.refresh(host);
        Ok(result)
    }

    pub fn refresh(&self, host: &NativeHost) {
        with(host, |state| {
            self.dnd.set_checked(state.dnd_manual);
            self.sounds.set_checked(state.settings.sounds);
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
        // Keep existing registrations while testing new combinations.
        let mut added = Vec::new();
        for key in next.iter().filter(|key| !self.current.contains(key)) {
            if self.manager.register(*key).is_err() {
                for registered in added {
                    self.manager.unregister(registered).map_err(|e| e.to_string())?;
                }
                let shortcut = if Some(*key) == dnd { &settings.dnd_hotkey } else { &settings.launch_hotkey };
                return Err(format!("Windows could not register {shortcut}. Choose another shortcut or disable it."));
            }
            added.push(*key);
        }
        for key in self.current.iter().filter(|key| !next.contains(key)) {
            self.manager.unregister(*key).map_err(|e| e.to_string())?;
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
