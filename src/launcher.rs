//! App launcher: a centered search box with pages, toggled with `zehll:launcherToggle`.

use iced::keyboard::{self, Modifiers, key::Named};
use iced::widget::{self, Column, Row, Space, Stack, button, column, container, mouse_area, row, text, text_input};
use iced::{Alignment, Border, Color, Element, Length, Padding, mouse};

use crate::{BODY_BOLD, BODY_FONT, Bar, CONDENSED_BOLD, CONDENSED_HEAVY, ICON_FONT, Message, glow_style, line, panel_style, shape, slanted};

/// Rows shown at once; the list scrolls past them.
pub const VISIBLE_ROWS: usize = 7;
const SEARCH_HEIGHT: f32 = 54.0;
const ROW_HEIGHT: f32 = 46.0;
const SECTION_HEIGHT: f32 = 34.0;
const LIST_PADDING: f32 = 8.0;
const FOOTER_HEIGHT: f32 = 34.0;
pub const WIDTH: u32 = 720;
// Search bar, list and footer, plus the two dividers.
pub const HEIGHT: u32 = (SEARCH_HEIGHT + SECTION_HEIGHT + ROW_HEIGHT * VISIBLE_ROWS as f32 + LIST_PADDING + FOOTER_HEIGHT + 2.0) as u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Home,
    Files,
    Clipboard,
    Keybinds,
    Calculator,
}

impl Page {
    /// Pages opened from Home, in the order Home lists them.
    const CHILDREN: [Page; 4] = [Page::Files, Page::Clipboard, Page::Keybinds, Page::Calculator];

    /// Label of the search bar chip.
    pub fn chip(self) -> &'static str {
        match self {
            Page::Home => "HOME",
            Page::Files => "FILES",
            Page::Clipboard => "CLIP",
            Page::Keybinds => "KEYS",
            Page::Calculator => "CALC",
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
        }
    }

    pub fn placeholder(self) -> &'static str {
        match self {
            Page::Home => "Search apps, pages and commands",
            Page::Files => "Search files and folders",
            Page::Clipboard => "Filter clipboard entries",
            Page::Keybinds => "Search keybindings",
            Page::Calculator => "Do math, convert units, or search past calculations",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Page::Home => "Home",
            Page::Files => "Files",
            Page::Clipboard => "Clipboard",
            Page::Keybinds => "Keybinds",
            Page::Calculator => "Calculator",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Page::Home => "",
            Page::Files => "Recent folders and file search",
            Page::Clipboard => "Clipboard history",
            Page::Keybinds => "Hyprland keybindings",
            Page::Calculator => "Math, units and currency",
        }
    }
}

/// What a launcher row does when activated.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Open(Page),
}

/// One row of the launcher list.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub icon: &'static str,
    pub title: String,
    pub description: String,
    pub tag: &'static str,
    pub action: Action,
}

/// Rows of the current page. Home lists the pages whose name or description contains the search.
pub fn items(launcher: &Launcher) -> Vec<Item> {
    match launcher.page() {
        Page::Home => {
            let query = launcher.query.to_lowercase();
            Page::CHILDREN
                .into_iter()
                .filter(|page| page.title().to_lowercase().contains(&query) || page.description().to_lowercase().contains(&query))
                .map(|page| Item {
                    icon: page.icon(),
                    title: page.title().to_string(),
                    description: page.description().to_string(),
                    tag: "PAGE",
                    action: Action::Open(page),
                })
                .collect()
        }
        Page::Files | Page::Clipboard | Page::Keybinds | Page::Calculator => Vec::new(),
    }
}

/// Header above the list.
pub fn section(launcher: &Launcher, count: usize) -> String {
    match (launcher.page(), launcher.query.is_empty()) {
        (Page::Home, true) => String::from("SUGGESTIONS"),
        _ => format!("RESULTS ({count})"),
    }
}

