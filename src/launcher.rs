//! App launcher: a centered search box with pages, toggled with `zehll:launcherToggle`.

use std::collections::HashMap;

use iced::keyboard::{self, Modifiers, key::Named};
use iced::widget::{self, Column, Row, Space, Stack, button, column, container, image, mouse_area, row, svg, text, text_input};
use iced::{Alignment, Border, Color, Element, Font, Length, Padding, mouse};

use crate::apps::{self, App, Score};
use crate::keybinds::Keybind;
use crate::{calc, clipboard, emoji, files, fuzzy, gallery, tray, wallhaven};
use crate::{BODY_BOLD, BODY_FONT, Bar, CONDENSED_BOLD, CONDENSED_HEAVY, ICON_FONT, Message, glow_style, line, panel_style, shape, slanted};

/// Rows shown at once; the list scrolls past them.
pub const VISIBLE_ROWS: usize = 7;
const SEARCH_HEIGHT: f32 = 54.0;
const ROW_HEIGHT: f32 = 46.0;
const SECTION_HEIGHT: f32 = 34.0;
const LIST_PADDING: f32 = 8.0;
const FOOTER_HEIGHT: f32 = 34.0;
const APP_ICON: f32 = 22.0;
const CARD_HEIGHT: f32 = 120.0;
// List rows the calculator's result card takes the place of.
const CARD_ROWS: usize = 3;
// Width of the list beside a preview, and the preview's box and detail rows.
const PREVIEW_LIST_WIDTH: f32 = 300.0;
const PREVIEW_HEIGHT: f32 = 150.0;
const DETAIL_HEIGHT: f32 = 30.0;
// Open-window marker, the green of the media player's source tag.
const RUNNING: Color = Color::from_rgb8(0x5f, 0xe3, 0x9a);
pub const WIDTH: u32 = 720;
/// The Wallpapers page is wider and taller, for a grid of 4 x 3 tiles.
pub const GALLERY_SIZE: (u32, u32) = (1040, 640);
const TILE_HEIGHT: f32 = 146.0;
const TILE_GAP: f32 = 12.0;
const ACCENT_HEIGHT: f32 = 58.0;
const WALLHAVEN_TILE_HEIGHT: f32 = 150.0;
/// The Emojis page is taller, for rows of large emojis.
pub const EMOJI_SIZE: (u32, u32) = (WIDTH, 560);
const EMOJI_PADDING: f32 = 12.0;
/// Named, or fallback picks a font with outline-only emoji glyphs for many of them.
const EMOJI_FONT: Font = Font::with_name("Noto Color Emoji");
/// Height the emoji grid lays its lines out in: the surface less the search, footer, dividers
/// and the grid's padding.
const EMOJI_GRID_HEIGHT: f32 = EMOJI_SIZE.1 as f32 - SEARCH_HEIGHT - FOOTER_HEIGHT - 2.0 - 2.0 * EMOJI_PADDING;
// Search bar, list and footer, plus the two dividers.
pub const HEIGHT: u32 = (SEARCH_HEIGHT + SECTION_HEIGHT + ROW_HEIGHT * VISIBLE_ROWS as f32 + LIST_PADDING + FOOTER_HEIGHT + 2.0) as u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Home,
    Files,
    Clipboard,
    Keybinds,
    Calculator,
    Wallpapers,
    Wallhaven,
    Emojis,
}

impl Page {
    /// Pages opened from Home, in the order Home lists them.
    const CHILDREN: [Page; 7] = [Page::Files, Page::Clipboard, Page::Keybinds, Page::Calculator, Page::Wallpapers, Page::Wallhaven, Page::Emojis];

    /// Label of the search bar chip.
    pub fn chip(self) -> &'static str {
        match self {
            Page::Home => "HOME",
            Page::Files => "FILES",
            Page::Clipboard => "CLIP",
            Page::Keybinds => "KEYS",
            Page::Calculator => "CALC",
            Page::Wallpapers => "WALLPAPERS",
            Page::Wallhaven => "WALLHAVEN",
            Page::Emojis => "EMOJIS",
        }
    }

    /// Material Symbols icon.
    pub fn icon(self) -> &'static str {
        match self {
            Page::Home => "search",
            Page::Files => "folder",
            Page::Clipboard => "content_paste",
            Page::Keybinds => "keyboard",
            Page::Calculator => "calculate",
            Page::Wallpapers => "wallpaper",
            Page::Wallhaven => "travel_explore",
            Page::Emojis => "mood",
        }
    }

    pub fn placeholder(self) -> &'static str {
        match self {
            Page::Home => "Search apps, pages and commands",
            Page::Files => "Search files and folders",
            Page::Clipboard => "Filter clipboard entries",
            Page::Keybinds => "Search keybindings",
            Page::Calculator => "Do math, convert units, or search past calculations",
            Page::Wallpapers => "Filter wallpapers by name",
            Page::Wallhaven => "Search wallpapers on Wallhaven",
            Page::Emojis => "Search emojis",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Page::Home => "Home",
            Page::Files => "Files",
            Page::Clipboard => "Clipboard",
            Page::Keybinds => "Keybinds",
            Page::Calculator => "Calculator",
            Page::Wallpapers => "Wallpapers",
            Page::Wallhaven => "Wallhaven",
            Page::Emojis => "Emojis",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Page::Home => "",
            Page::Files => "Recent folders and file search",
            Page::Clipboard => "Clipboard history",
            Page::Keybinds => "Hyprland keybindings",
            Page::Calculator => "Math, units and currency",
            Page::Wallpapers => "Desktop wallpaper and accent color",
            Page::Wallhaven => "Search and download wallpapers",
            Page::Emojis => "Type or copy an emoji",
        }
    }
}

/// System commands listed in Home.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Power {
    Lock,
    Suspend,
    Logout,
    Restart,
    Shutdown,
}

impl Power {
    const ALL: [Power; 5] = [Power::Lock, Power::Suspend, Power::Logout, Power::Restart, Power::Shutdown];

    fn title(self) -> &'static str {
        match self {
            Power::Lock => "Lock",
            Power::Suspend => "Suspend",
            Power::Logout => "Logout",
            Power::Restart => "Restart",
            Power::Shutdown => "Shutdown",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Power::Lock => "lock",
            Power::Suspend => "bedtime",
            Power::Logout => "logout",
            Power::Restart => "restart_alt",
            Power::Shutdown => "power_settings_new",
        }
    }

    /// Commands that end the session ask for a second Enter.
    pub fn needs_confirm(self) -> bool {
        matches!(self, Power::Logout | Power::Restart | Power::Shutdown)
    }

    /// Shell command, or `None` for Logout, which asks Hyprland to exit.
    pub fn command(self) -> Option<&'static str> {
        match self {
            Power::Lock => Some("loginctl lock-session"),
            Power::Suspend => Some("systemctl suspend"),
            Power::Logout => None,
            Power::Restart => Some("systemctl reboot"),
            Power::Shutdown => Some("systemctl poweroff"),
        }
    }

    fn description(self) -> &'static str {
        match self {
            Power::Lock => "loginctl lock-session",
            Power::Suspend => "systemctl suspend",
            Power::Logout => "Ends the Hyprland session; asks to confirm",
            Power::Restart => "systemctl reboot; asks to confirm",
            Power::Shutdown => "systemctl poweroff; asks to confirm",
        }
    }
}

/// A step waiting for a second request before it runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pending {
    Power(Power),
    ClearClipboard,
}

/// What a launcher row does when activated.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Rows only shown, like keybindings.
    None,
    Open(Page),
    /// Index into `Launcher::history`.
    Recall(usize),
    /// Index into `Launcher::apps`.
    App(usize),
    Power(Power),
    /// Open it, or with Ctrl+Enter put its path in the search.
    File(files::Entry),
    /// Copy it back, or with Ctrl+Enter pin or unpin it.
    Clip(clipboard::Entry),
}

#[derive(Clone, Debug)]
pub enum ItemIcon {
    /// Material Symbols icon.
    Symbol(&'static str),
    App(Option<tray::Icon>),
}

/// Font of a row title.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TitleStyle {
    Body,
    /// Key combinations, in condensed capitals.
    Keys,
    /// Calculations, in monospace with the result as the tag.
    Mono,
}

/// What Enter does on the Calculator page.
#[derive(Clone, Debug, PartialEq)]
pub enum CalcEnter {
    /// Copy the answer and remember it.
    Copy(calc::Answer),
    /// Put a past calculation back in the search.
    Recall(String),
    Nothing,
}

pub fn calculator_enter(launcher: &Launcher) -> CalcEnter {
    let picked = || match items(launcher).into_iter().nth(launcher.selected).map(|item| item.action) {
        Some(Action::Recall(index)) => launcher.history.get(index).map(|entry| CalcEnter::Recall(entry.input.clone())),
        _ => None,
    };
    match (launcher.answer(), launcher.moved) {
        (Some(answer), false) => CalcEnter::Copy(answer.clone()),
        _ => picked().unwrap_or(CalcEnter::Nothing),
    }
}

/// Text shown when a page has no rows.
pub fn empty_text(page: Page) -> &'static str {
    match page {
        Page::Calculator => "NO MATCHING HISTORY",
        Page::Wallpapers => "NO WALLPAPERS IN ~/Pictures/Wallpapers",
        Page::Wallhaven | Page::Emojis => "NO RESULTS",
        Page::Home | Page::Files | Page::Clipboard | Page::Keybinds => "NO RESULTS",
    }
}

/// One row of the launcher list.
#[derive(Clone, Debug)]
pub struct Item {
    pub icon: ItemIcon,
    pub title: String,
    pub title_style: TitleStyle,
    pub description: String,
    /// Right-aligned label: the kind of row, or a keybinding's description.
    pub tag: String,
    /// An app with an open window.
    pub running: bool,
    pub action: Action,
}

/// Rows of the current page. Home ranks apps, pages and commands by how well they match the
/// search plus each app's launch score; with an empty search, launched apps come first.
pub fn items(launcher: &Launcher) -> Vec<Item> {
    match launcher.page() {
        Page::Home => home_items(launcher),
        Page::Keybinds => keybind_items(launcher),
        Page::Calculator => history_items(launcher),
        Page::Files => file_items(launcher),
        Page::Clipboard => clip_items(launcher),
        // A grid of tiles, not rows: see `gallery_body`.
        Page::Wallpapers | Page::Wallhaven | Page::Emojis => Vec::new(),
    }
}

/// Clipboard history matching the search, pinned entries first, else newest first; only the
/// pinned ones while the pin button is on.
fn clip_items(launcher: &Launcher) -> Vec<Item> {
    let mut entries: Vec<&clipboard::Entry> = launcher
        .clips
        .iter()
        .filter(|entry| (!launcher.pinned_only || launcher.is_pinned(entry)) && clipboard::matches(&launcher.query, &entry.preview))
        .collect();
    entries.sort_by_key(|entry| !launcher.is_pinned(entry));
    entries
        .into_iter()
        .map(|entry| Item {
            icon: ItemIcon::Symbol(match (launcher.is_pinned(entry), entry.is_image()) {
                (true, _) => "push_pin",
                (false, true) => "image",
                (false, false) => "content_copy",
            }),
            title: entry.preview.clone(),
            title_style: TitleStyle::Mono,
            description: String::new(),
            tag: String::new(),
            running: false,
            action: Action::Clip(entry.clone()),
        })
        .collect()
}

