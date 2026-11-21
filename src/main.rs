mod hyprland;
mod menu;
mod shape;
mod system;
mod theme;
mod tray;

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Datelike, Local, NaiveDate};
use iced::font::{Style as FontStyle, Weight};
use iced::widget::{Column, Row, Space, Stack, button, canvas, column, container, image, mouse_area, row, svg, text};
use iced::window::Id;
use iced::{Alignment, Border, Color, Element, Font, Length, Padding, Shadow, Subscription, Task, time, window};
use iced_layershell::actions::IcedNewPopupSettings;
use iced_layershell::daemon;
use iced_layershell::reexport::{Anchor, KeyboardInteractivity, Layer, NewLayerShellSettings, OutputOption, PopupAnchor, PopupGravity};
use iced_layershell::settings::{LayerShellSettings, Settings, StartMode};
use iced_layershell::to_layer_message;
use system_tray::client::{ActivateRequest, Client, Event as TrayEvent, UpdateEvent};
use system_tray::menu::{MenuType, ToggleState, ToggleType};

const BAR_HEIGHT: u32 = 44;
const BAR_PADDING: u16 = 16;

const ICON_FONT: Font = Font::with_name("Material Symbols Rounded");
const BODY_FONT: Font = Font::with_name("Barlow");
const BODY_MEDIUM: Font = Font { weight: Weight::Medium, ..BODY_FONT };
const BODY_BOLD: Font = Font { weight: Weight::Semibold, ..BODY_FONT };
const CONDENSED: Font = Font { weight: Weight::Semibold, ..Font::with_name("Barlow Condensed") };
const CONDENSED_BOLD: Font = Font { weight: Weight::Bold, ..CONDENSED };
const CONDENSED_HEAVY: Font = Font { weight: Weight::ExtraBold, style: FontStyle::Italic, ..CONDENSED };

const TOOLTIP_WIDTH: u32 = 320;
const TOOLTIP_HEADER_HEIGHT: u32 = 70;
const TOOLTIP_ROW_HEIGHT: u32 = 38;
const TOOLTIP_STRIP_HEIGHT: u32 = 28;
const TOOLTIP_LINE_HEIGHT: u32 = 20;
const TOOLTIP_LIST_PADDING: (u16, u16) = (8, 14);

pub fn main() -> Result<(), iced_layershell::Error> {
    daemon(Bar::new, namespace, update, view)
        .style(style)
        .subscription(subscription)
        .settings(Settings {
            layer_settings: LayerShellSettings {
                size: Some((0, BAR_HEIGHT)),
                exclusive_zone: BAR_HEIGHT as i32,
                anchor: Anchor::Top | Anchor::Left | Anchor::Right,
                start_mode: StartMode::AllScreens,
                ..Default::default()
            },
            fonts: vec![
                Cow::Borrowed(include_bytes!("../assets/fonts/Barlow-Regular.ttf").as_slice()),
                Cow::Borrowed(include_bytes!("../assets/fonts/Barlow-Medium.ttf").as_slice()),
                Cow::Borrowed(include_bytes!("../assets/fonts/Barlow-SemiBold.ttf").as_slice()),
                Cow::Borrowed(include_bytes!("../assets/fonts/BarlowCondensed-SemiBold.ttf").as_slice()),
                Cow::Borrowed(include_bytes!("../assets/fonts/BarlowCondensed-Bold.ttf").as_slice()),
                Cow::Borrowed(include_bytes!("../assets/fonts/BarlowCondensed-ExtraBoldItalic.ttf").as_slice()),
            ],
            default_font: BODY_FONT,
            antialiasing: true,
            ..Default::default()
        })
        .run()
}

struct Bar {
    theme: theme::Theme,
    theme_modified: Option<SystemTime>,
    window: hyprland::ActiveWindow,
    now: DateTime<Local>,
    uptime: Duration,
    todos: system::Todos,
    tooltip: Option<Id>,
    // Still rendered as a tooltip while the compositor plays its close animation.
    closing_tooltip: Option<Id>,
    tray_client: Option<Arc<Client>>,
    // Keyed by D-Bus address, which keeps the icon order stable.
    tray_items: BTreeMap<String, tray::TrayItem>,
    tray_menu: Option<OpenMenu>,
    // Still rendered as a menu while the compositor plays its close animation.
    closing_tray_menu: Option<OpenMenu>,
}