/// Key hints of the footer: key and what it does.
pub fn hints(page: Page) -> &'static [(&'static str, &'static str)] {
    match page {
        Page::Home => &[("↵", "open"), ("^↵", "action"), ("^1–9", "pick"), ("ESC", "close")],
        Page::Files | Page::Clipboard | Page::Keybinds | Page::Calculator => &[("↵", "open"), ("⇧ESC", "back"), ("ESC", "close")],
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
    let input = text_input(page.placeholder(), &launcher.query)
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
    let search = row![slanted(chip, 14.0, theme.accent), container(input).width(Length::Fill).center_y(Length::Fill)]
        .spacing(14)
        .padding(Padding::ZERO.right(18.0))
        .height(SEARCH_HEIGHT);

    let header = container(text(section(launcher, len)).font(CONDENSED_BOLD).size(12).color(theme.text_muted))
        .padding(Padding { top: 12.0, right: 18.0, bottom: 6.0, left: 18.0 });
    let list: Element<'_, Message> = match items.is_empty() {
        true => container(text("NO RESULTS").font(CONDENSED_BOLD).size(14).color(theme.text_muted)).center(Length::Fill).into(),
        false => {
            let rows = items.into_iter().enumerate().skip(launcher.first).take(VISIBLE_ROWS);
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
    let body = column![header, list].height(Length::Fill);

    let hints = Row::with_children(hints(page).iter().map(|(key, label)| {
        let key = container(text(*key).font(CONDENSED_BOLD).size(12).color(theme.text)).padding([1, 6]).style(move |_| container::Style {
            border: Border { color: theme.border, width: 1.0, radius: 0.0.into() },
            ..Default::default()
        });
        row![key, text(*label).font(BODY_FONT).size(12).color(theme.text_muted)].spacing(6).align_y(Alignment::Center).into()
    }))
    .spacing(18);
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

fn item_row<'a>(theme: crate::theme::Theme, item: Item, index: usize, selected: bool) -> Element<'a, Message> {
    let icon_color = if selected { theme.glow } else { theme.text };
    let content = row![
        container(icon_colored(item.icon, 18.0, icon_color)).center_x(26),
        text(item.title).font(BODY_BOLD).size(15).wrapping(text::Wrapping::None),
        container(text(item.description).font(BODY_FONT).size(12).color(theme.text_muted).wrapping(text::Wrapping::None))
            .width(Length::Fill)
            .clip(true),
        text(item.tag).font(CONDENSED_BOLD).size(12).color(theme.text_muted),
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
}

pub struct Launcher {
    pub query: String,
    // Pages entered from Home, the current one last.
    stack: Vec<Page>,
    pub selected: usize,
    // First row shown; follows the selection.
    pub first: usize,
}

impl Launcher {
    pub fn new() -> Self {
        Self { query: String::new(), stack: Vec::new(), selected: 0, first: 0 }
    }

    pub fn page(&self) -> Page {
        self.stack.last().copied().unwrap_or(Page::Home)
    }

    pub fn open(&mut self, page: Page) {
        self.stack.push(page);
        self.set_query(String::new());
    }

    pub fn set_query(&mut self, query: String) {
        self.query = query;
        self.selected = 0;
        self.first = 0;
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
        if len == 0 {
            self.selected = 0;
            self.first = 0;
            return;
        }
        self.selected = (self.selected as i64 + delta as i64).rem_euclid(len as i64) as usize;
        if self.selected < self.first {
            self.first = self.selected;
        } else if self.selected >= self.first + VISIBLE_ROWS {
            self.first = self.selected + 1 - VISIBLE_ROWS;
        }
    }

    /// Scrolls by whole rows without moving past the list.
    pub fn scroll(&mut self, rows: i32, len: usize) {
        let last = len.saturating_sub(VISIBLE_ROWS) as i64;
        self.first = (self.first as i64 + rows as i64).clamp(0, last) as usize;
    }
}

/// Maps a key press to navigation; typing is left to the search field.
pub fn nav(key: &keyboard::Key, modifiers: Modifiers) -> Option<Nav> {
    match key {
        keyboard::Key::Named(Named::Escape) if modifiers.shift() => Some(Nav::Back),
        keyboard::Key::Named(Named::Escape) => Some(Nav::Close),
        keyboard::Key::Named(Named::Enter) => Some(Nav::Activate { secondary: modifiers.control() }),
        keyboard::Key::Named(Named::ArrowDown) => Some(Nav::Move(1)),
        keyboard::Key::Named(Named::ArrowUp) => Some(Nav::Move(-1)),
        keyboard::Key::Named(Named::Tab) => Some(Nav::Move(if modifiers.shift() { -1 } else { 1 })),
        keyboard::Key::Character(c) if modifiers.control() => match c.as_str() {
            "j" | "n" => Some(Nav::Move(1)),
            "k" | "p" => Some(Nav::Move(-1)),
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

    #[test]
    fn home_lists_the_pages_matching_the_search() {
        let mut launcher = Launcher::new();
        let titles = |launcher: &Launcher| items(launcher).into_iter().map(|item| item.title).collect::<Vec<_>>();
        assert_eq!(titles(&launcher), ["Files", "Clipboard", "Keybinds", "Calculator"]);
        launcher.set_query(String::from("CALC"));
        assert_eq!(titles(&launcher), ["Calculator"]);
        launcher.set_query(String::from("history"));
        assert_eq!(titles(&launcher), ["Clipboard"]);
        launcher.set_query(String::from("calc"));
        let calculator = &items(&launcher)[0];
        assert_eq!((calculator.tag, &calculator.action), ("PAGE", &Action::Open(Page::Calculator)));
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