/// The entry the Clipboard preview shows.
pub fn selected_clip(launcher: &Launcher) -> Option<clipboard::Entry> {
    match items(launcher).into_iter().nth(launcher.selected)?.action {
        Action::Clip(entry) => Some(entry),
        _ => None,
    }
}

/// Key/value rows under the Clipboard preview. cliphist keeps no time, so the date is unknown.
fn clip_rows(launcher: &Launcher, entry: &clipboard::Entry) -> Vec<(&'static str, String)> {
    vec![
        ("TYPE", entry.type_label()),
        ("MIME", entry.mime()),
        ("DATE", String::from("—")),
        ("HASH", launcher.decoded(entry.id).map_or_else(|| String::from("…"), |decoded| decoded.hash.clone())),
    ]
}

/// The Clipboard preview: the decoded text or image, and the entry's details.
fn clip_preview<'a>(theme: crate::theme::Theme, launcher: &'a Launcher) -> Element<'a, Message> {
    let Some(entry) = selected_clip(launcher) else {
        return preview_pane(theme, Space::new().into(), Vec::new());
    };
    let decoded = launcher.decoded(entry.id);
    let content: Element<'a, Message> = match (decoded.and_then(|decoded| decoded.image.clone()), decoded.and_then(|decoded| decoded.text.as_deref())) {
        (Some(handle), _) => container(image(handle).content_fit(iced::ContentFit::Contain)).padding(10).center(Length::Fill).into(),
        (None, Some(preview)) => container(text(preview).font(Font::MONOSPACE).size(11).color(theme.text_muted))
            .padding(10)
            .width(Length::Fill)
            .height(Length::Fill)
            .clip(true)
            .into(),
        (None, None) => container(icon_colored(if entry.is_image() { "image" } else { "content_paste" }, 64.0, theme.text_dim)).center(Length::Fill).into(),
    };
    preview_pane(theme, content, clip_rows(launcher, &entry))
}

/// Recent folders, or recent and found entries matching the search, best first.
fn file_items(launcher: &Launcher) -> Vec<Item> {
    files::rank(&launcher.query, &launcher.recent, &launcher.found, &launcher.home)
        .into_iter()
        .map(|entry| Item {
            icon: ItemIcon::Symbol(if entry.is_dir { "folder" } else { "description" }),
            title: entry.name.clone(),
            title_style: TitleStyle::Body,
            description: String::new(),
            tag: String::new(),
            running: false,
            action: Action::File(entry),
        })
        .collect()
}

/// The entry the Files preview shows.
pub fn selected_file(launcher: &Launcher) -> Option<files::Entry> {
    match items(launcher).into_iter().nth(launcher.selected)?.action {
        Action::File(entry) => Some(entry),
        _ => None,
    }
}

/// Pages that show the selected row beside the list.
pub fn has_preview(page: Page) -> bool {
    matches!(page, Page::Files | Page::Clipboard)
}

/// Key/value rows under the Files preview; `…` while the details load.
fn preview_rows(launcher: &Launcher, entry: &files::Entry) -> Vec<(&'static str, String)> {
    let details = launcher.details(&entry.path);
    let pending = || String::from("…");
    vec![
        ("NAME", entry.name.clone()),
        ("TYPE", details.map_or_else(pending, |details| details.mime.clone())),
        ("SIZE", details.map_or_else(pending, |details| files::format_size(details.size))),
        ("PATH", files::tilde(&entry.path, &launcher.home)),
        ("ITEMS", details.map_or_else(pending, |details| details.items.map_or_else(|| String::from("—"), |items| items.to_string()))),
    ]
}

/// The Files preview: folder icon, or the start of a readable file, and the entry's details.
fn file_preview<'a>(theme: crate::theme::Theme, launcher: &'a Launcher) -> Element<'a, Message> {
    let Some(entry) = selected_file(launcher) else {
        return preview_pane(theme, Space::new().into(), Vec::new());
    };
    let text_preview = launcher.details(&entry.path).and_then(|details| details.preview.as_deref());
    let content: Element<'a, Message> = match (entry.is_dir, text_preview) {
        (false, Some(preview)) => container(text(preview).font(Font::MONOSPACE).size(11).color(theme.text_muted).wrapping(text::Wrapping::None))
            .padding(10)
            .width(Length::Fill)
            .height(Length::Fill)
            .clip(true)
            .into(),
        (true, _) => container(icon_colored("folder", 64.0, theme.accent)).center(Length::Fill).into(),
        (false, None) => container(icon_colored("description", 64.0, theme.text_dim)).center(Length::Fill).into(),
    };
    preview_pane(theme, content, preview_rows(launcher, &entry))
}

/// A large box with `content`, then key/value rows each over a divider; shared by the pages
/// that preview their selected row.
fn preview_pane<'a>(theme: crate::theme::Theme, content: Element<'a, Message>, rows: Vec<(&'static str, String)>) -> Element<'a, Message> {
    let preview = container(content).width(Length::Fill).height(PREVIEW_HEIGHT).style(move |_| container::Style {
        background: Some(theme.section_background.into()),
        border: Border { color: theme.border, width: 1.0, radius: 0.0.into() },
        ..Default::default()
    });
    let rows = rows.into_iter().map(|(key, value)| {
        let value = container(text(value).font(BODY_BOLD).size(13).color(theme.text).wrapping(text::Wrapping::None)).width(Length::Fill).align_right(Length::Fill).clip(true);
        column![
            row![text(key).font(CONDENSED_BOLD).size(12).color(theme.text_muted), value].spacing(16).height(DETAIL_HEIGHT).align_y(Alignment::Center),
            line(theme.border),
        ]
        .into()
    });
    column![preview, Column::with_children(rows)].spacing(10).padding(16).width(Length::Fill).height(Length::Fill).into()
}

/// Past calculations, or those matching the search by input or result, best first.
fn history_items(launcher: &Launcher) -> Vec<Item> {
    let mut ranked: Vec<(f32, Item)> = launcher
        .history
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            let score = [&entry.input, &entry.result].iter().filter_map(|field| fuzzy::score(&launcher.query, field)).reduce(f32::max)?;
            let item = Item {
                icon: ItemIcon::Symbol("history"),
                title: entry.input.clone(),
                title_style: TitleStyle::Mono,
                description: String::new(),
                tag: entry.result.clone(),
                running: false,
                action: Action::Recall(index),
            };
            Some((score, item))
        })
        .collect();
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
    ranked.into_iter().map(|(_, item)| item).collect()
}

/// Every described keybinding, or those matching the search by keys or description, best first.
fn keybind_items(launcher: &Launcher) -> Vec<Item> {
    let mut ranked: Vec<(f32, Item)> = launcher
        .keybinds
        .iter()
        .filter_map(|bind| {
            let score = [&bind.keys, &bind.description].iter().filter_map(|field| fuzzy::score(&launcher.query, field)).reduce(f32::max)?;
            let item = Item {
                icon: ItemIcon::Symbol("keyboard"),
                title: bind.keys.clone(),
                title_style: TitleStyle::Keys,
                description: String::new(),
                tag: bind.description.clone(),
                running: false,
                action: Action::None,
            };
            Some((score, item))
        })
        .collect();
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
    ranked.into_iter().map(|(_, item)| item).collect()
}

fn home_items(launcher: &Launcher) -> Vec<Item> {
    let query = launcher.query.as_str();
    let best = |fields: &[&str]| fields.iter().filter_map(|field| fuzzy::score(query, field)).reduce(f32::max);
    let mut ranked: Vec<(f64, Item)> = Vec::new();
    for (index, app) in launcher.apps.iter().enumerate() {
        let Some(score) = best(&[&app.name, &app.id, &app.description]) else {
            continue;
        };
        let launch = launcher.scores.get(&app.id).map_or(0.0, |score| score.at(launcher.now));
        let item = Item {
            icon: ItemIcon::App(app.icon.clone()),
            title: app.name.clone(),
            title_style: TitleStyle::Body,
            description: app.description.clone(),
            tag: String::from("APPLICATION"),
            running: launcher.running.iter().any(|class| apps::owns_window(app, class)),
            action: Action::App(index),
        };
        ranked.push((score as f64 + launch, item));
    }
    for page in Page::CHILDREN {
        if let Some(score) = best(&[page.title(), page.description()]) {
            let item = Item {
                icon: ItemIcon::Symbol(page.icon()),
                title: page.title().to_string(),
                title_style: TitleStyle::Body,
                description: page.description().to_string(),
                tag: String::from("PAGE"),
                running: false,
                action: Action::Open(page),
            };
            ranked.push((score as f64, item));
        }
    }
    for power in Power::ALL {
        if let Some(score) = best(&[power.title()]) {
            let item = Item {
                icon: ItemIcon::Symbol(power.icon()),
                title: power.title().to_string(),
                title_style: TitleStyle::Body,
                description: power.description().to_string(),
                tag: String::from(if launcher.confirm == Some(Pending::Power(power)) { "CONFIRM ↵" } else { "COMMAND" }),
                running: false,
                action: Action::Power(power),
            };
            ranked.push((score as f64, item));
        }
    }
    // Stable: equal scores keep apps (by name), then pages, then commands.
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
    ranked.into_iter().map(|(_, item)| item).collect()
}

/// Header above the list.
pub fn section(launcher: &Launcher, count: usize) -> String {
    match (launcher.page(), launcher.query.is_empty()) {
        (Page::Home, true) => String::from("SUGGESTIONS"),
        (Page::Keybinds, true) => String::from("KEYBINDS"),
        (Page::Files, true) => String::from("RECENT FOLDERS (ZOXIDE)"),
        (Page::Clipboard, _) if launcher.pinned_only => format!("PINNED ({count})"),
        (Page::Clipboard, true) => format!("HISTORY ({count})"),
        (Page::Calculator, _) => String::from("HISTORY"),
        _ => format!("RESULTS ({count})"),
    }
}

/// Key hints of the footer: key and what it does.
pub fn hints(page: Page) -> &'static [(&'static str, &'static str)] {
    match page {
        Page::Home => &[("↵", "open"), ("^↵", "action"), ("^1–9", "pick"), ("ESC", "close")],
        Page::Keybinds => &[("↑↓", "move"), ("⇧ESC", "back"), ("ESC", "close")],
        Page::Calculator => &[("↵", "copy"), ("^↵", "reuse"), ("TAB", "history"), ("⇧ESC", "back")],
        Page::Files => &[("↵", "open"), ("^↵", "path"), ("⇧ESC", "back")],
        Page::Clipboard => &[("↵", "copy"), ("^↵", "pin"), ("⇧ESC", "back")],
        Page::Wallpapers => &[("↑↓←→", "move"), ("↵", "apply"), ("^R", "random"), ("ALT ←→", "accent"), ("⇧ESC", "back")],
        Page::Wallhaven => &[("↑↓←→", "move"), ("↵", "apply"), ("^↵", "save + apply"), ("⇧ESC", "back")],
        Page::Emojis => &[("↑↓←→", "move"), ("↵", "type"), ("^↵", "copy"), ("ALT ←→", "tone")],
    }
}

pub fn input_id() -> widget::Id {
    widget::Id::new("launcher-search")
}