struct OpenMenu {
    id: Id,
    address: String,
    // Ids of the submenus entered from the root.
    path: Vec<i32>,
}

impl Bar {
    fn new() -> (Self, Task<Message>) {
        let bar = Self {
            theme: theme::Theme::load(),
            theme_modified: theme::modified(),
            window: hyprland::active_window().unwrap_or_default(),
            now: Local::now(),
            uptime: Duration::ZERO,
            todos: system::Todos::default(),
            tooltip: None,
            closing_tooltip: None,
            tray_client: None,
            tray_items: BTreeMap::new(),
            tray_menu: None,
            closing_tray_menu: None,
        };
        (bar, Task::none())
    }

    fn menu_title(&self, address: &str) -> Option<String> {
        let title = self.tray_items.get(address)?.title.as_deref()?;
        (!title.is_empty()).then(|| title.to_uppercase())
    }
}

#[to_layer_message(multi)]
#[derive(Debug, Clone)]
enum Message {
    Tick,
    ActiveWindow(hyprland::ActiveWindow),
    ShowTooltip,
    HideTooltip,
    Tray(tray::TrayEvent),
    ActivateTray(String),
    SecondaryActivateTray(String),
    OpenTrayMenu { bar: Id, address: String },
    NavigateTrayMenu(Vec<i32>),
    ActivateTrayMenuItem(i32),
    WindowClosed(Id),
}

fn namespace() -> String {
    String::from("zehll")
}

fn subscription(_: &Bar) -> Subscription<Message> {
    Subscription::batch([
        time::every(Duration::from_secs(1)).map(|_| Message::Tick),
        Subscription::run(hyprland::active_window_events).map(Message::ActiveWindow),
        Subscription::run(tray::events).map(Message::Tray),
        window::close_events().map(Message::WindowClosed),
    ])
}

