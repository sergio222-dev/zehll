use system_tray::menu::{MenuDiff, MenuItem, MenuType, ToggleType, TrayMenu};

pub const PADDING: u16 = 6;
// Space between the bar and the menu.
pub const GAP: u16 = 6;
pub const ROW_PADDING: u16 = 10;
pub const ICON_WIDTH: u32 = 16;
pub const SPACING: u32 = 8;
// Rough average advance of a 13px proportional font; popups need their size up front.
const CHAR_WIDTH: f32 = 7.0;
const MIN_WIDTH: u32 = 120;
pub const ROW_HEIGHT: u32 = 30;
pub const SEPARATOR_HEIGHT: u32 = 9;

/// Applies property updates sent by the application to the cached menu tree.
pub fn apply_diffs(menu: &mut TrayMenu, diffs: &[MenuDiff]) {
    for diff in diffs {
        let Some(item) = find_mut(&mut menu.submenus, diff.id) else {
            continue;
        };
        let update = &diff.update;
        if let Some(label) = &update.label {
            item.label = label.clone();
        }
        if let Some(enabled) = update.enabled {
            item.enabled = enabled;
        }
        if let Some(visible) = update.visible {
            item.visible = visible;
        }
        if let Some(toggle_state) = update.toggle_state {
            item.toggle_state = toggle_state;
        }
    }
}

fn find_mut(items: &mut [MenuItem], id: i32) -> Option<&mut MenuItem> {
    for item in items {
        if item.id == id {
            return Some(item);
        }
        if let Some(found) = find_mut(&mut item.submenu, id) {
            return Some(found);
        }
    }
    None
}

/// Visible entries of the submenu reached by following `path` (a list of item ids).
pub fn entries<'a>(menu: &'a TrayMenu, path: &[i32]) -> Vec<&'a MenuItem> {
    let mut items = &menu.submenus;
    for id in path {
        match items.iter().find(|item| item.id == *id) {
            Some(item) => items = &item.submenu,
            None => return Vec::new(),
        }
    }
    items.iter().filter(|item| item.visible).collect()
}

/// Whether rows need a leading column for toggle marks or the submenu "back" arrow.
pub fn has_leading_column(entries: &[&MenuItem], in_submenu: bool) -> bool {
    in_submenu || entries.iter().any(|item| item.toggle_type != ToggleType::CannotBeToggled)
}

/// Popup size for the given entries, plus a "back" row when inside a submenu.
pub fn size(entries: &[&MenuItem], in_submenu: bool) -> (u32, u32) {
    let label_chars = entries
        .iter()
        .filter_map(|item| item.label.as_deref())
        .chain(in_submenu.then_some("Back"))
        .map(|label| label.chars().count())
        .max()
        .unwrap_or_default();
    let leading = if has_leading_column(entries, in_submenu) { ICON_WIDTH + SPACING } else { 0 };
    let trailing = if entries.iter().any(|item| !item.submenu.is_empty()) { SPACING + ICON_WIDTH } else { 0 };
    let width = (label_chars as f32 * CHAR_WIDTH).ceil() as u32 + leading + trailing + 2 * (PADDING + ROW_PADDING) as u32;

    let rows: u32 = entries
        .iter()
        .map(|item| match item.menu_type {
            MenuType::Separator => SEPARATOR_HEIGHT,
            MenuType::Standard => ROW_HEIGHT,
        })
        .sum();
    let back = if in_submenu { ROW_HEIGHT + SEPARATOR_HEIGHT } else { 0 };
    (width.max(MIN_WIDTH), rows.max(ROW_HEIGHT) + back + 2 * PADDING as u32)
}