pub fn view(bar: &Bar) -> Element<'_, Message> {
    let theme = bar.theme;
    let launcher = &bar.launcher;
    let page = launcher.page();
    let items = items(launcher);
    let len = items.len();

    let chip = container(
        row![
            icon_colored(page.icon(), 16.0, theme.on_accent),
            text(page.chip()).font(CONDENSED_HEAVY).size(16).color(theme.on_accent),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding(Padding { top: 0.0, right: 26.0, bottom: 0.0, left: 16.0 })
    .center_y(Length::Fill);
    let editing_key = page == Page::Wallhaven && launcher.wallhaven.editing_key.is_some();
    let placeholder = match editing_key {
        true => "Paste your Wallhaven API key · Enter saves it, empty removes it",
        false => page.placeholder(),
    };
    let input = text_input(placeholder, &launcher.query)
        .secure(editing_key)
        .id(input_id())
        .on_input(Message::LauncherQuery)
        .font(BODY_FONT)
        .size(18)
        .padding(0)
        .style(move |_, _| text_input::Style {
            background: Color::TRANSPARENT.into(),
            border: Border::default(),
            icon: theme.text_muted,
            placeholder: theme.text_dim,
            value: theme.text,
            selection: Color { a: 0.3, ..theme.glow },
        });
    let mut search = row![slanted(chip, 14.0, theme.accent), container(input).width(Length::Fill).center_y(Length::Fill)];
    if page == Page::Clipboard {
        let pin_color = if launcher.pinned_only { theme.accent } else { theme.text };
        search = search.push(bracket_button(theme, "push_pin", pin_color, Message::ClipboardPinnedOnly));
        // The clear button asks for a second press, saying so beside it.
        if launcher.clear_armed() {
            search = search.push(text("CONFIRM").font(CONDENSED_BOLD).size(12).color(theme.accent));
        }
        let clear_color = if launcher.clear_armed() { theme.accent } else { theme.text };
        search = search.push(bracket_button(theme, "delete_sweep", clear_color, Message::ClipboardClear));
    }
    if page == Page::Wallpapers {
        let shown = launcher.gallery_shown().len();
        if shown > 0 {
            let position = format!("{} OF {shown}", launcher.selected + 1);
            search = search.push(text(position).font(CONDENSED_BOLD).size(12).color(theme.text_muted));
        }
        search = search.push(bracket_button(theme, "shuffle", theme.text, Message::LauncherNav(Nav::Random)));
        search = search.push(bracket_button(theme, "folder_open", theme.text, Message::OpenWallpaperFolder));
    }
    if page == Page::Wallhaven {
        let color = if editing_key || launcher.wallhaven.filters.has_key() { theme.accent } else { theme.text };
        search = search.push(bracket_button(theme, "key", color, Message::WallhavenKey));
    }
    let search = search
        .spacing(14)
        .align_y(Alignment::Center)
        .padding(Padding::ZERO.right(18.0))
        .height(SEARCH_HEIGHT);

    let header = container(text(section(launcher, len)).font(CONDENSED_BOLD).size(12).color(theme.text_muted))
        .padding(Padding { top: 12.0, right: 18.0, bottom: 6.0, left: 18.0 });
    let list: Element<'_, Message> = match items.is_empty() {
        true => {
            let empty = text(empty_text(page)).font(CONDENSED_BOLD).size(14).color(theme.text_muted);
            match page {
                Page::Calculator => container(column![icon_colored("calculate", 40.0, theme.text_dim), empty].spacing(10).align_x(Alignment::Center)).center(Length::Fill).into(),
                _ => container(empty).center(Length::Fill).into(),
            }
        }
        false => {
            let rows = items.into_iter().enumerate().skip(launcher.first).take(launcher.visible_rows());
            let rows = Column::with_children(rows.map(|(index, item)| item_row(theme, item, index, index == launcher.selected)));
            mouse_area(container(rows).height(Length::Fill))
                .on_scroll(move |delta| {
                    let rows = match delta {
                        mouse::ScrollDelta::Lines { y, .. } => -y.signum() as i32,
                        mouse::ScrollDelta::Pixels { y, .. } => -(y / ROW_HEIGHT).round() as i32,
                    };
                    Message::LauncherScroll(rows, len)
                })
                .into()
        }
    };
    let body: Element<'_, Message> = match launcher.answer().filter(|_| page == Page::Calculator) {
        _ if page == Page::Wallpapers => gallery_body(theme, launcher, bar.wallpaper.path.as_deref()),
        _ if page == Page::Wallhaven => wallhaven_body(theme, launcher),
        _ if page == Page::Emojis => emoji_body(theme, launcher),
        Some(answer) => column![answer_card(theme, answer), header, list].height(Length::Fill).into(),
        None if has_preview(page) => {
            let divider = container(Space::new()).width(1).height(Length::Fill).style(move |_| container::background(theme.border));
            let preview = match page {
                Page::Clipboard => clip_preview(theme, launcher),
                _ => file_preview(theme, launcher),
            };
            row![column![header, list].width(PREVIEW_LIST_WIDTH).height(Length::Fill), divider, preview].height(Length::Fill).into()
        }
        None => column![header, list].height(Length::Fill).into(),
    };

    let hints = Row::with_children(hints(page).iter().map(|(key, label)| {
        let key = container(text(*key).font(CONDENSED_BOLD).size(12).color(theme.text)).padding([1, 6]).style(move |_| container::Style {
            border: Border { color: theme.border, width: 1.0, radius: 0.0.into() },
            ..Default::default()
        });
        row![key, text(*label).font(BODY_FONT).size(12).color(theme.text_muted)].spacing(6).align_y(Alignment::Center).into()
    }))
    .spacing(18);
    let hints = match page {
        Page::Emojis => hints.push(Space::new().width(Length::Fill)).push(emoji_name(theme, launcher)),
        _ => hints,
    };
    let footer = container(hints)
        .padding([0, 18])
        .height(FOOTER_HEIGHT)
        .width(Length::Fill)
        .center_y(FOOTER_HEIGHT)
        .style(move |_| container::background(theme.bar_background));

    let content = column![search, line(theme.border), body, line(theme.border), footer];
    let background = container(Space::new()).width(Length::Fill).height(Length::Fill).style(move |_| panel_style(theme));
    Stack::new()
        .push(background)
        .push(content)
        .push(shape(shape::Kind::Brackets { arm: 18.0, width: 2.0 }, theme.accent))
        .into()
}

/// The Wallpapers page: a grid of thumbnails, 4 x 3 at a time, and the accent strip.
fn gallery_body<'a>(theme: crate::theme::Theme, launcher: &'a Launcher, current: Option<&std::path::Path>) -> Element<'a, Message> {
    let shown = launcher.gallery_shown();
    if shown.is_empty() {
        let empty = match launcher.gallery.entries.is_empty() {
            true => empty_text(Page::Wallpapers),
            false => "NO RESULTS",
        };
        return container(text(empty).font(CONDENSED_BOLD).size(14).color(theme.text_muted)).center(Length::Fill).into();
    }
    let first = launcher.gallery.first_row * gallery::COLUMNS;
    let tiles = shown.iter().enumerate().skip(first).take(gallery::COLUMNS * gallery::ROWS).map(|(at, &index)| {
        let entry = &launcher.gallery.entries[index];
        let tile = Tile {
            thumb: launcher.gallery.thumbs.get(&entry.path).map(|thumb| thumb.as_path()),
            placeholder: "",
            name: entry.name.clone(),
            size: entry.size,
            selected: at == launcher.selected,
            current: current == Some(entry.path.as_path()),
            busy: false,
            height: TILE_HEIGHT,
            on_press: Message::GalleryClick(at),
        };
        grid_tile(theme, tile)
    });
    let rows = tile_rows(tiles.collect(), gallery::COLUMNS);
    let grid = mouse_area(container(rows).padding([14, 16]).height(Length::Fill)).on_scroll(|delta| {
        let rows = match delta {
            mouse::ScrollDelta::Lines { y, .. } => -y.signum() as i32,
            mouse::ScrollDelta::Pixels { y, .. } => -(y / TILE_HEIGHT).round() as i32,
        };
        Message::GalleryScroll(rows)
    });

    let colors = launcher.gallery_selected().and_then(|entry| launcher.gallery.colors.get(&entry.path));
    let mut strip = row![text("ACCENT").font(CONDENSED_BOLD).size(12).color(theme.text_muted)].spacing(18).align_y(Alignment::Center);
    for (index, color) in colors.into_iter().flatten().enumerate() {
        let Ok(color) = color.parse::<Color>() else {
            continue;
        };
        let ring = if index == launcher.gallery.accent { theme.glow } else { Color::TRANSPARENT };
        let swatch = container(container(shape(shape::Kind::Diamond, color)).width(24).height(24)).center(34).style(move |_| container::Style {
            border: Border { color: ring, width: 2.0, radius: 0.0.into() },
            ..Default::default()
        });
        strip = strip.push(button(swatch).on_press(Message::GalleryAccent(index)).padding(0).style(|_, _| button::Style::default()));
    }
    let note = match colors {
        Some(_) => "From the selected wallpaper · applies with it",
        None => "Reading the colors…",
    };
    let strip = strip.push(Space::new().width(Length::Fill)).push(text(note).font(BODY_FONT).size(12).color(theme.text_muted));
    let strip = container(strip).padding([0, 18]).height(ACCENT_HEIGHT).center_y(ACCENT_HEIGHT);
    column![grid, line(theme.border), strip].height(Length::Fill).into()
}