fn update(bar: &mut Bar, message: Message) -> Task<Message> {
    match message {
        Message::Tick => {
            bar.now = Local::now();
            let modified = theme::modified();
            if modified != bar.theme_modified {
                bar.theme_modified = modified;
                bar.theme = theme::Theme::load();
            }
            if bar.tooltip.is_some() {
                bar.uptime = system::uptime().unwrap_or_default();
            }
        }
        Message::ActiveWindow(window) => bar.window = window,
        Message::ShowTooltip => {
            if bar.tooltip.is_some() {
                return Task::none();
            }
            // A layer surface instead of an xdg_popup: layershellev grabs the pointer for every
            // popup, which sends a leave to the bar and makes the tooltip flicker.
            // Hyprland moves monitor focus with the cursor, so the focused monitor is the hovered bar.
            let Some(output) = hyprland::focused_monitor() else {
                return Task::none();
            };
            bar.uptime = system::uptime().unwrap_or_default();
            bar.todos = system::pending_todos();

            // The surface needs a fixed size, and every block of the tooltip has a fixed height.
            let (list_top, list_bottom) = TOOLTIP_LIST_PADDING;
            let list = bar.todos.lines.len() as u32 * TOOLTIP_LINE_HEIGHT + list_top as u32 + list_bottom as u32;
            let height = TOOLTIP_HEADER_HEIGHT + 2 + TOOLTIP_ROW_HEIGHT + TOOLTIP_STRIP_HEIGHT + list + 2;
            let settings = NewLayerShellSettings {
                size: Some((TOOLTIP_WIDTH, height)),
                layer: Layer::Overlay,
                // Anchored only to the top, so it is centered under the clock.
                anchor: Anchor::Top,
                exclusive_zone: Some(0),
                margin: Some((8, 0, 0, 0)),
                keyboard_interactivity: KeyboardInteractivity::None,
                output_option: OutputOption::OutputName(output),
                events_transparent: true,
                namespace: Some(String::from("zehll-tooltip")),
            };

            let id = Id::unique();
            bar.tooltip = Some(id);
            return Task::done(Message::NewLayerShell { settings, id });
        }
        Message::Tray(tray::TrayEvent::Ready(client)) => bar.tray_client = Some(client),
        Message::Tray(tray::TrayEvent::Event(event)) => match event {
            TrayEvent::Add(address, item) => {
                bar.tray_items.insert(address, tray::TrayItem::new(&item));
            }
            TrayEvent::Remove(address) => {
                bar.tray_items.remove(&address);
                if bar.tray_menu.as_ref().is_some_and(|open| open.address == address) {
                    return close_tray_menu(bar);
                }
            }
            TrayEvent::Update(address, update) => {
                if let Some(item) = bar.tray_items.get_mut(&address) {
                    match update {
                        UpdateEvent::Icon { icon_name, icon_pixmap } => {
                            item.set_icon(icon_name.as_deref(), icon_pixmap.as_deref())
                        }
                        UpdateEvent::Title(title) => item.title = title,
                        UpdateEvent::Menu(menu) => item.menu = Some(menu),
                        UpdateEvent::MenuDiff(diffs) => {
                            if let Some(menu) = &mut item.menu {
                                menu::apply_diffs(menu, &diffs);
                            }
                        }
                        _ => {}
                    }
                }
            }
        },
        Message::ActivateTray(address) => {
            return activate_tray(bar, ActivateRequest::Default { address, x: 0, y: 0 });
        }
        Message::SecondaryActivateTray(address) => {
            return activate_tray(bar, ActivateRequest::Secondary { address, x: 0, y: 0 });
        }
        Message::OpenTrayMenu { bar: parent, address } => {
            let close = close_tray_menu(bar);
            let has_header = bar.menu_title(&address).is_some();
            let Some(menu) = bar.tray_items.get(&address).and_then(|item| item.menu.as_ref()) else {
                return close;
            };
            let (width, height) = menu::size(&menu::entries(menu, &[]), false, has_header);
            // Hang the menu below the bar at the pointer; the compositor slides it back on-screen.
            // NewPopUp grabs the pointer, so clicking anywhere else dismisses the menu.
            // The cursor is queried only on click instead of tracking every pointer motion.
            let x = hyprland::cursor_x_on_focused_monitor().unwrap_or_default();
            let anchor = (x, 0, 1, BAR_HEIGHT as i32);
            let mut settings = IcedNewPopupSettings::new(parent, (width, height + menu::GAP as u32), anchor);
            settings.anchor = PopupAnchor::Bottom;
            settings.gravity = PopupGravity::Bottom;
            let id = Id::unique();
            bar.tray_menu = Some(OpenMenu { id, address, path: Vec::new() });
            return close.chain(Task::done(Message::NewPopUp { settings, id }));
        }
        Message::NavigateTrayMenu(path) => {
            let Some(open) = &bar.tray_menu else {
                return Task::none();
            };
            let has_header = bar.menu_title(&open.address).is_some();
            let Some(menu) = bar.tray_items.get(&open.address).and_then(|item| item.menu.as_ref()) else {
                return Task::none();
            };
            let (width, height) = menu::size(&menu::entries(menu, &path), !path.is_empty(), has_header);
            let Some(open) = &mut bar.tray_menu else {
                return Task::none();
            };
            open.path = path;
            return Task::done(Message::SizeChange { id: open.id, size: (width, height + menu::GAP as u32) });
        }
        Message::ActivateTrayMenuItem(submenu_id) => {
            let Some(open) = &bar.tray_menu else {
                return Task::none();
            };
            let Some(menu_path) = bar.tray_items.get(&open.address).and_then(|item| item.menu_path.clone()) else {
                return close_tray_menu(bar);
            };
            let request = ActivateRequest::MenuItem { address: open.address.clone(), menu_path, submenu_id };
            return activate_tray(bar, request).chain(close_tray_menu(bar));
        }
        Message::WindowClosed(id) => {
            if bar.tray_menu.as_ref().is_some_and(|open| open.id == id) {
                bar.tray_menu = None;
            }
            if bar.closing_tray_menu.as_ref().is_some_and(|open| open.id == id) {
                bar.closing_tray_menu = None;
            }
            if bar.closing_tooltip == Some(id) {
                bar.closing_tooltip = None;
            }
        }
        Message::HideTooltip => {
            if let Some(id) = bar.tooltip.take() {
                bar.closing_tooltip = Some(id);
                return Task::done(Message::RemoveWindow(id));
            }
        }
        _ => {}
    }
    Task::none()
}

