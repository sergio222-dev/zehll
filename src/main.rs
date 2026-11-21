mod corner;
mod hyprland;
mod menu;
mod system;
mod theme;
mod tray;

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Local};
use iced::widget::{Column, Row, Space, button, canvas, column, container, image, mouse_area, row, svg, text};
use iced::window::Id;
use iced::{Alignment, Border, Color, Element, Font, Length, Subscription, Task, time, window};
use iced_layershell::actions::IcedNewPopupSettings;
use iced_layershell::daemon;
use iced_layershell::reexport::{Anchor, KeyboardInteractivity, Layer, NewLayerShellSettings, OutputOption, PopupAnchor, PopupGravity};
use iced_layershell::settings::{LayerShellSettings, Settings, StartMode};
use iced_layershell::to_layer_message;
use system_tray::client::{ActivateRequest, Client, Event as TrayEvent, UpdateEvent};
use system_tray::menu::{MenuType, ToggleState, ToggleType};

const BAR_HEIGHT: u32 = 40;
const BAR_PADDING: u16 = 16;
const ICON_FONT: Font = Font::with_name("Material Symbols Rounded");

const TOOLTIP_PADDING: u16 = 12;
const TOOLTIP_SPACING: u16 = 4;
const TOOLTIP_WIDTH: u32 = 300;
const TOOLTIP_LINE_HEIGHT: u32 = 21;

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
    todos: Vec<String>,
    tooltip: Option<Id>,
    // Still rendered as a tooltip while the compositor plays its close animation.
    closing_tooltip: Option<Id>,
    corners: HashMap<Id, corner::Side>,
    tray_client: Option<Arc<Client>>,
    // Keyed by D-Bus address, which keeps the icon order stable.
    tray_items: BTreeMap<String, tray::TrayItem>,
    tray_menu: Option<OpenMenu>,
}

struct OpenMenu {
    id: Id,
    address: String,
    // Ids of the submenus entered from the root.
    path: Vec<i32>,
}