/// The Wallhaven page: the filters, then the results, 3 x 3 at a time.
fn wallhaven_body<'a>(theme: crate::theme::Theme, launcher: &'a Launcher) -> Element<'a, Message> {
    let browser = &launcher.wallhaven;
    let filters = &browser.filters;
    let toggle = |label: &'static str, on: bool, enabled: bool, message: Message| -> Element<'a, Message> {
        let (background, color, border) = match (on, enabled) {
            (true, _) => (theme.accent, theme.on_accent, theme.accent),
            (false, true) => (Color::TRANSPARENT, theme.text_muted, theme.border),
            (false, false) => (Color::TRANSPARENT, theme.text_dim, Color { a: 0.5, ..theme.border }),
        };
        let chip = container(text(label).font(CONDENSED_BOLD).size(13).color(color)).padding([0, 10]).height(28).center_y(28).style(move |_| container::Style {
            background: Some(background.into()),
            border: Border { color: border, width: 1.0, radius: 0.0.into() },
            ..Default::default()
        });
        let chip = button(chip).padding(0).style(|_, _| button::Style::default());
        match enabled {
            true => chip.on_press(message).into(),
            false => chip.into(),
        }
    };
    let separator = || container(Space::new()).width(1).height(22).style(move |_| container::background(theme.border));
    let mut bar = row![].spacing(8).align_y(Alignment::Center);
    for (index, label) in ["GENERAL", "ANIME", "PEOPLE"].into_iter().enumerate() {
        bar = bar.push(toggle(label, filters.categories[index], true, Message::WallhavenCategory(index)));
    }
    bar = bar.push(separator());
    for (index, label) in ["SFW", "SKETCHY", "NSFW"].into_iter().enumerate() {
        bar = bar.push(toggle(label, filters.purity[index], index == 0 || filters.has_key(), Message::WallhavenPurity(index)));
    }
    bar = bar.push(separator());
    bar = bar.push(toggle(filters.sorting.label(), false, true, Message::WallhavenSorting));
    let order = if filters.descending { "arrow_downward" } else { "arrow_upward" };
    bar = bar.push(bracket_button(theme, order, theme.text, Message::WallhavenOrder));
    bar = bar.push(bracket_button(theme, "refresh", theme.text, Message::WallhavenReload));
    bar = bar.push(Space::new().width(Length::Fill));
    if browser.last_page > 0 {
        let pages = format!("PAGE {} / {}", browser.page, browser.last_page);
        bar = bar.push(text(pages).font(CONDENSED_BOLD).size(12).color(theme.text_muted));
    }
    let bar = container(bar).padding([0, 16]).height(46).center_y(46);

    let status = match (&browser.error, browser.loading, browser.results.is_empty()) {
        (Some(error), _, true) => Some(error.to_uppercase()),
        (None, true, true) => Some(String::from("SEARCHING…")),
        (None, false, true) => Some(String::from(empty_text(Page::Wallhaven))),
        _ => None,
    };
    let grid: Element<'a, Message> = match status {
        Some(status) => container(text(status).font(CONDENSED_BOLD).size(14).color(theme.text_muted)).center(Length::Fill).into(),
        None => {
            let first = browser.first_row * wallhaven::COLUMNS;
            let tiles = browser.results.iter().enumerate().skip(first).take(wallhaven::COLUMNS * wallhaven::ROWS).map(|(at, wallpaper)| {
                let thumb = browser.thumbs.get(&wallpaper.id);
                let tile = Tile {
                    thumb: thumb.and_then(|thumb| thumb.as_deref()),
                    placeholder: if matches!(thumb, Some(None)) { "CLICK TO RETRY" } else { "LOADING" },
                    name: format!("{}.{}", wallpaper.id, wallpaper.file_name().rsplit('.').next().unwrap_or("")),
                    size: wallpaper.size,
                    selected: at == launcher.selected,
                    current: false,
                    busy: browser.applying.as_deref() == Some(wallpaper.id.as_str()),
                    height: WALLHAVEN_TILE_HEIGHT,
                    on_press: Message::WallhavenClick(at),
                };
                grid_tile(theme, tile)
            });
            let rows = tile_rows(tiles.collect(), wallhaven::COLUMNS);
            mouse_area(container(rows).padding([14, 16]).height(Length::Fill))
                .on_scroll(|delta| {
                    let rows = match delta {
                        mouse::ScrollDelta::Lines { y, .. } => -y.signum() as i32,
                        mouse::ScrollDelta::Pixels { y, .. } => -(y / WALLHAVEN_TILE_HEIGHT).round() as i32,
                    };
                    Message::WallhavenScroll(rows)
                })
                .into()
        }
    };
    let mut body = column![bar, line(theme.border), grid].height(Length::Fill);
    // An error after some results (a later page, a download) shows under them.
    if let (Some(error), false) = (&browser.error, browser.results.is_empty()) {
        body = body.push(container(text(error.as_str()).font(BODY_FONT).size(12).color(theme.accent)).padding([6, 18]));
    }
    body.into()
}

/// The Emojis page: the grid's lines from the first one shown, as many as fit.
fn emoji_body<'a>(theme: crate::theme::Theme, launcher: &'a Launcher) -> Element<'a, Message> {
    let picker = &launcher.emoji;
    if picker.shown.is_empty() {
        return container(text(empty_text(Page::Emojis)).font(CONDENSED_BOLD).size(14).color(theme.text_muted)).center(Length::Fill).into();
    }
    let mut height = 0.0;
    let mut lines = Column::new().spacing(emoji::GAP);
    for line in &picker.lines[picker.first_line.min(picker.lines.len())..] {
        if height >= EMOJI_GRID_HEIGHT {
            break;
        }
        height += line.height();
        lines = lines.push(match line {
            emoji::Line::Header(title) => {
                container(text(*title).font(CONDENSED_BOLD).size(13).color(theme.text_dim)).height(emoji::HEADER_HEIGHT).align_bottom(emoji::HEADER_HEIGHT).into()
            }
            emoji::Line::Row { emojis, counts } => {
                let tiles = emojis.clone().map(|index| emoji_tile(theme, picker, index, *counts, index == launcher.selected)).collect();
                tile_row(tiles, emoji::COLUMNS, emoji::GAP)
            }
        });
    }
    mouse_area(container(lines).padding([EMOJI_PADDING, 16.0]).width(Length::Fill).height(Length::Fill).clip(true))
        .on_scroll(|delta| {
            let rows = match delta {
                mouse::ScrollDelta::Lines { y, .. } => -y.signum() as i32,
                mouse::ScrollDelta::Pixels { y, .. } => -(y / emoji::TILE_HEIGHT).round() as i32,
            };
            Message::EmojiScroll(rows)
        })
        .into()
}

/// One emoji, in the chosen tone, with a + when it has tones and its count when asked.
fn emoji_tile<'a>(theme: crate::theme::Theme, picker: &'a emoji::Picker, index: usize, counts: bool, selected: bool) -> Element<'a, Message> {
    let base = picker.shown[index];
    let glyph = container(text(emoji::with_tone(base, picker.tone).as_str()).font(EMOJI_FONT).size(44).shaping(text::Shaping::Advanced)).center(Length::Fill);
    let mut layers = Stack::new().push(glyph);
    if emoji::has_tones(base) {
        layers = layers.push(container(text("+").font(CONDENSED_BOLD).size(13).color(theme.text_dim)).padding([4, 8]).align_right(Length::Fill));
    }
    let count = emoji::count(&picker.counts, base);
    if counts && count > 0 {
        let label = text(format!("{count}×")).font(Font::MONOSPACE).size(10).color(theme.text_dim);
        layers = layers.push(container(label).padding([4, 8]).align_right(Length::Fill).align_bottom(Length::Fill));
    }
    let (outline, width) = if selected { (theme.glow, 2.0) } else { (theme.border, 1.0) };
    let frame = container(Space::new()).width(Length::Fill).height(Length::Fill).style(move |_| container::Style {
        border: Border { color: outline, width, radius: 0.0.into() },
        ..Default::default()
    });
    let tile = container(layers.push(frame)).width(Length::Fill).height(emoji::TILE_HEIGHT).clip(true);
    mouse_area(tile).on_press(Message::EmojiClick(index)).interaction(mouse::Interaction::Pointer).into()
}

/// The selected emoji's name and shortcode, at the right of the footer.
fn emoji_name<'a>(theme: crate::theme::Theme, launcher: &'a Launcher) -> Element<'a, Message> {
    let Some(base) = launcher.emoji.shown.get(launcher.selected) else {
        return Space::new().into();
    };
    let emoji = emoji::with_tone(base, launcher.emoji.tone);
    let mut name = row![text(emoji.name()).font(BODY_FONT).size(12).color(theme.text)].spacing(10).align_y(Alignment::Center);
    if let Some(code) = base.shortcode() {
        name = name.push(text(format!(":{code}:")).font(Font::MONOSPACE).size(11).color(theme.text_muted));
    }
    name.into()
}

/// What a grid tile shows.
struct Tile<'a> {
    thumb: Option<&'a std::path::Path>,
    /// Over an empty tile: still loading, or how to retry.
    placeholder: &'static str,
    name: String,
    size: Option<(u32, u32)>,
    selected: bool,
    current: bool,
    /// A bar under the label while the full image downloads.
    busy: bool,
    height: f32,
    on_press: Message,
}

/// One wallpaper: its thumbnail (or a placeholder while it is made), name and size.
fn grid_tile<'a>(theme: crate::theme::Theme, tile: Tile<'a>) -> Element<'a, Message> {
    let picture: Element<'a, Message> = match tile.thumb {
        Some(thumb) => image(image::Handle::from_path(thumb)).width(Length::Fill).height(Length::Fill).content_fit(iced::ContentFit::Cover).into(),
        None => container(text(tile.placeholder).font(CONDENSED_BOLD).size(12).color(theme.text_dim))
            .center(Length::Fill)
            .style(move |_| container::background(theme.section_background))
            .into(),
    };
    let size = tile.size.map(|(w, h)| format!("{w} × {h}")).unwrap_or_default();
    let label = container(
        row![
            text(tile.name).font(CONDENSED_BOLD).size(12).color(theme.text).wrapping(text::Wrapping::None),
            Space::new().width(Length::Fill),
            text(size).font(BODY_FONT).size(11).color(theme.text_muted),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([0, 10])
    .height(26)
    .width(Length::Fill)
    .center_y(26)
    .style(move |_| container::background(Color { a: 0.85, ..theme.popup_background }));
    let mut bottom = column![];
    if tile.busy {
        bottom = bottom.push(container(Space::new()).width(Length::Fill).height(3).style(move |_| container::background(theme.accent)));
    }
    let bottom = bottom.push(label);
    let mut layers = Stack::new().push(picture).push(container(bottom).height(Length::Fill).align_bottom(Length::Fill));
    if tile.current {
        let tag = container(text("CURRENT").font(CONDENSED_HEAVY).size(12).color(theme.on_accent)).padding(Padding { top: 1.0, right: 14.0, bottom: 1.0, left: 8.0 });
        layers = layers.push(container(slanted(tag, 7.0, theme.accent)).padding(8));
    }
    let outline = if tile.selected { theme.glow } else { theme.border };
    let width = if tile.selected { 2.0 } else { 1.0 };
    // On top: a container's own border is covered by its content.
    let frame = container(Space::new()).width(Length::Fill).height(Length::Fill).style(move |_| container::Style {
        border: Border { color: outline, width, radius: 0.0.into() },
        ..Default::default()
    });
    let framed = container(layers.push(frame)).width(Length::Fill).height(tile.height).clip(true);
    mouse_area(framed).on_press(tile.on_press).interaction(mouse::Interaction::Pointer).into()
}

/// Rows of `columns` tiles, the last one padded so its tiles keep their width.
fn tile_rows<'a>(tiles: Vec<Element<'a, Message>>, columns: usize) -> Element<'a, Message> {
    let mut rows = Vec::new();
    let mut tiles = tiles.into_iter().peekable();
    while tiles.peek().is_some() {
        rows.push(tile_row(tiles.by_ref().take(columns).collect(), columns, TILE_GAP));
    }
    Column::with_children(rows).spacing(TILE_GAP).into()
}

/// One row of at most `columns` tiles `gap` apart, padded so its tiles keep their width.
fn tile_row<'a>(mut cells: Vec<Element<'a, Message>>, columns: usize, gap: f32) -> Element<'a, Message> {
    while cells.len() < columns {
        cells.push(Space::new().width(Length::Fill).into());
    }
    Row::with_children(cells).spacing(gap).into()
}

/// The expression as qalc read it, then the result, as in the design's result card.
fn answer_card(theme: crate::theme::Theme, answer: &calc::Answer) -> Element<'_, Message> {
    let side = |content: String, color: Color| {
        container(text(content).font(CONDENSED_BOLD).size(30).color(color).wrapping(text::Wrapping::None)).center_x(Length::Fill).center_y(Length::Fill).clip(true)
    };
    let sign = container(text(answer.sign).font(CONDENSED_BOLD).size(26).color(theme.text_muted)).center(44).style(move |_| container::Style {
        border: Border { color: theme.border, width: 1.0, radius: 0.0.into() },
        ..Default::default()
    });
    let card = row![side(answer.expression.clone(), theme.text), container(sign).height(Length::Fill).center_y(Length::Fill), side(answer.result.clone(), theme.accent)]
        .height(Length::Fill)
        .align_y(Alignment::Center);
    container(container(card).height(Length::Fill).style(move |_| container::Style {
        background: Some(theme.section_background.into()),
        border: Border { color: theme.border, width: 1.0, radius: 0.0.into() },
        ..Default::default()
    }))
    .padding(Padding { top: 16.0, right: 16.0, bottom: 0.0, left: 16.0 })
    .height(CARD_HEIGHT + 16.0)
    .into()
}