fn close_tray_menu(bar: &mut Bar) -> Task<Message> {
    let Some(open) = bar.tray_menu.take() else {
        return Task::none();
    };
    let id = open.id;
    bar.closing_tray_menu = Some(open);
    Task::done(Message::RemoveWindow(id))
}

fn activate_tray(bar: &Bar, request: ActivateRequest) -> Task<Message> {
    let Some(client) = bar.tray_client.clone() else {
        return Task::none();
    };
    Task::future(async move {
        if let Err(err) = client.activate(request).await {
            eprintln!("zehll: tray activation failed: {err}");
        }
    })
    .discard()
}

fn view(bar: &Bar, id: Id) -> Element<'_, Message> {
    if bar.tooltip == Some(id) || bar.closing_tooltip == Some(id) {
        return clock_tooltip(bar);
    }
    if let Some(open) = [&bar.tray_menu, &bar.closing_tray_menu].into_iter().flatten().find(|open| open.id == id) {
        return tray_menu_view(bar, open);
    }

    let theme = bar.theme;
    let mut window = Row::new().spacing(14).height(Length::Fill).align_y(Alignment::Center);
    if !bar.window.class.is_empty() {
        let tag = container(text(bar.window.class.to_uppercase()).font(CONDENSED_HEAVY).size(17).color(theme.on_accent))
            .padding(Padding { top: 0.0, right: 30.0, bottom: 0.0, left: 18.0 })
            .center_y(Length::Fill);
        window = window.push(slanted(tag, 16.0, theme.accent));
    }
    window = window.push(text(&bar.window.title).font(BODY_MEDIUM).size(14).wrapping(text::Wrapping::None));

    let time = row![
        text(bar.now.format("%H:%M").to_string()).font(CONDENSED_BOLD).size(22),
        text(bar.now.format("%a %d/%m").to_string().to_uppercase()).font(CONDENSED_BOLD).size(15).color(theme.text_muted),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    let time = Stack::new()
        .push(container(time).padding(Padding::ZERO.bottom(5.0)))
        .push(shape(shape::Kind::Underline(2.0), theme.accent));
    // The hover area spans the full bar height so the tooltip is easy to trigger.
    let clock = mouse_area(container(time).padding([0, 18]).center_y(Length::Fill))
        .on_enter(Message::ShowTooltip)
        .on_exit(Message::HideTooltip);

    let content = container(
        row![
            container(window).width(Length::FillPortion(1)).clip(true),
            container(clock).center_x(Length::FillPortion(1)),
            container(tray_view(bar, id)).align_right(Length::FillPortion(1)),
        ]
        .height(Length::Fill)
        .align_y(Alignment::Center),
    )
    .padding(Padding::ZERO.right(f32::from(BAR_PADDING)))
    .height(Length::Fill);

    container(column![content, line(theme.bar_border)])
        .style(move |_| container::background(theme.bar_background))
        .into()
}

fn tray_view(bar: &Bar, bar_id: Id) -> Element<'_, Message> {
    let theme = bar.theme;
    let size = f32::from(tray::ICON_SIZE);
    let items = bar.tray_items.iter().filter_map(|(address, item)| {
        let icon: Element<'_, Message> = match item.icon.as_ref()? {
            tray::Icon::Raster(handle) => image(handle.clone()).width(size).height(size).into(),
            tray::Icon::Svg(handle) => svg(handle.clone()).width(size).height(size).into(),
        };
        let slot = Stack::new()
            .push(container(icon).center(30))
            .push_under(shape(shape::Kind::Brackets, theme.bracket));
        let slot = button(slot)
            .on_press(Message::ActivateTray(address.clone()))
            .padding(0)
            .style(move |_, status| glow_style(theme, status, 10.0));
        Some(
            mouse_area(slot)
                .on_middle_press(Message::SecondaryActivateTray(address.clone()))
                .on_right_press(Message::OpenTrayMenu { bar: bar_id, address: address.clone() })
                .into(),
        )
    });
    Row::with_children(items).spacing(10).height(Length::Fill).align_y(Alignment::Center).into()
}

