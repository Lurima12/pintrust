//! Persisted user preferences.
//!
//! The application only stores a handful of values, so a small key=value file
//! under `$XDG_CONFIG_HOME/pintrust` is used instead of pulling in a settings
//! schema. The file is written atomically and unknown or malformed entries fall
//! back to the defaults, which keeps upgrades safe.

use std::fs;
use std::path::PathBuf;

/// Where the navigation buttons are placed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NavPlacement {
    /// Inside the title bar, where they were before floating buttons existed.
    Header,
    TopLeft,
    TopRight,
    BottomLeft,
    #[default]
    BottomRight,
}

impl NavPlacement {
    /// Order used by the preferences drop-down.
    pub const ALL: [Self; 5] = [
        Self::BottomRight,
        Self::BottomLeft,
        Self::TopRight,
        Self::TopLeft,
        Self::Header,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Header => "In the title bar",
            Self::TopLeft => "Top left",
            Self::TopRight => "Top right",
            Self::BottomLeft => "Bottom left",
            Self::BottomRight => "Bottom right",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Header => "header",
            Self::TopLeft => "top-left",
            Self::TopRight => "top-right",
            Self::BottomLeft => "bottom-left",
            Self::BottomRight => "bottom-right",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|placement| placement.key() == value)
    }
}

/// One of the buttons that can be shown in the navigation area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavItem {
    Back,
    Forward,
    Home,
    Reload,
}

impl NavItem {
    /// Every item, used to build the buttons and to validate stored orders.
    pub const ALL: [Self; 4] = [Self::Back, Self::Forward, Self::Home, Self::Reload];

    /// Order shown until the user changes it.
    pub const DEFAULT_ORDER: [Self; 4] = [Self::Home, Self::Back, Self::Forward, Self::Reload];

    pub fn label(self) -> &'static str {
        match self {
            Self::Back => "Back",
            Self::Forward => "Forward",
            Self::Home => "Home",
            Self::Reload => "Reload",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::Back => "go-previous-symbolic",
            Self::Forward => "go-next-symbolic",
            Self::Home => "go-home-symbolic",
            Self::Reload => "view-refresh-symbolic",
        }
    }

    pub fn action(self) -> &'static str {
        match self {
            Self::Back => "win.back",
            Self::Forward => "win.forward",
            Self::Home => "win.home",
            Self::Reload => "win.reload",
        }
    }

    pub fn tooltip(self) -> &'static str {
        match self {
            Self::Back => "Back (Alt+Left)",
            Self::Forward => "Forward (Alt+Right)",
            Self::Home => "Home (Alt+Home)",
            Self::Reload => "Reload (Ctrl+R)",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Back => "back",
            Self::Forward => "forward",
            Self::Home => "home",
            Self::Reload => "reload",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|item| item.key() == value)
    }
}

/// User-visible preferences.
#[derive(Clone, Debug)]
pub struct Settings {
    pub placement: NavPlacement,
    /// Button order from left to right. Always contains every item once.
    pub order: [NavItem; 4],
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            placement: NavPlacement::default(),
            order: NavItem::DEFAULT_ORDER,
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        let mut settings = Self::default();
        let Ok(contents) = fs::read_to_string(Self::path()) else {
            return settings;
        };

        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };

            match key.trim() {
                "placement" => {
                    if let Some(placement) = NavPlacement::parse(value.trim()) {
                        settings.placement = placement;
                    }
                }
                "order" => {
                    let items: Vec<NavItem> = value
                        .split(',')
                        .filter_map(|entry| NavItem::parse(entry.trim()))
                        .collect();
                    // Only accept a complete, duplicate-free permutation;
                    // anything else would make a button disappear.
                    if items.len() == NavItem::ALL.len()
                        && items
                            .iter()
                            .enumerate()
                            .all(|(index, item)| !items[..index].contains(item))
                    {
                        settings.order = [items[0], items[1], items[2], items[3]];
                    }
                }
                _ => {}
            }
        }

        settings
    }

    pub fn save(&self) {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            if let Err(error) = fs::create_dir_all(parent) {
                eprintln!("pintrust: cannot create {}: {error}", parent.display());
                return;
            }
        }

        let order = self
            .order
            .iter()
            .map(|item| item.key())
            .collect::<Vec<_>>()
            .join(",");

        let contents = format!(
            "# PintRust preferences. Edited by the application.\nplacement={}\norder={}\n",
            self.placement.key(),
            order
        );

        // Write a sibling file first so an interrupted write cannot leave a
        // half-written settings file behind.
        let temporary = path.with_extension("conf.tmp");
        if let Err(error) = fs::write(&temporary, contents) {
            eprintln!("pintrust: cannot write {}: {error}", temporary.display());
            return;
        }
        if let Err(error) = fs::rename(&temporary, &path) {
            eprintln!("pintrust: cannot replace {}: {error}", path.display());
            let _ = fs::remove_file(&temporary);
        }
    }

    fn path() -> PathBuf {
        let mut directory = glib::user_config_dir();
        directory.push("pintrust");
        directory.push("settings.conf");
        directory
    }

    /// Moves the button at `index` one step left (`delta` < 0) or right.
    pub fn move_item(&mut self, index: usize, delta: isize) -> bool {
        let Some(target) = index.checked_add_signed(delta) else {
            return false;
        };
        if target >= self.order.len() {
            return false;
        }
        self.order.swap(index, target);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_item_has_a_stable_key() {
        for item in NavItem::ALL {
            assert_eq!(NavItem::parse(item.key()), Some(item));
        }
        for placement in NavPlacement::ALL {
            assert_eq!(NavPlacement::parse(placement.key()), Some(placement));
        }
    }

    #[test]
    fn moving_swaps_neighbours_and_stops_at_the_edges() {
        let mut settings = Settings::default();
        assert!(!settings.move_item(0, -1));
        assert!(settings.move_item(0, 1));
        assert_eq!(
            settings.order,
            [NavItem::Back, NavItem::Home, NavItem::Forward, NavItem::Reload]
        );
        assert!(!settings.move_item(3, 1));
    }

    #[test]
    fn the_default_order_starts_with_home() {
        assert_eq!(
            Settings::default().order,
            [NavItem::Home, NavItem::Back, NavItem::Forward, NavItem::Reload]
        );
    }

    #[test]
    fn documented_file_parses() {
        let mut settings = Settings::default();
        let contents = "placement=top-left\norder=reload,home,forward,back\n";
        for line in contents.lines() {
            let (key, value) = line.split_once('=').unwrap();
            match key {
                "placement" => settings.placement = NavPlacement::parse(value).unwrap(),
                "order" => {
                    let items: Vec<_> = value.split(',').map(|v| NavItem::parse(v).unwrap()).collect();
                    settings.order = [items[0], items[1], items[2], items[3]];
                }
                _ => unreachable!(),
            }
        }
        assert_eq!(settings.placement, NavPlacement::TopLeft);
        assert_eq!(
            settings.order,
            [NavItem::Reload, NavItem::Home, NavItem::Forward, NavItem::Back]
        );
    }
}