/// An icon in corner brackets, like the panel's buttons.
fn bracket_button<'a>(theme: crate::theme::Theme, name: &'static str, color: Color, message: Message) -> Element<'a, Message> {
    let content = Stack::new().push(container(icon_colored(name, 18.0, color)).center(34)).push_under(shape(shape::Kind::BRACKETS, theme.bracket));
    button(content).on_press(message).padding(0).style(move |_, status| glow_style(theme, status, 10.0)).into()
}

fn item_row<'a>(theme: crate::theme::Theme, item: Item, index: usize, selected: bool) -> Element<'a, Message> {
    let icon_color = if selected { theme.glow } else { theme.text };
    let icon: Element<'a, Message> = match item.icon {
        ItemIcon::Symbol(name) => icon_colored(name, 18.0, icon_color),
        ItemIcon::App(Some(tray::Icon::Svg(handle))) => svg(handle).width(APP_ICON).height(APP_ICON).into(),
        ItemIcon::App(Some(tray::Icon::Raster(handle))) => image(handle).width(APP_ICON).height(APP_ICON).into(),
        ItemIcon::App(None) => icon_colored("apps", 18.0, icon_color),
    };
    // A small diamond under the icon marks an app with an open window.
    let running: Element<'a, Message> = match item.running {
        true => container(shape(shape::Kind::Diamond, RUNNING)).width(8).height(8).into(),
        false => Space::new().height(8).into(),
    };
    let icon = column![container(icon).center_x(26).height(APP_ICON), running].spacing(1).align_x(Alignment::Center);
    let title = match item.title_style {
        TitleStyle::Body => text(item.title).font(BODY_BOLD).size(15),
        TitleStyle::Keys => text(item.title).font(CONDENSED_BOLD).size(16),
        TitleStyle::Mono => text(item.title).font(Font::MONOSPACE).size(14),
    }
    .wrapping(text::Wrapping::None);
    // Without a description the title takes the room, cut at the edge, as clipboard text runs long.
    let (title, description): (Element<'a, Message>, Element<'a, Message>) = match item.description.is_empty() {
        true => (container(title).width(Length::Fill).clip(true).into(), Space::new().into()),
        false => (
            title.into(),
            container(text(item.description).font(BODY_FONT).size(12).color(theme.text_muted).wrapping(text::Wrapping::None))
                .width(Length::Fill)
                .clip(true)
                .into(),
        ),
    };
    let trash: Element<'a, Message> = match (&item.action, selected) {
        (Action::Clip(entry), true) => button(container(icon_colored("delete", 18.0, theme.text)).center(28))
            .on_press(Message::ClipboardDelete(entry.clone()))
            .padding(0)
            .style(move |_, status| glow_style(theme, status, 6.0))
            .into(),
        _ => Space::new().into(),
    };
    let content = row![
        icon,
        title,
        description,
        match item.title_style {
            // A keybinding's description reads as text, not as a label.
            TitleStyle::Keys => text(item.tag).font(BODY_FONT).size(13).color(theme.text_muted),
            TitleStyle::Body => text(item.tag).font(CONDENSED_BOLD).size(12).color(theme.text_muted),
            TitleStyle::Mono => text(item.tag).font(CONDENSED_BOLD).size(17).color(theme.text),
        },
        trash,
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    let row = button(container(content).padding([0, 12]).center_y(ROW_HEIGHT).width(Length::Fill))
        .on_press(Message::LauncherActivate(index))
        .padding(0)
        .width(Length::Fill)
        .style(move |_, status| {
            let status = if selected { button::Status::Hovered } else { status };
            glow_style(theme, status, 0.0)
        });
    container(row).padding([0, 8]).into()
}

fn icon_colored<'a>(name: &'static str, size: f32, color: Color) -> Element<'a, Message> {
    text(name).font(ICON_FONT).size(size).color(color).shaping(text::Shaping::Advanced).into()
}

/// Navigation requested by a key press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nav {
    Close,
    Back,
    Move(i32),
    Activate { secondary: bool },
    Pick(usize),
    /// Left or right, for grids.
    Sideways(i32),
    /// Alt+Left or Alt+Right: the previous or next accent color.
    Accent(i32),
    /// Ctrl+R: a random wallpaper.
    Random,
}

/// Surface size of the launcher on `page`.
pub fn size(page: Page) -> (u32, u32) {
    match page {
        Page::Wallpapers | Page::Wallhaven => GALLERY_SIZE,
        Page::Emojis => EMOJI_SIZE,
        _ => (WIDTH, HEIGHT),
    }
}

pub struct Launcher {
    pub query: String,
    // Pages entered from Home, the current one last.
    stack: Vec<Page>,
    pub selected: usize,
    // First row shown; follows the selection.
    pub first: usize,
    /// Installed apps, kept between openings while a fresh list loads.
    pub apps: Vec<App>,
    /// Launch scores by desktop id.
    pub scores: HashMap<String, Score>,
    /// Described Hyprland binds, read when the Keybinds page opens.
    pub keybinds: Vec<Keybind>,
    /// Past calculations, newest first.
    pub history: Vec<calc::Entry>,
    /// Latest qalc answer and the search it was computed for.
    answer: Option<(String, calc::Answer)>,
    /// The selection was moved since the last search, so Enter acts on the row.
    moved: bool,
    /// Classes of the open windows, read when the launcher opens.
    pub running: Vec<String>,
    /// Unix seconds when the launcher opened, for decaying launch scores.
    pub now: i64,
    /// Command or clear waiting for a second request.
    confirm: Option<Pending>,
    /// Home folder, for `~` in searches and paths.
    pub home: String,
    /// Folders from zoxide, best first, read when the Files page opens.
    pub recent: Vec<files::Entry>,
    /// Latest fd results that were for the search shown.
    found: Vec<files::Entry>,
    /// Details of previewed entries by path.
    file_details: HashMap<String, files::Details>,
    /// Clipboard history, newest first, read when the Clipboard page opens.
    pub clips: Vec<clipboard::Entry>,
    /// Latest history read asked for; older ones arriving late are dropped.
    clips_request: u64,
    /// Pinned clipboard entries by their pin key.
    pub pins: Vec<String>,
    /// The pin button shows only the pinned entries.
    pub pinned_only: bool,
    /// Decoded clipboard entries by id.
    decoded: HashMap<u64, clipboard::Decoded>,
    /// The Wallpapers page.
    pub gallery: gallery::Gallery,
    /// The Wallhaven page.
    pub wallhaven: wallhaven::Browser,
    /// The Emojis page.
    pub emoji: emoji::Picker,
    /// The window focused when the launcher opened, which gets a picked emoji.
    pub previous_window: Option<String>,
}