fn tray_menu_view<'a>(bar: &'a Bar, open: &OpenMenu) -> Element<'a, Message> {
    let theme = bar.theme;
    let entries = bar
        .tray_items
        .get(&open.address)
        .and_then(|item| item.menu.as_ref())
        .map(|menu| menu::entries(menu, &open.path))
        .unwrap_or_default();

    let leading_column = menu::has_leading_column(&entries, !open.path.is_empty());

    let mut rows = Column::new();
    if !open.path.is_empty() {
        let parent = open.path[..open.path.len() - 1].to_vec();
        rows = rows
            .push(menu_row(theme, Some(icon("arrow_back")), "Back", false, Some(Message::NavigateTrayMenu(parent))))
            .push(menu_separator(theme));
    }
    for item in entries {
        if item.menu_type == MenuType::Separator {
            rows = rows.push(menu_separator(theme));
            continue;
        }
        let toggle = leading_column.then(|| match (item.toggle_type, item.toggle_state) {
            (ToggleType::Checkmark, ToggleState::On) => icon("check"),
            (ToggleType::Radio, ToggleState::On) => icon("radio_button_checked"),
            (ToggleType::Radio, _) => icon("radio_button_unchecked"),
            _ => Space::new().width(menu::ICON_WIDTH as f32).into(),
        });
        let has_submenu = !item.submenu.is_empty();
        let message = item.enabled.then(|| {
            if has_submenu {
                let mut path = open.path.clone();
                path.push(item.id);
                Message::NavigateTrayMenu(path)
            } else {
                Message::ActivateTrayMenuItem(item.id)
            }
        });
        rows = rows.push(menu_row(theme, toggle, item.label.as_deref().unwrap_or_default(), has_submenu, message));
    }

    let rows = container(rows).padding(Padding::ZERO.left(f32::from(menu::PADDING)).right(f32::from(menu::PADDING)));
    let body: Element<'a, Message> = match bar.menu_title(&open.address) {
        Some(title) => {
            let strip = container(text(title).font(CONDENSED).size(13).color(theme.accent))
                .padding([0, 14])
                .height(menu::HEADER_HEIGHT as f32)
                .center_y(menu::HEADER_HEIGHT as f32);
            let strip = container(slanted(strip, 10.0, theme.section_background)).padding(Padding::ZERO.right(16.0));
            column![strip, Space::new().height(f32::from(menu::HEADER_GAP)), rows].into()
        }
        None => container(rows).padding(Padding::ZERO.top(f32::from(menu::PADDING))).into(),
    };

    let menu = container(body)
        .padding(Padding::ZERO.bottom(f32::from(menu::PADDING)))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| panel_style(theme));
    // Transparent strip on top so the menu does not touch the bar.
    container(menu).padding(Padding::ZERO.top(f32::from(menu::GAP))).into()
}

fn menu_row<'a>(theme: theme::Theme, leading: Option<Element<'a, Message>>, label: &'a str, submenu: bool, on_press: Option<Message>) -> Element<'a, Message> {
    let content = Row::new()
        .push(leading)
        .push(text(label).size(13).width(Length::Fill))
        .push(submenu.then(|| icon("chevron_right")))
        .spacing(menu::SPACING as f32)
        .height(Length::Fill)
        .align_y(Alignment::Center);
    button(content)
        .on_press_maybe(on_press)
        .padding([0, menu::ROW_PADDING])
        .width(Length::Fill)
        .height(menu::ROW_HEIGHT as f32)
        .style(move |_, status| glow_style(theme, status, 8.0))
        .into()
}

fn menu_separator<'a>(theme: theme::Theme) -> Element<'a, Message> {
    container(line(theme.border))
        .padding([0, 4])
        .height(menu::SEPARATOR_HEIGHT as f32)
        .center_y(menu::SEPARATOR_HEIGHT as f32)
        .into()
}