impl Bar {
    fn new() -> (Self, Task<Message>) {
        let mut bar = Self {
            theme: theme::Theme::load(),
            theme_modified: theme::modified(),
            window: hyprland::active_window().unwrap_or_default(),
            now: Local::now(),
            uptime: Duration::ZERO,
            todos: Vec::new(),
            tooltip: None,
            closing_tooltip: None,
            corners: HashMap::new(),
            tray_client: None,
            tray_items: BTreeMap::new(),
            tray_menu: None,
        };
        let corners = create_corners(&mut bar);
        (bar, corners)
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

/// Opens the concave corners below the bar on every monitor.
fn create_corners(bar: &mut Bar) -> Task<Message> {
    let mut tasks = Vec::new();
    for output in hyprland::monitors() {
        for (side, anchor) in [(corner::Side::Left, Anchor::Left), (corner::Side::Right, Anchor::Right)] {
            // A zero exclusive zone makes the compositor place them right below the bar.
            let settings = NewLayerShellSettings {
                size: Some((corner::RADIUS, corner::RADIUS)),
                layer: Layer::Top,
                anchor: Anchor::Top | anchor,
                exclusive_zone: Some(0),
                margin: None,
                keyboard_interactivity: KeyboardInteractivity::None,
                output_option: OutputOption::OutputName(output.clone()),
                events_transparent: true,
                namespace: Some(String::from("zehll-corner")),
            };
            let id = Id::unique();
            bar.corners.insert(id, side);
            tasks.push(Task::done(Message::NewLayerShell { settings, id }));
        }
    }
    Task::batch(tasks)
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

            // The surface needs a fixed size: header, uptime and "To Do" rows plus one per task.
            let rows = 3 + bar.todos.len() as u32;
            let height = rows * TOOLTIP_LINE_HEIGHT + (rows - 1) * TOOLTIP_SPACING as u32 + 2 * TOOLTIP_PADDING as u32;
            let settings = NewLayerShellSettings {
                size: Some((TOOLTIP_WIDTH, height)),
                layer: Layer::Overlay,
                // Anchored only to the top, so it is centered under the clock.
                anchor: Anchor::Top,
                exclusive_zone: Some(0),
                margin: Some((4, 0, 0, 0)),
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
            let Some(menu) = bar.tray_items.get(&address).and_then(|item| item.menu.as_ref()) else {
                return close;
            };
            let (width, height) = menu::size(&menu::entries(menu, &[]), false);
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
            let Some(open) = &mut bar.tray_menu else {
                return Task::none();
            };
            let Some(menu) = bar.tray_items.get(&open.address).and_then(|item| item.menu.as_ref()) else {
                return Task::none();
            };
            let (width, height) = menu::size(&menu::entries(menu, &path), !path.is_empty());
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
    match bar.tray_menu.take() {
        Some(open) => Task::done(Message::RemoveWindow(open.id)),
        None => Task::none(),
    }
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
    if let Some(open) = bar.tray_menu.as_ref().filter(|open| open.id == id) {
        return tray_menu_view(bar, open);
    }
    if let Some(&side) = bar.corners.get(&id) {
        let corner = corner::Corner { side, color: bar.theme.bar_background };
        return canvas(corner).width(Length::Fill).height(Length::Fill).into();
    }

    let window = column![
        text(&bar.window.class).size(11).color(bar.theme.text_muted),
        text(&bar.window.title).size(14),
    ];

    // The hover area spans the full bar height so the tooltip is easy to trigger.
    let clock = mouse_area(
        container(text(bar.now.format("%H:%M • %a, %d/%m").to_string()).size(14))
            .padding([0, 12])
            .center_y(Length::Fill),
    )
        .on_enter(Message::ShowTooltip)
        .on_exit(Message::HideTooltip);

    container(
        row![
            container(window).width(Length::FillPortion(1)),
            container(clock).center_x(Length::FillPortion(1)),
            container(tray_view(bar, id)).align_right(Length::FillPortion(1)),
        ]
        .height(Length::Fill)
        .align_y(Alignment::Center),
    )
    .padding([0, BAR_PADDING])
    .style(move |_| container::background(bar.theme.bar_background))
    .into()
}

fn tray_view(bar: &Bar, bar_id: Id) -> Element<'_, Message> {
    let size = f32::from(tray::ICON_SIZE);
    let items = bar.tray_items.iter().filter_map(|(address, item)| {
        let icon: Element<'_, Message> = match item.icon.as_ref()? {
            tray::Icon::Raster(handle) => image(handle.clone()).width(size).height(size).into(),
            tray::Icon::Svg(handle) => svg(handle.clone()).width(size).height(size).into(),
        };
        Some(
            mouse_area(container(icon).padding([0, 4]).center_y(Length::Fill))
                .on_press(Message::ActivateTray(address.clone()))
                .on_middle_press(Message::SecondaryActivateTray(address.clone()))
                .on_right_press(Message::OpenTrayMenu { bar: bar_id, address: address.clone() })
                .into(),
        )
    });
    Row::with_children(items).height(Length::Fill).align_y(Alignment::Center).into()
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

    let menu = container(rows)
        .padding(menu::PADDING)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(theme.popup_background.into()),
            border: Border { color: theme.border, width: 1.0, radius: 12.0.into() },
            ..Default::default()
        });
    // Transparent strip on top so the menu does not touch the bar.
    container(menu).padding(iced::Padding::ZERO.top(f32::from(menu::GAP))).into()
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
        .style(move |_, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: hovered.then(|| Color { a: 0.1, ..theme.text }.into()),
                text_color: if status == button::Status::Disabled { theme.text_muted } else { theme.text },
                border: Border { radius: 8.0.into(), ..Default::default() },
                ..Default::default()
            }
        })
        .into()
}

fn menu_separator<'a>(theme: theme::Theme) -> Element<'a, Message> {
    container(container(Space::new()).width(Length::Fill).height(1).style(move |_| container::background(theme.border)))
        .height(menu::SEPARATOR_HEIGHT as f32)
        .center_y(menu::SEPARATOR_HEIGHT as f32)
        .into()
}

fn clock_tooltip(bar: &Bar) -> Element<'_, Message> {
    let header = row![
        icon("calendar_month"),
        text(bar.now.format("%A, %B %d, %Y").to_string()).size(15),
    ];
    let uptime = row![
        icon("timelapse"),
        text("System uptime:").size(14),
        container(text(system::format_uptime(bar.uptime)).size(14)).align_right(Length::Fill),
    ];
    let todos = bar.todos.iter().map(|todo| text(todo).size(14).color(bar.theme.text_muted).into());

    container(
        column![header.spacing(6), uptime.spacing(6), row![icon("checklist"), text("To Do:").size(14)].spacing(6)]
            .extend(todos)
            .spacing(f32::from(TOOLTIP_SPACING)),
    )
    .padding(TOOLTIP_PADDING)
    .width(Length::Fill)
    .height(Length::Fill)
    .style(move |_| container::Style {
        background: Some(bar.theme.popup_background.into()),
        border: Border { color: bar.theme.border, width: 1.0, radius: 12.0.into() },
        ..Default::default()
    })
    .into()
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