impl Launcher {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            stack: Vec::new(),
            selected: 0,
            first: 0,
            apps: Vec::new(),
            scores: HashMap::new(),
            keybinds: Vec::new(),
            history: Vec::new(),
            answer: None,
            moved: false,
            running: Vec::new(),
            now: 0,
            confirm: None,
            home: String::new(),
            recent: Vec::new(),
            found: Vec::new(),
            file_details: HashMap::new(),
            clips: Vec::new(),
            clips_request: 0,
            pins: Vec::new(),
            pinned_only: false,
            decoded: HashMap::new(),
            gallery: gallery::Gallery::default(),
            wallhaven: wallhaven::Browser::default(),
            emoji: emoji::Picker::default(),
            previous_window: None,
        }
    }

    /// With the saved launch scores, calculator history and home folder.
    pub fn load() -> Self {
        let home = std::env::var("HOME").unwrap_or_default();
        let mut launcher = Self { scores: apps::load_scores(), history: calc::load_history(), pins: clipboard::load_pins(), home, ..Self::new() };
        launcher.wallhaven.filters = wallhaven::Filters::parse(&crate::wallpaper::load_section("wallhaven"));
        launcher.emoji = emoji::Picker::load();
        launcher
    }

    /// The calculator's answer, if it is for the current search.
    pub fn answer(&self) -> Option<&calc::Answer> {
        self.answer.as_ref().filter(|(query, _)| *query == self.query).map(|(_, answer)| answer)
    }

    /// Stores qalc's answer for `query`, dropping answers to older searches that arrive late.
    pub fn set_answer(&mut self, query: String, answer: Option<calc::Answer>) {
        if query == self.query {
            self.answer = answer.map(|answer| (query, answer));
        }
    }

    /// Stores fd's results for `query`, dropping results to older searches that arrive late.
    /// Kept while the next search runs, so typing re-ranks them instead of emptying the list.
    pub fn set_found(&mut self, query: String, found: Vec<files::Entry>) {
        if query == self.query {
            self.found = found;
        }
    }

    pub fn set_details(&mut self, path: String, details: files::Details) {
        if self.file_details.len() >= files::LIMIT {
            self.file_details.clear();
        }
        self.file_details.insert(path, details);
    }

    pub fn details(&self, path: &str) -> Option<&files::Details> {
        self.file_details.get(path)
    }

    /// Rows that fit under the search, fewer under the calculator's result card.
    pub fn visible_rows(&self) -> usize {
        match self.page() == Page::Calculator && self.answer().is_some() {
            true => VISIBLE_ROWS - CARD_ROWS,
            false => VISIBLE_ROWS,
        }
    }

    /// Back to an empty Home, keeping the loaded apps and scores.
    pub fn reset(&mut self) {
        self.stack.clear();
        self.set_query(String::new());
    }

    /// Whether `power` should run now: at once, or on the second request when it needs confirming.
    pub fn confirm(&mut self, power: Power) -> bool {
        if !power.needs_confirm() {
            self.confirm = None;
            return true;
        }
        self.second_request(Pending::Power(power))
    }

    /// Whether the clipboard history should be wiped now: on the second press of the clear button.
    pub fn confirm_clear(&mut self) -> bool {
        self.second_request(Pending::ClearClipboard)
    }

    pub fn clear_armed(&self) -> bool {
        self.confirm == Some(Pending::ClearClipboard)
    }

    fn second_request(&mut self, pending: Pending) -> bool {
        if self.confirm == Some(pending) {
            self.confirm = None;
            return true;
        }
        self.confirm = Some(pending);
        false
    }

    /// Starts a history read; its answer must carry the returned number to be kept.
    pub fn begin_clips(&mut self) -> u64 {
        self.clips_request += 1;
        self.clips_request
    }

    pub fn set_clips(&mut self, request: u64, clips: Vec<clipboard::Entry>) {
        if request == self.clips_request {
            self.clips = clips;
            self.keep_selection();
        }
    }

    /// Drops a deleted entry at once, before cliphist answers.
    pub fn remove_clip(&mut self, id: u64) {
        self.clips.retain(|entry| entry.id != id);
        self.confirm = None;
        self.keep_selection();
    }

    pub fn clear_clips(&mut self) {
        self.clips.clear();
        self.decoded.clear();
        self.keep_selection();
    }

    pub fn is_pinned(&self, entry: &clipboard::Entry) -> bool {
        self.pins.iter().any(|pin| pin == entry.pin_key())
    }

    pub fn toggle_pin(&mut self, entry: &clipboard::Entry) {
        clipboard::toggle_pin(&mut self.pins, entry.pin_key());
        self.confirm = None;
        // Pinned entries move to the top; the selection follows the entry while it is listed.
        match items(self).iter().position(|item| matches!(&item.action, Action::Clip(listed) if listed.id == entry.id)) {
            Some(at) => {
                self.selected = at;
                let visible = self.visible_rows();
                self.first = self.first.min(at).max((at + 1).saturating_sub(visible));
            }
            None => self.keep_selection(),
        }
    }

    pub fn toggle_pinned_only(&mut self) {
        self.pinned_only = !self.pinned_only;
        self.selected = 0;
        self.first = 0;
        self.confirm = None;
    }

    pub fn set_decoded(&mut self, id: u64, decoded: clipboard::Decoded) {
        // Images are kept decoded, so only the last few entries are.
        if self.decoded.len() >= 32 {
            self.decoded.clear();
        }
        self.decoded.insert(id, decoded);
    }

    pub fn decoded(&self, id: u64) -> Option<&clipboard::Decoded> {
        self.decoded.get(&id)
    }

    /// Keeps the selection on the list after it shrank.
    fn keep_selection(&mut self) {
        let len = items(self).len();
        self.selected = self.selected.min(len.saturating_sub(1));
        self.first = self.first.min(self.selected);
    }

    pub fn page(&self) -> Page {
        self.stack.last().copied().unwrap_or(Page::Home)
    }

    /// Indices into the gallery's entries of the tiles shown, for the search.
    pub fn gallery_shown(&self) -> Vec<usize> {
        gallery::filter(&self.gallery.entries, &self.query)
    }

    /// The wallpaper under the selection.
    pub fn gallery_selected(&self) -> Option<&gallery::Entry> {
        self.gallery_shown().get(self.selected).map(|&index| &self.gallery.entries[index])
    }

    /// Moves the selection among the tiles, scrolling by rows to keep it shown; a new tile
    /// starts on its first accent.
    pub fn gallery_move(&mut self, delta: i32) {
        let len = self.gallery_shown().len();
        self.gallery_select(gallery::step(self.selected, delta, len));
    }

    pub fn gallery_select(&mut self, index: usize) {
        if index != self.selected {
            self.gallery.accent = 0;
        }
        self.selected = index;
        self.gallery.first_row = gallery::first_row(index, self.gallery.first_row, gallery::ROWS, gallery::COLUMNS);
    }

    /// Moves the selection among Wallhaven's results, scrolling by rows to keep it shown.
    pub fn wallhaven_move(&mut self, delta: i32) {
        let len = self.wallhaven.results.len();
        self.wallhaven_select(gallery::step(self.selected, delta, len));
    }

    pub fn wallhaven_select(&mut self, index: usize) {
        self.selected = index;
        self.wallhaven.first_row = gallery::first_row(index, self.wallhaven.first_row, wallhaven::ROWS, wallhaven::COLUMNS);
    }

    pub fn wallhaven_scroll(&mut self, rows: i32) {
        let len = self.wallhaven.results.len();
        let last = len.div_ceil(wallhaven::COLUMNS).saturating_sub(wallhaven::ROWS) as i64;
        self.wallhaven.first_row = (self.wallhaven.first_row as i64 + rows as i64).clamp(0, last) as usize;
    }

    /// Selects the emoji at `index`, scrolling to keep its row (and the header above it) shown.
    pub fn emoji_select(&mut self, index: usize) {
        self.selected = index.min(self.emoji.shown.len().saturating_sub(1));
        if let Some(line) = emoji::line_of(&self.emoji.lines, self.selected) {
            self.emoji.first_line = emoji::first_line(&self.emoji.lines, line, self.emoji.first_line, EMOJI_GRID_HEIGHT);
        }
    }

    /// Moves the selection `rows` rows up or down.
    pub fn emoji_vertical(&mut self, rows: i32) {
        self.emoji_select(emoji::vertical(&self.emoji.lines, self.selected, rows));
    }

    /// Moves the selection to the emoji beside it.
    pub fn emoji_sideways(&mut self, delta: i32) {
        self.emoji_select(emoji::sideways(self.selected, delta, self.emoji.shown.len()));
    }

    /// Scrolls the emoji grid by lines, keeping the selection where it is.
    pub fn emoji_scroll(&mut self, rows: i32) {
        self.emoji.first_line = emoji::scroll(&self.emoji.lines, self.emoji.first_line, rows, EMOJI_GRID_HEIGHT);
    }

    /// Scrolls the grid by rows, keeping the selection where it is.
    pub fn gallery_scroll(&mut self, rows: i32) {
        let len = self.gallery_shown().len();
        let last = len.div_ceil(gallery::COLUMNS).saturating_sub(gallery::ROWS) as i64;
        self.gallery.first_row = (self.gallery.first_row as i64 + rows as i64).clamp(0, last) as usize;
    }

    pub fn open(&mut self, page: Page) {
        if page == Page::Files {
            self.found.clear();
            self.file_details.clear();
        }
        if page == Page::Clipboard {
            self.pinned_only = false;
            self.decoded.clear();
        }
        self.stack.push(page);
        self.set_query(String::new());
    }

    pub fn set_query(&mut self, query: String) {
        self.query = query;
        self.selected = 0;
        self.first = 0;
        self.gallery.first_row = 0;
        self.gallery.accent = 0;
        self.confirm = None;
        self.moved = false;
        if self.page() == Page::Emojis {
            self.emoji.refresh(&self.query);
        }
    }

    /// Clears the search, else goes back a page. False when there is nothing to undo.
    pub fn back(&mut self) -> bool {
        if !self.query.is_empty() {
            self.set_query(String::new());
            return true;
        }
        let went_back = self.stack.pop().is_some();
        self.set_query(String::new());
        went_back
    }

    /// Moves the selection, wrapping at both ends, and scrolls to keep it visible.
    pub fn move_selection(&mut self, delta: i32, len: usize) {
        self.confirm = None;
        self.moved = true;
        let visible = self.visible_rows();
        if len == 0 {
            self.selected = 0;
            self.first = 0;
            return;
        }
        self.selected = (self.selected as i64 + delta as i64).rem_euclid(len as i64) as usize;
        if self.selected < self.first {
            self.first = self.selected;
        } else if self.selected >= self.first + visible {
            self.first = self.selected + 1 - visible;
        }
    }

    /// Scrolls by whole rows without moving past the list.
    pub fn scroll(&mut self, rows: i32, len: usize) {
        let last = len.saturating_sub(self.visible_rows()) as i64;
        self.first = (self.first as i64 + rows as i64).clamp(0, last) as usize;
    }
}

/// Maps a key press to navigation; typing is left to the search field.
/// The navigation a key event triggers. Escape acts when released, other keys when pressed:
/// closing on the press would hand the release to the window below, and a fullscreen video in
/// Firefox leaves fullscreen on a lone Escape release.
pub fn nav_event(event: &keyboard::Event) -> Option<Nav> {
    let escape = keyboard::Key::Named(Named::Escape);
    match event {
        keyboard::Event::KeyPressed { key, modifiers, .. } if *key != escape => nav(key, *modifiers),
        keyboard::Event::KeyReleased { key, modifiers, .. } if *key == escape => nav(key, *modifiers),
        _ => None,
    }
}