fn clock_tooltip(bar: &Bar) -> Element<'_, Message> {
    let theme = bar.theme;
    let today = bar.now.date_naive();

    let date = column![
        text(bar.now.format("%A").to_string()).font(CONDENSED_BOLD).size(26),
        text(bar.now.format("%B %d, %Y").to_string()).font(BODY_BOLD).size(15).color(theme.text_muted),
    ]
    .spacing(2);
    let day = column![
        text("DAY").font(CONDENSED).size(12).color(theme.accent),
        row![
            text(today.day().to_string()).font(CONDENSED_HEAVY).size(34).color(theme.accent),
            text(format!("/{}", days_in_month(today))).font(BODY_BOLD).size(14).color(theme.text_muted),
        ]
        .spacing(2)
        .align_y(Alignment::End),
    ]
    .align_x(Alignment::End);
    let header = row![date, Space::new().width(Length::Fill), day]
        .align_y(Alignment::End)
        .padding([0, 16])
        .height(TOOLTIP_HEADER_HEIGHT as f32);

    let uptime = row![
        text("System uptime").size(14),
        Space::new().width(Length::Fill),
        text(system::format_uptime(bar.uptime)).font(CONDENSED).size(16),
    ]
    .align_y(Alignment::Center)
    .padding([0, 16])
    .height(TOOLTIP_ROW_HEIGHT as f32);

    let strip = row![
        text("TO DO").font(CONDENSED).size(14).color(theme.accent),
        Space::new().width(Length::Fill),
        text(format!("{} PENDING", bar.todos.pending)).font(CONDENSED).size(13).color(theme.text_muted),
    ]
    .align_y(Alignment::Center)
    .padding([0, 16])
    .height(TOOLTIP_STRIP_HEIGHT as f32);
    let strip = container(slanted(strip, 10.0, theme.section_background)).padding(Padding::ZERO.right(16.0));

    let (list_top, list_bottom) = TOOLTIP_LIST_PADDING;
    let todos = bar
        .todos
        .lines
        .iter()
        .map(|todo| text(todo).size(13).color(theme.text_muted).height(TOOLTIP_LINE_HEIGHT as f32).into());
    let todos = Column::with_children(todos).padding(Padding { top: f32::from(list_top), right: 16.0, bottom: f32::from(list_bottom), left: 16.0 });

    container(column![header, container(line(theme.accent).height(2)).padding([0, 16]), uptime, strip, todos])
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| panel_style(theme))
        .into()
}

fn days_in_month(date: NaiveDate) -> u32 {
    let (year, month) = if date.month() == 12 { (date.year() + 1, 1) } else { (date.year(), date.month() + 1) };
    NaiveDate::from_ymd_opt(year, month, 1).and_then(|next| next.pred_opt()).map_or(31, |last| last.day())
}

/// Places `content` over a rectangle with a diagonal right edge.
fn slanted<'a>(content: impl Into<Element<'a, Message>>, slant: f32, color: Color) -> Element<'a, Message> {
    Stack::new().push(content).push_under(shape(shape::Kind::Slant(slant), color)).into()
}

fn shape<'a>(kind: shape::Kind, color: Color) -> Element<'a, Message> {
    canvas(shape::Shape { kind, color }).width(Length::Fill).height(Length::Fill).into()
}

fn line<'a>(color: Color) -> container::Container<'a, Message> {
    container(Space::new()).width(Length::Fill).height(1).style(move |_| container::background(color))
}

fn panel_style(theme: theme::Theme) -> container::Style {
    container::Style {
        background: Some(theme.popup_background.into()),
        border: Border { color: theme.border, width: 1.0, radius: 0.0.into() },
        ..Default::default()
    }
}

/// Hover highlight of the HUD: a translucent fill with a glowing outline.
fn glow_style(theme: theme::Theme, status: button::Status, blur: f32) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    let text_color = if status == button::Status::Disabled { theme.text_muted } else { theme.text };
    if !hovered {
        return button::Style { text_color, ..Default::default() };
    }
    button::Style {
        background: Some(Color { a: 0.12, ..theme.glow }.into()),
        text_color,
        border: Border { color: theme.glow, width: 1.0, radius: 0.0.into() },
        shadow: Shadow { color: Color { a: 0.4, ..theme.glow }, blur_radius: blur, ..Default::default() },
        ..Default::default()
    }
}

fn icon(name: &str) -> Element<'_, Message> {
    text(name)
        .font(ICON_FONT)
        .size(16)
        .shaping(text::Shaping::Advanced)
        .into()
}

fn style(bar: &Bar, _: &iced::Theme) -> iced::theme::Style {
    iced::theme::Style {
        background_color: Color::TRANSPARENT,
        text_color: bar.theme.text,
    }
}