pub fn nav(key: &keyboard::Key, modifiers: Modifiers) -> Option<Nav> {
    match key {
        keyboard::Key::Named(Named::Escape) if modifiers.shift() => Some(Nav::Back),
        keyboard::Key::Named(Named::Escape) => Some(Nav::Close),
        keyboard::Key::Named(Named::Enter) => Some(Nav::Activate { secondary: modifiers.control() }),
        keyboard::Key::Named(Named::ArrowDown) => Some(Nav::Move(1)),
        keyboard::Key::Named(Named::ArrowUp) => Some(Nav::Move(-1)),
        keyboard::Key::Named(Named::ArrowLeft) if modifiers.alt() => Some(Nav::Accent(-1)),
        keyboard::Key::Named(Named::ArrowRight) if modifiers.alt() => Some(Nav::Accent(1)),
        keyboard::Key::Named(Named::ArrowLeft) if modifiers.is_empty() => Some(Nav::Sideways(-1)),
        keyboard::Key::Named(Named::ArrowRight) if modifiers.is_empty() => Some(Nav::Sideways(1)),
        keyboard::Key::Named(Named::Tab) => Some(Nav::Move(if modifiers.shift() { -1 } else { 1 })),
        keyboard::Key::Character(c) if modifiers.control() => match c.as_str() {
            "j" | "n" => Some(Nav::Move(1)),
            "k" | "p" => Some(Nav::Move(-1)),
            "r" => Some(Nav::Random),
            digit => match digit.parse::<usize>() {
                Ok(n @ 1..=9) => Some(Nav::Pick(n - 1)),
                _ => None,
            },
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(key: Named) -> keyboard::Key {
        keyboard::Key::Named(key)
    }

    fn character(c: &str) -> keyboard::Key {
        keyboard::Key::Character(c.into())
    }

    #[test]
    fn escape_navigates_on_release_and_other_keys_on_press() {
        let event = |pressed: bool, key: keyboard::Key, modifiers: Modifiers| {
            let (physical_key, location) = (keyboard::key::Physical::Unidentified(keyboard::key::NativeCode::Unidentified), keyboard::Location::Standard);
            match pressed {
                true => keyboard::Event::KeyPressed { key: key.clone(), modified_key: key, physical_key, location, modifiers, text: None, repeat: false },
                false => keyboard::Event::KeyReleased { key: key.clone(), modified_key: key, physical_key, location, modifiers },
            }
        };
        let none = Modifiers::empty();
        assert_eq!(nav_event(&event(true, named(Named::Escape), none)), None);
        assert_eq!(nav_event(&event(false, named(Named::Escape), none)), Some(Nav::Close));
        assert_eq!(nav_event(&event(false, named(Named::Escape), Modifiers::SHIFT)), Some(Nav::Back));
        assert_eq!(nav_event(&event(true, named(Named::ArrowDown), none)), Some(Nav::Move(1)));
        assert_eq!(nav_event(&event(false, named(Named::ArrowDown), none)), None);
        assert_eq!(nav_event(&event(true, named(Named::Enter), Modifiers::CTRL)), Some(Nav::Activate { secondary: true }));
    }

    #[test]
    fn keys_map_to_navigation() {
        let none = Modifiers::empty();
        assert_eq!(nav(&named(Named::Escape), none), Some(Nav::Close));
        assert_eq!(nav(&named(Named::Escape), Modifiers::SHIFT), Some(Nav::Back));
        assert_eq!(nav(&named(Named::Enter), none), Some(Nav::Activate { secondary: false }));
        assert_eq!(nav(&named(Named::Enter), Modifiers::CTRL), Some(Nav::Activate { secondary: true }));
        assert_eq!(nav(&named(Named::ArrowDown), none), Some(Nav::Move(1)));
        assert_eq!(nav(&named(Named::ArrowUp), none), Some(Nav::Move(-1)));
        assert_eq!(nav(&named(Named::Tab), none), Some(Nav::Move(1)));
        assert_eq!(nav(&named(Named::Tab), Modifiers::SHIFT), Some(Nav::Move(-1)));
        for (c, delta) in [("j", 1), ("n", 1), ("k", -1), ("p", -1)] {
            assert_eq!(nav(&character(c), Modifiers::CTRL), Some(Nav::Move(delta)), "Ctrl+{c}");
        }
        assert_eq!(nav(&character("1"), Modifiers::CTRL), Some(Nav::Pick(0)));
        assert_eq!(nav(&character("9"), Modifiers::CTRL), Some(Nav::Pick(8)));
    }

    #[test]
    fn typing_is_left_to_the_search_field() {
        assert_eq!(nav(&character("j"), Modifiers::empty()), None);
        assert_eq!(nav(&character("1"), Modifiers::empty()), None);
        assert_eq!(nav(&character("0"), Modifiers::CTRL), None);
        assert_eq!(nav(&named(Named::Backspace), Modifiers::empty()), None);
    }

    #[test]
    fn shift_escape_clears_the_search_then_goes_back_then_closes() {
        let mut launcher = Launcher::new();
        launcher.open(Page::Calculator);
        launcher.set_query(String::from("2+2"));
        assert!(launcher.back());
        assert_eq!((launcher.query.as_str(), launcher.page()), ("", Page::Calculator));
        assert!(launcher.back());
        assert_eq!(launcher.page(), Page::Home);
        assert!(!launcher.back());
    }

    #[test]
    fn opening_a_page_starts_with_an_empty_search() {
        let mut launcher = Launcher::new();
        launcher.set_query(String::from("calc"));
        launcher.move_selection(1, 4);
        launcher.open(Page::Calculator);
        assert_eq!((launcher.query.as_str(), launcher.selected, launcher.page()), ("", 0, Page::Calculator));
    }

    #[test]
    fn typing_resets_the_selection() {
        let mut launcher = Launcher::new();
        launcher.move_selection(2, 10);
        launcher.set_query(String::from("f"));
        assert_eq!((launcher.selected, launcher.first), (0, 0));
    }

    #[test]
    fn selection_wraps_and_stays_visible() {
        let mut launcher = Launcher::new();
        launcher.move_selection(-1, 10);
        assert_eq!((launcher.selected, launcher.first), (9, 3));
        launcher.move_selection(1, 10);
        assert_eq!((launcher.selected, launcher.first), (0, 0));
        launcher.move_selection(7, 10);
        assert_eq!((launcher.selected, launcher.first), (7, 1));
        launcher.move_selection(1, 0);
        assert_eq!((launcher.selected, launcher.first), (0, 0));
    }

    fn titles(launcher: &Launcher) -> Vec<String> {
        items(launcher).into_iter().map(|item| item.title).collect()
    }

    fn app(id: &str, name: &str, description: &str) -> App {
        apps::parse(id, &format!("[Desktop Entry]\nType=Application\nName={name}\nComment={description}\nExec={}\n", id.trim_end_matches(".desktop"))).unwrap()
    }

    fn with_apps() -> Launcher {
        let mut launcher = Launcher::new();
        launcher.apps = vec![
            app("firefox.desktop", "Firefox", "Browse the Web"),
            app("kitty.desktop", "Kitty", "Fast, GPU-based terminal"),
            app("org.gnome.Nautilus.desktop", "Files", "Access and organize files"),
        ];
        launcher.now = 1_000_000;
        launcher
    }

    fn with_wallpapers(count: usize) -> Launcher {
        let mut launcher = Launcher::new();
        launcher.open(Page::Wallpapers);
        launcher.gallery.entries = (0..count)
            .map(|n| gallery::Entry { path: format!("/w/{n:02}.png").into(), name: format!("{n:02}.png"), size: None, modified: 0, len: 0 })
            .collect();
        launcher
    }

    #[test]
    fn the_wallpapers_page_is_a_larger_surface() {
        assert_eq!(size(Page::Wallpapers), (1040, 640));
        assert_eq!(size(Page::Wallhaven), (1040, 640));
        assert_eq!(size(Page::Home), (WIDTH, HEIGHT));
        assert_eq!(size(Page::Files), (WIDTH, HEIGHT));
        assert_eq!(size(Page::Emojis), (720, 560));
    }

    #[test]
    fn the_emojis_page_lays_out_on_entering_and_follows_the_selection() {
        let mut launcher = Launcher::new();
        launcher.emoji.counts.insert(String::from("🍌"), 3);
        launcher.open(Page::Emojis);
        assert_eq!(launcher.emoji.lines[0], emoji::Line::Header("MOST USED"));
        assert_eq!(launcher.emoji.text(0), Some("🍌"));
        // Down from MOST USED crosses the SMILEYS header, in the same column.
        launcher.emoji_vertical(1);
        assert_eq!(launcher.selected, 1);
        launcher.emoji_sideways(2);
        assert_eq!(launcher.selected, 3);
        for _ in 0..10 {
            launcher.emoji_vertical(1);
        }
        assert_eq!(launcher.selected, 63);
        assert!(launcher.emoji.first_line > 0);
        let line = emoji::line_of(&launcher.emoji.lines, launcher.selected).unwrap();
        let shown: f32 = launcher.emoji.lines[launcher.emoji.first_line..=line].iter().map(emoji::Line::height).sum();
        assert!(shown <= EMOJI_GRID_HEIGHT);
        // A search flattens the grid and starts over at its top.
        launcher.set_query(String::from("banana"));
        assert_eq!((launcher.selected, launcher.emoji.first_line), (0, 0));
        assert_eq!(launcher.emoji.text(0), Some("🍌"));
        launcher.emoji.tone = 3;
        launcher.set_query(String::from("thumbs up"));
        assert_eq!(launcher.emoji.text(0), Some("👍🏽"));
    }

    #[test]
    fn grid_keys_are_arrows_alt_arrows_and_ctrl_r() {
        let named = |key| keyboard::Key::Named(key);
        assert_eq!(nav(&named(Named::ArrowLeft), Modifiers::empty()), Some(Nav::Sideways(-1)));
        assert_eq!(nav(&named(Named::ArrowRight), Modifiers::empty()), Some(Nav::Sideways(1)));
        assert_eq!(nav(&named(Named::ArrowRight), Modifiers::ALT), Some(Nav::Accent(1)));
        assert_eq!(nav(&named(Named::ArrowLeft), Modifiers::ALT), Some(Nav::Accent(-1)));
        // Shift+arrows still select text in the search.
        assert_eq!(nav(&named(Named::ArrowLeft), Modifiers::SHIFT), None);
        assert_eq!(nav(&keyboard::Key::Character("r".into()), Modifiers::CTRL), Some(Nav::Random));
    }

    #[test]
    fn the_grid_selection_scrolls_by_rows_and_resets_the_accent() {
        let mut launcher = with_wallpapers(20);
        launcher.gallery.accent = 2;
        launcher.gallery_move(gallery::COLUMNS as i32 * 3);
        assert_eq!((launcher.selected, launcher.gallery.first_row, launcher.gallery.accent), (12, 1, 0));
        assert_eq!(launcher.gallery_selected().unwrap().name, "12.png");
        launcher.gallery_move(1);
        assert_eq!(launcher.selected, 13);
        launcher.gallery_scroll(5);
        assert_eq!(launcher.gallery.first_row, 2);
        // The search narrows the tiles and starts over.
        launcher.set_query(String::from("1"));
        assert_eq!(launcher.gallery_shown().len(), 11);
        assert_eq!((launcher.selected, launcher.gallery.first_row), (0, 0));
        assert!(items(&launcher).is_empty());
    }

    #[test]
    fn home_lists_apps_then_pages_then_commands() {
        assert_eq!(
            titles(&with_apps()),
            ["Firefox", "Kitty", "Files", "Files", "Clipboard", "Keybinds", "Calculator", "Wallpapers", "Wallhaven", "Emojis", "Lock", "Suspend", "Logout", "Restart", "Shutdown"]
        );
        let items = items(&with_apps());
        assert_eq!((items[0].tag.as_str(), &items[0].action), ("APPLICATION", &Action::App(0)));
        assert_eq!((items[3].tag.as_str(), &items[3].action), ("PAGE", &Action::Open(Page::Files)));
        assert_eq!((items[14].tag.as_str(), &items[14].action), ("COMMAND", &Action::Power(Power::Shutdown)));
    }

    #[test]
    fn launched_apps_come_first_when_the_search_is_empty() {
        let mut launcher = with_apps();
        apps::bump(&mut launcher.scores, "kitty.desktop", launcher.now);
        assert_eq!(titles(&launcher)[..3], ["Kitty", "Firefox", "Files"]);
    }

    #[test]
    fn the_search_matches_names_ids_and_descriptions_fuzzily() {
        let mut launcher = with_apps();
        launcher.set_query(String::from("ffx"));
        assert_eq!(titles(&launcher), ["Firefox"]);
        launcher.set_query(String::from("nautilus"));
        assert_eq!(titles(&launcher), ["Files"]);
        launcher.set_query(String::from("gpu term"));
        assert_eq!(titles(&launcher), ["Kitty"]);
        launcher.set_query(String::from("calc"));
        assert_eq!(titles(&launcher), ["Calculator"]);
        launcher.set_query(String::from("HISTORY"));
        assert_eq!(titles(&launcher), ["Clipboard"]);
    }

    #[test]
    fn launch_scores_break_ties_between_matches() {
        let mut launcher = with_apps();
        launcher.apps.push(app("firefox-dev.desktop", "Firefox", "Developer Edition"));
        apps::bump(&mut launcher.scores, "firefox-dev.desktop", launcher.now);
        launcher.set_query(String::from("firefox"));
        let items = items(&launcher);
        assert_eq!(items[0].action, Action::App(3));
    }

    #[test]
    fn apps_with_an_open_window_are_marked() {
        let mut launcher = with_apps();
        launcher.running = vec![String::from("kitty")];
        let running: Vec<bool> = items(&launcher).iter().take(3).map(|item| item.running).collect();
        assert_eq!(running, [false, true, false]);
    }

    #[test]
    fn session_commands_ask_for_a_second_enter() {
        let mut launcher = with_apps();
        assert!(launcher.confirm(Power::Lock));
        assert!(!launcher.confirm(Power::Shutdown));
        let shutdown = items(&launcher).into_iter().find(|item| item.action == Action::Power(Power::Shutdown)).unwrap();
        assert_eq!(shutdown.tag, "CONFIRM ↵");
        assert!(launcher.confirm(Power::Shutdown));
        // Moving away cancels it.
        assert!(!launcher.confirm(Power::Restart));
        launcher.move_selection(1, 12);
        assert!(!launcher.confirm(Power::Restart));
        launcher.set_query(String::from("r"));
        assert!(!launcher.confirm(Power::Restart));
    }

    #[test]
    fn the_keybinds_page_matches_keys_and_descriptions() {
        let mut launcher = Launcher::new();
        launcher.keybinds = vec![
            Keybind { keys: String::from("SUPER + RETURN"), description: String::from("Launch terminal") },
            Keybind { keys: String::from("SUPER + SPACE"), description: String::from("Toggle launcher") },
            Keybind { keys: String::from("SUPER + V"), description: String::from("Open clipboard manager") },
        ];
        launcher.open(Page::Keybinds);
        assert_eq!(section(&launcher, 3), "KEYBINDS");
        assert_eq!(titles(&launcher), ["SUPER + RETURN", "SUPER + SPACE", "SUPER + V"]);
        let first = &items(&launcher)[0];
        assert_eq!((first.tag.as_str(), &first.action, first.title_style), ("Launch terminal", &Action::None, TitleStyle::Keys));
        launcher.set_query(String::from("clipboard"));
        assert_eq!(titles(&launcher), ["SUPER + V"]);
        launcher.set_query(String::from("super space"));
        assert_eq!(titles(&launcher), ["SUPER + SPACE"]);
        assert_eq!(section(&launcher, 1), "RESULTS (1)");
    }

    fn calculator() -> Launcher {
        let mut launcher = Launcher::new();
        launcher.history = vec![
            calc::Entry { input: String::from("4 * 22 + 1"), result: String::from("89") },
            calc::Entry { input: String::from("500ns to ms"), result: String::from("0.0005 ms") },
        ];
        launcher.open(Page::Calculator);
        launcher
    }

    fn answer(result: &str) -> calc::Answer {
        calc::Answer { expression: String::from("900 days"), sign: "=", result: result.to_string() }
    }

    #[test]
    fn the_calculator_lists_past_calculations_matching_the_search() {
        let mut launcher = calculator();
        assert_eq!((section(&launcher, 2).as_str(), titles(&launcher)), ("HISTORY", vec![String::from("4 * 22 + 1"), String::from("500ns to ms")]));
        let first = &items(&launcher)[0];
        assert_eq!((first.tag.as_str(), first.title_style, &first.action), ("89", TitleStyle::Mono, &Action::Recall(0)));
        launcher.set_query(String::from("ms"));
        assert_eq!(titles(&launcher), ["500ns to ms"]);
        launcher.set_query(String::from("900 day to ns"));
        assert!(items(&launcher).is_empty());
        assert_eq!(empty_text(Page::Calculator), "NO MATCHING HISTORY");
    }

    #[test]
    fn the_answer_belongs_to_the_search_it_was_computed_for() {
        let mut launcher = calculator();
        launcher.set_query(String::from("900 day to ns"));
        launcher.set_answer(String::from("900 day to n"), Some(answer("7.776E13 ns")));
        assert_eq!(launcher.answer(), None);
        launcher.set_answer(String::from("900 day to ns"), Some(answer("7.776E16 ns")));
        assert_eq!(launcher.answer(), Some(&answer("7.776E16 ns")));
        // qalc runs per keystroke, so an older search can answer last.
        launcher.set_answer(String::from("900 day to n"), Some(answer("7.776E13 ns")));
        assert_eq!(launcher.answer(), Some(&answer("7.776E16 ns")));
        // Fewer rows fit under the result card.
        assert!(launcher.visible_rows() < VISIBLE_ROWS);
        launcher.set_query(String::from("900 day to s"));
        assert_eq!(launcher.answer(), None);
    }

    #[test]
    fn enter_copies_the_answer_unless_a_past_calculation_was_picked() {
        let mut launcher = calculator();
        assert_eq!(calculator_enter(&launcher), CalcEnter::Recall(String::from("4 * 22 + 1")));
        launcher.set_query(String::from("22"));
        launcher.set_answer(String::from("22"), Some(answer("22")));
        assert_eq!(calculator_enter(&launcher), CalcEnter::Copy(answer("22")));
        launcher.move_selection(1, 1);
        assert_eq!(calculator_enter(&launcher), CalcEnter::Recall(String::from("4 * 22 + 1")));
        launcher.set_query(String::from("zzz"));
        assert_eq!(calculator_enter(&launcher), CalcEnter::Nothing);
    }

    fn files() -> Launcher {
        let mut launcher = Launcher::new();
        launcher.home = String::from("/home/user");
        launcher.recent = vec![files::Entry::new("/home/user/code/zehll", true, 90.0), files::Entry::new("/home/user/Downloads", true, 40.0)];
        launcher.open(Page::Files);
        launcher
    }

    #[test]
    fn the_files_page_lists_recent_folders_then_search_results() {
        let mut launcher = files();
        assert_eq!((section(&launcher, 2).as_str(), titles(&launcher)), ("RECENT FOLDERS (ZOXIDE)", vec![String::from("zehll"), String::from("Downloads")]));
        let first = &items(&launcher)[0];
        assert_eq!((first.title_style, &first.action), (TitleStyle::Body, &Action::File(launcher.recent[0].clone())));
        assert!(matches!(first.icon, ItemIcon::Symbol("folder")));
        assert_eq!(selected_file(&launcher), Some(launcher.recent[0].clone()));
        launcher.set_query(String::from("notes"));
        launcher.set_found(String::from("notes"), vec![files::Entry::new("/home/user/notes.md", false, 0.0)]);
        assert_eq!(titles(&launcher), ["notes.md"]);
        assert!(matches!(items(&launcher)[0].icon, ItemIcon::Symbol("description")));
        assert_eq!(section(&launcher, 1), "RESULTS (1)");
        launcher.set_query(String::from("zzzz"));
        assert!(items(&launcher).is_empty());
        assert_eq!(empty_text(Page::Files), "NO RESULTS");
        assert_eq!(hints(Page::Files), [("↵", "open"), ("^↵", "path"), ("⇧ESC", "back")]);
        assert!(has_preview(Page::Files) && !has_preview(Page::Home));
    }

    #[test]
    fn search_results_for_an_older_search_are_dropped() {
        let mut launcher = files();
        launcher.set_query(String::from("notes"));
        launcher.set_found(String::from("note"), vec![files::Entry::new("/home/user/notes.txt", false, 0.0)]);
        assert_eq!(titles(&launcher), Vec::<String>::new());
        launcher.set_found(String::from("notes"), vec![files::Entry::new("/home/user/notes.md", false, 0.0)]);
        // fd runs per search, so an older one can finish last.
        launcher.set_found(String::from("note"), vec![files::Entry::new("/home/user/notes.txt", false, 0.0)]);
        assert_eq!(titles(&launcher), ["notes.md"]);
    }

    #[test]
    fn file_details_are_kept_by_path() {
        let mut launcher = files();
        let details = files::Details { mime: String::from("inode/directory"), size: 4096, items: Some(21), preview: None };
        launcher.set_details(String::from("/home/user/code/zehll"), details.clone());
        assert_eq!(launcher.details("/home/user/code/zehll"), Some(&details));
        assert_eq!(launcher.details("/home/user/Downloads"), None);
        assert_eq!(
            preview_rows(&launcher, &launcher.recent[0]),
            [("NAME", "zehll"), ("TYPE", "inode/directory"), ("SIZE", "4.0 KB"), ("PATH", "~/code/zehll"), ("ITEMS", "21")].map(|(k, v)| (k, v.to_string()))
        );
        assert_eq!(preview_rows(&launcher, &launcher.recent[1])[1], ("TYPE", String::from("…")));
    }

    fn clip(id: u64, preview: &str) -> clipboard::Entry {
        clipboard::parse_line(&format!("{id}\t{preview}")).unwrap()
    }

    fn clipboard_page() -> Launcher {
        let mut launcher = Launcher::new();
        launcher.open(Page::Clipboard);
        let request = launcher.begin_clips();
        launcher.set_clips(request, vec![clip(30, "git push origin"), clip(29, "[[ binary data 4 KiB png 412x150 ]]"), clip(28, "token abc")]);
        launcher
    }

    #[test]
    fn the_clipboard_page_lists_the_history_matching_the_search() {
        let mut launcher = clipboard_page();
        assert_eq!(titles(&launcher), ["git push origin", "[[ binary data 4 KiB png 412x150 ]]", "token abc"]);
        assert_eq!(section(&launcher, items(&launcher).len()), "HISTORY (3)");
        let first = &items(&launcher)[0];
        assert_eq!((first.title_style, &first.action), (TitleStyle::Mono, &Action::Clip(clip(30, "git push origin"))));
        assert!(matches!(items(&launcher)[1].icon, ItemIcon::Symbol("image")));
        launcher.set_query(String::from("PUSH git"));
        assert_eq!(titles(&launcher), ["git push origin"]);
        assert_eq!(section(&launcher, 1), "RESULTS (1)");
        assert_eq!(hints(Page::Clipboard), [("↵", "copy"), ("^↵", "pin"), ("⇧ESC", "back")]);
        assert!(has_preview(Page::Clipboard));
    }

    #[test]
    fn the_clipboard_preview_shows_type_mime_date_and_hash() {
        let mut launcher = clipboard_page();
        launcher.move_selection(1, 3);
        let image = selected_clip(&launcher).unwrap();
        assert_eq!(
            clip_rows(&launcher, &image),
            [("TYPE", "Image 412x150, 4 KiB"), ("MIME", "image/png"), ("DATE", "—"), ("HASH", "…")].map(|(k, v)| (k, v.to_string()))
        );
        let text = clip(28, "token abc");
        launcher.set_decoded(28, clipboard::Decoded::new(&text, b"token abc"));
        assert_eq!(clip_rows(&launcher, &text)[3], ("HASH", clipboard::content_hash(b"token abc")));
    }

    #[test]
    fn pinned_entries_come_first_and_stay_pinned_under_a_new_id() {
        let mut launcher = clipboard_page();
        launcher.toggle_pin(&clip(28, "token abc"));
        assert_eq!(titles(&launcher)[0], "token abc");
        // The selection follows the pinned entry.
        assert_eq!(selected_clip(&launcher), Some(clip(28, "token abc")));
        assert!(matches!(items(&launcher)[0].icon, ItemIcon::Symbol("push_pin")));
        // Copying it back gives it a new id.
        let request = launcher.begin_clips();
        launcher.set_clips(request, vec![clip(31, "token abc"), clip(30, "git push origin")]);
        assert_eq!(titles(&launcher), ["token abc", "git push origin"]);
        assert!(launcher.is_pinned(&clip(31, "token abc")));
    }

    #[test]
    fn the_pin_button_shows_only_pinned_entries() {
        let mut launcher = clipboard_page();
        launcher.toggle_pin(&clip(30, "git push origin"));
        launcher.toggle_pinned_only();
        assert_eq!(titles(&launcher), ["git push origin"]);
        assert_eq!(section(&launcher, 1), "PINNED (1)");
        launcher.toggle_pinned_only();
        assert_eq!(titles(&launcher).len(), 3);
    }

    #[test]
    fn clearing_the_history_needs_a_second_press() {
        let mut launcher = clipboard_page();
        assert!(!launcher.confirm_clear());
        assert!(launcher.clear_armed());
        assert!(launcher.confirm_clear());
        assert!(!launcher.clear_armed());
        // Moving or typing disarms it.
        assert!(!launcher.confirm_clear());
        launcher.move_selection(1, 3);
        assert!(!launcher.clear_armed());
        assert!(!launcher.confirm_clear());
        launcher.set_query(String::from("g"));
        assert!(!launcher.confirm_clear());
        launcher.toggle_pinned_only();
        assert!(!launcher.clear_armed());
    }

    #[test]
    fn an_older_history_read_arriving_late_is_dropped() {
        let mut launcher = clipboard_page();
        let older = launcher.begin_clips();
        let newer = launcher.begin_clips();
        launcher.set_clips(newer, vec![clip(40, "fresh")]);
        launcher.set_clips(older, vec![clip(30, "git push origin")]);
        assert_eq!(titles(&launcher), ["fresh"]);
    }

    #[test]
    fn deleting_the_last_entry_keeps_the_selection_on_the_list() {
        let mut launcher = clipboard_page();
        launcher.move_selection(2, 3);
        launcher.remove_clip(28);
        assert_eq!((titles(&launcher).len(), launcher.selected), (2, 1));
        launcher.clear_clips();
        assert_eq!((items(&launcher).len(), launcher.selected), (0, 0));
    }

    #[test]
    fn scrolling_stops_at_the_ends() {
        let mut launcher = Launcher::new();
        launcher.scroll(-2, 10);
        assert_eq!(launcher.first, 0);
        launcher.scroll(10, 10);
        assert_eq!(launcher.first, 3);
        launcher.scroll(1, 4);
        assert_eq!(launcher.first, 0);
    }
}
