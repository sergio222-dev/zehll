mod agenda;
mod alert;
mod audio;
mod bluetooth;
mod brightness;
mod calendar;
mod dbus;
mod flashbang;
mod hyprland;
mod idle;
mod meetings;
mod menu;
mod mixer;
mod network;
mod nightlight;
mod notifications;
mod panel;
mod pomodoro;
mod popup;
mod shape;
mod shortcuts;
mod state;
mod subpanel;
mod system;
mod theme;
mod todo;
mod tray;

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveDateTime};
use iced::font::{Style as FontStyle, Weight};
use iced::widget::{Column, Row, Space, Stack, button, canvas, column, container, image, mouse_area, row, svg, text};
use iced::window::Id;
use iced::{Alignment, Border, Color, Element, Font, Length, Padding, Shadow, Subscription, Task, keyboard, time, window};
use iced_layershell::actions::IcedNewPopupSettings;
use iced_layershell::daemon;
use iced_layershell::reexport::{Anchor, KeyboardInteractivity, Layer, NewLayerShellSettings, OutputOption, PopupAnchor, PopupGravity};
use iced_layershell::settings::{LayerShellSettings, Settings, StartMode};
use iced_layershell::to_layer_message;
use system_tray::client::{ActivateRequest, Client, Event as TrayEvent, UpdateEvent};
use system_tray::menu::{MenuType, ToggleState, ToggleType};

const BAR_HEIGHT: u32 = 44;
const BAR_PADDING: u16 = 16;
// After the panel closes, the pointer re-enters the bar's hover zone once the catcher is gone;
// hovering does not reopen the panel within this window.
const PANEL_REOPEN_DELAY: Duration = Duration::from_millis(400);
// calendarz's poll interval.
const MEETINGS_POLL: Duration = Duration::from_secs(120);
const PILL_BACKGROUND: Color = Color::from_rgb8(0x0d, 0x14, 0x30);

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
// Two 13 px lines with 8 px padding; long titles are clipped.
const MEETING_TOOLTIP_SIZE: (u32, u32) = (300, 56);

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
    // Every surface besides the bars, kept until the compositor reports it closed so a surface
    // playing its close animation is never drawn as a bar.
    surfaces: HashMap<Id, Surface>,
    tooltip: Option<Id>,
    tray_client: Option<Arc<Client>>,
    // Keyed by D-Bus address, which keeps the icon order stable.
    tray_items: BTreeMap<String, tray::TrayItem>,
    tray_menu: Option<Id>,
    panel: Option<OpenPanel>,
    panel_closed_at: Option<Instant>,
    calendar_month: NaiveDate,
    audio: Option<audio::Audio>,
    network: Option<network::Network>,
    bluetooth: Option<bluetooth::Bluetooth>,
    night_light: Option<u32>,
    // Last state the automatic schedule asked for; a change means a schedule boundary was crossed.
    night_schedule: Option<bool>,
    // logind idle inhibitor lock held while "keep awake" is on.
    keep_awake: Option<Arc<zbus::zvariant::OwnedFd>>,
    notifications_handle: Option<notifications::Handle>,
    // Newest first.
    notifications: Vec<notifications::Notification>,
    do_not_disturb: bool,
    // Newest first.
    popups: Vec<Popup>,
    popup_surface: Option<Id>,
    panel_tab: panel::Tab,
    todo_items: Vec<todo::Item>,
    todo_input: String,
    pomodoro: pomodoro::Pomodoro,
    subpanel: Option<subpanel::Subpanel>,
    mixer: Option<mixer::Mixer>,
    // Icons of audio streams by executable name, looked up once.
    app_icons: HashMap<String, Option<tray::Icon>>,
    // Stream index and volume while a volume slider is being dragged.
    volume_drag: Option<(u32, u32)>,
    sink_picker_open: bool,
    // Animation step of the scanning indicator.
    scan_frame: u8,
    night_temperature: u32,
    night_automatic: bool,
    content_adjustment: bool,
    gamma: Option<u32>,
    brightness: Option<brightness::Monitor>,
    // Slider being dragged in the eye protection sub-panel and its value.
    level_drag: Option<(subpanel::Level, u32)>,
    wifi: Option<network::Wifi>,
    // SSID asking for a password in the Wi-Fi sub-panel.
    wifi_prompt: Option<String>,
    wifi_password: String,
    meetings: Vec<meetings::Meeting>,
    meetings_status: meetings::Status,
    // `gcal.py auth` is waiting for the browser sign-in.
    meetings_signing_in: bool,
    // Meeting alerts already raised, so each fires once (see `meetings::due_alerts`).
    meetings_fired: HashSet<String>,
    // Meetings that started and still show an alert card, the ringing one first.
    meeting_alerts: Vec<meetings::Meeting>,
    alert_surface: Option<Id>,
    // `pw-play` playing the alert sound, stopped before the next ring and when the last card closes.
    ringing: Option<std::process::Child>,
    // Calendar popover and its click catcher.
    calendar: Option<OpenPanel>,
    // Day whose agenda the calendar popover shows.
    calendar_day: NaiveDate,
}

struct Popup {
    id: u32,
    expires: Option<Instant>,
}

enum Surface {
    Tooltip(Tooltip),
    TrayMenu(OpenMenu),
    Panel,
    Calendar,
    MeetingAlerts,
    Popups,
    // Transparent full-screen layer below the panel; clicking it closes the panel.
    Catcher,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tooltip {
    Clock,
    Meeting,
}

struct OpenMenu {
    address: String,
    // Ids of the submenus entered from the root.
    path: Vec<i32>,
}

struct OpenPanel {
    panel: Id,
    catcher: Id,
}

impl Bar {
    fn new() -> (Self, Task<Message>) {
        let state = state::State::load();
        let bar = Self {
            theme: theme::Theme::load(),
            theme_modified: theme::modified(),
            window: hyprland::active_window().unwrap_or_default(),
            now: Local::now(),
            uptime: Duration::ZERO,
            todos: system::Todos::default(),
            surfaces: HashMap::new(),
            tooltip: None,
            tray_client: None,
            tray_items: BTreeMap::new(),
            tray_menu: None,
            panel: None,
            panel_closed_at: None,
            calendar_month: panel::first_of_month(Local::now().date_naive()),
            audio: None,
            network: None,
            bluetooth: None,
            night_light: None,
            night_schedule: None,
            keep_awake: None,
            notifications_handle: None,
            notifications: Vec::new(),
            do_not_disturb: state.do_not_disturb,
            popups: Vec::new(),
            popup_surface: None,
            panel_tab: panel::Tab::Calendar,
            todo_items: Vec::new(),
            todo_input: String::new(),
            pomodoro: pomodoro::Pomodoro::default(),
            subpanel: None,
            mixer: None,
            app_icons: HashMap::new(),
            volume_drag: None,
            sink_picker_open: false,
            scan_frame: 0,
            night_temperature: state.night_temperature,
            night_automatic: state.night_automatic,
            content_adjustment: state.content_adjustment,
            gamma: None,
            brightness: None,
            level_drag: None,
            wifi: None,
            wifi_prompt: None,
            wifi_password: String::new(),
            meetings: Vec::new(),
            meetings_status: meetings::Status::Ok,
            meetings_signing_in: false,
            meetings_fired: HashSet::new(),
            meeting_alerts: Vec::new(),
            alert_surface: None,
            ringing: None,
            calendar: None,
            calendar_day: Local::now().date_naive(),
        };
        let restore = match state.keep_awake {
            true => Task::perform(idle::inhibit(), Message::KeepAwake),
            false => Task::none(),
        };
        // Runtime Hyprland options are gone after a restart, so reapply the shader.
        let restore = match state.content_adjustment {
            true => restore.chain(Task::perform(flashbang::set(true), Message::ContentAdjustment)),
            false => restore,
        };
        let restore = restore.chain(fetch_meetings(&bar));
        (bar, restore)
    }

    fn save_state(&self) {
        state::State {
            keep_awake: self.keep_awake.is_some(),
            do_not_disturb: self.do_not_disturb,
            night_temperature: self.night_temperature,
            night_automatic: self.night_automatic,
            content_adjustment: self.content_adjustment,
        }
        .save();
    }

    fn open_menu(&self) -> Option<(Id, &OpenMenu)> {
        let id = self.tray_menu?;
        match self.surfaces.get(&id)? {
            Surface::TrayMenu(open) => Some((id, open)),
            _ => None,
        }
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
    ShowTooltip(Tooltip),
    HideTooltip,
    Tray(tray::TrayEvent),
    ActivateTray(String),
    SecondaryActivateTray(String),
    OpenTrayMenu { bar: Id, address: String },
    NavigateTrayMenu(Vec<i32>),
    ActivateTrayMenuItem(i32),
    WindowClosed(Id),
    TogglePanel,
    HoverPanel,
    ClosePanel,
    Key(keyboard::Event),
    CalendarMonth(i32),
    ReloadShell,
    PowerMenu,
    Audio(audio::Audio),
    ToggleMute(&'static str),
    Network(network::Network),
    SetWifi(bool),
    Bluetooth(bluetooth::Bluetooth),
    SetBluetooth(zbus::zvariant::OwnedObjectPath, bool),
    NightLight(Option<u32>),
    SetNightLight(bool),
    ToggleKeepAwake,
    KeepAwake(Option<Arc<zbus::zvariant::OwnedFd>>),
    Notifications(notifications::Event),
    DismissNotification(u32),
    ClearNotifications,
    ToggleDoNotDisturb,
    HidePopup(u32),
    InvokeAction(u32, String),
    SelectTab(panel::Tab),
    TodoInput(String),
    AddTodo,
    ToggleTodo(usize, String),
    RemoveTodo(usize, String),
    TogglePomodoro,
    ResetPomodoro,
    SkipPomodoro,
    OpenSubpanel(subpanel::Subpanel),
    CloseSubpanel,
    SubpanelDetails(subpanel::Subpanel),
    Mixer(mixer::Mixer),
    SetStreamVolume(u32, u32),
    DragStreamVolume(u32, u32),
    ReleaseStreamVolume,
    ToggleSinkPicker,
    ScanFrame,
    ToggleNightAutomatic,
    ContentAdjustment(bool),
    SetContentAdjustment(bool),
    Gamma(Option<u32>),
    Brightness(Option<brightness::Monitor>),
    DragLevel(subpanel::Level, u32),
    ReleaseLevel,
    SetDefaultSink(String),
    BluetoothDevice(bluetooth::DeviceAction),
    Wifi(network::Wifi),
    SelectWifi(String),
    WifiPassword(String),
    SubmitWifiPassword,
    FetchMeetings,
    SignInMeetings,
    ToggleCalendar,
    SelectDay(NaiveDate),
    OpenMeetingLink(String),
    MeetingTest,
    RingMeetingAlert,
    DismissMeetingAlert(String, NaiveDateTime),
    JoinMeetingAlert(String, NaiveDateTime),
    SignedInMeetings,
    Meetings((NaiveDate, NaiveDate), meetings::Status, Vec<meetings::Meeting>),
}

fn namespace() -> String {
    String::from("zehll")
}

fn subscription(bar: &Bar) -> Subscription<Message> {
    // Keys (Escape) and the panel's data sources only run while the panel is open.
    let mixer = match bar.subpanel {
        Some(subpanel::Subpanel::Audio) => Subscription::run(mixer::events).map(Message::Mixer),
        Some(subpanel::Subpanel::Wifi) => Subscription::run(network::wifi_events).map(Message::Wifi),
        // Holds a discovery session; it emits nothing.
        Some(subpanel::Subpanel::Bluetooth) => Subscription::run(bluetooth::discover).map(|()| Message::ScanFrame),
        _ => Subscription::none(),
    };
    // The scanning indicator animates only while a scan runs.
    let scanning = match bar.subpanel {
        Some(subpanel::Subpanel::Wifi) => bar.wifi.as_ref().is_some_and(|wifi| wifi.scanning),
        Some(subpanel::Subpanel::Bluetooth) => bar.bluetooth.as_ref().is_some_and(|bluetooth| bluetooth.discovering),
        _ => false,
    };
    let scan_animation = match scanning {
        true => time::every(Duration::from_millis(150)).map(|_| Message::ScanFrame),
        false => Subscription::none(),
    };
    let ring = match bar.meeting_alerts.is_empty() {
        true => Subscription::none(),
        false => time::every(alert::RING_INTERVAL).map(|_| Message::RingMeetingAlert),
    };
    let calendar_keys = match bar.calendar {
        Some(_) => keyboard::listen().map(Message::Key),
        None => Subscription::none(),
    };
    let panel = match bar.panel {
        Some(_) => Subscription::batch([
            mixer,
            scan_animation,
            keyboard::listen().map(Message::Key),
            Subscription::run(audio::events).map(Message::Audio),
            Subscription::run(network::events).map(Message::Network),
            Subscription::run(bluetooth::events).map(Message::Bluetooth),
        ]),
        None => Subscription::none(),
    };
    Subscription::batch([
        time::every(Duration::from_secs(1)).map(|_| Message::Tick),
        time::every(MEETINGS_POLL).map(|_| Message::FetchMeetings),
        Subscription::run(hyprland::active_window_events).map(Message::ActiveWindow),
        Subscription::run(tray::events).map(Message::Tray),
        Subscription::run(shortcuts::events).map(|shortcut| match shortcut {
            shortcuts::Shortcut::PanelToggle => Message::TogglePanel,
            shortcuts::Shortcut::MeetingTest => Message::MeetingTest,
        }),
        Subscription::run(notifications::events).map(Message::Notifications),
        window::close_events().map(Message::WindowClosed),
        panel,
        calendar_keys,
        ring,
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
            if bar.tooltip.is_some() || bar.panel.is_some() {
                bar.uptime = system::uptime().unwrap_or_default();
            }
            // Every step runs each second; none returns early and skips the ones after it.
            let mut tasks = Vec::new();
            // The pill disappears under the pointer when its meeting starts, without a pointer exit.
            let meeting_tooltip = bar.tooltip.and_then(|id| bar.surfaces.get(&id));
            if matches!(meeting_tooltip, Some(Surface::Tooltip(Tooltip::Meeting)))
                && meetings::next_meeting(&bar.meetings, bar.now.naive_local()).is_none()
            {
                tasks.push(Task::done(Message::HideTooltip));
            }
            tasks.extend(meeting_alerts(bar));
            if let Some(finished) = bar.pomodoro.tick(Duration::from_secs(1)) {
                tasks.push(pomodoro_finished(finished));
            }
            let now = Instant::now();
            let before = bar.popups.len();
            bar.popups.retain(|popup| popup.expires.is_none_or(|expires| expires > now));
            if bar.popups.len() != before {
                tasks.push(sync_popups(bar));
            }
            // Like ii, apply the schedule at startup and when it changes; manual toggles hold until then.
            let scheduled = nightlight::scheduled(bar.now.time());
            if bar.night_automatic && bar.night_schedule != Some(scheduled) {
                bar.night_schedule = Some(scheduled);
                tasks.push(Task::perform(nightlight::set(scheduled, bar.night_temperature), Message::NightLight));
            }
            return Task::batch(tasks);
        }
        Message::ActiveWindow(window) => bar.window = window,
        Message::ShowTooltip(tooltip) => {
            // The tooltips open under the clock, where the calendar popover is.
            if bar.tooltip.is_some() || bar.calendar.is_some() {
                return Task::none();
            }
            // A layer surface instead of an xdg_popup: layershellev grabs the pointer for every
            // popup, which sends a leave to the bar and makes the tooltip flicker.
            // Hyprland moves monitor focus with the cursor, so the focused monitor is the hovered bar.
            let Some(output) = hyprland::focused_monitor() else {
                return Task::none();
            };
            // The surface needs a fixed size, and every block of the tooltip has a fixed height.
            let size = match tooltip {
                Tooltip::Clock => {
                    bar.uptime = system::uptime().unwrap_or_default();
                    bar.todos = system::pending_todos();
                    let (list_top, list_bottom) = TOOLTIP_LIST_PADDING;
                    let list =
                        bar.todos.lines.len() as u32 * TOOLTIP_LINE_HEIGHT + list_top as u32 + list_bottom as u32;
                    (TOOLTIP_WIDTH, TOOLTIP_HEADER_HEIGHT + 2 + TOOLTIP_ROW_HEIGHT + TOOLTIP_STRIP_HEIGHT + list + 2)
                }
                Tooltip::Meeting => MEETING_TOOLTIP_SIZE,
            };
            let settings = NewLayerShellSettings {
                size: Some(size),
                layer: Layer::Overlay,
                // Anchored only to the top, so it is centered under the clock and the meeting pill.
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
            bar.surfaces.insert(id, Surface::Tooltip(tooltip));
            return Task::done(Message::NewLayerShell { settings, id });
        }
        Message::Tray(tray::TrayEvent::Ready(client)) => bar.tray_client = Some(client),
        Message::Tray(tray::TrayEvent::Event(event)) => match event {
            TrayEvent::Add(address, item) => {
                bar.tray_items.insert(address, tray::TrayItem::new(&item));
            }
            TrayEvent::Remove(address) => {
                bar.tray_items.remove(&address);
                if bar.open_menu().is_some_and(|(_, open)| open.address == address) {
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
            bar.tray_menu = Some(id);
            bar.surfaces.insert(id, Surface::TrayMenu(OpenMenu { address, path: Vec::new() }));
            return close.chain(Task::done(Message::NewPopUp { settings, id }));
        }
        Message::NavigateTrayMenu(path) => {
            let Some((id, open)) = bar.open_menu() else {
                return Task::none();
            };
            let has_header = bar.menu_title(&open.address).is_some();
            let Some(menu) = bar.tray_items.get(&open.address).and_then(|item| item.menu.as_ref()) else {
                return Task::none();
            };
            let size = menu::size(&menu::entries(menu, &path), !path.is_empty(), has_header);
            if let Some(Surface::TrayMenu(open)) = bar.surfaces.get_mut(&id) {
                open.path = path;
            }
            return Task::done(Message::SizeChange { id, size: (size.0, size.1 + menu::GAP as u32) });
        }
        Message::ActivateTrayMenuItem(submenu_id) => {
            let Some((_, open)) = bar.open_menu() else {
                return Task::none();
            };
            let Some(menu_path) = bar.tray_items.get(&open.address).and_then(|item| item.menu_path.clone()) else {
                return close_tray_menu(bar);
            };
            let request = ActivateRequest::MenuItem { address: open.address.clone(), menu_path, submenu_id };
            return activate_tray(bar, request).chain(close_tray_menu(bar));
        }
        Message::WindowClosed(id) => {
            bar.surfaces.remove(&id);
            // The compositor dismissed it, for example a popup clicked outside.
            if bar.tray_menu == Some(id) {
                bar.tray_menu = None;
            }
            if bar.popup_surface == Some(id) {
                bar.popup_surface = None;
            }
            if bar.alert_surface == Some(id) {
                bar.alert_surface = None;
            }
            if bar.panel.as_ref().is_some_and(|open| open.panel == id || open.catcher == id) {
                return close_panel(bar);
            }
            if bar.calendar.as_ref().is_some_and(|open| open.panel == id || open.catcher == id) {
                return close_calendar(bar);
            }
        }
        Message::HideTooltip => {
            if let Some(id) = bar.tooltip.take() {
                return Task::done(Message::RemoveWindow(id));
            }
        }
        Message::TogglePanel => {
            return if bar.panel.is_some() { close_panel(bar) } else { open_panel(bar) };
        }
        Message::HoverPanel => {
            let reopening = bar.panel_closed_at.is_some_and(|closed| closed.elapsed() < PANEL_REOPEN_DELAY);
            if bar.panel.is_none() && !reopening {
                return open_panel(bar);
            }
        }
        // Sent by the click catcher, which both the panel and the calendar popover use.
        Message::ClosePanel => return Task::batch([close_panel(bar), close_calendar(bar)]),
        Message::Key(keyboard::Event::KeyPressed { key: keyboard::Key::Named(keyboard::key::Named::Escape), .. }) => {
            if bar.calendar.is_some() {
                return close_calendar(bar);
            }
            // Escape closes the sub-panel first, then the panel.
            if bar.subpanel.take().is_none() {
                return close_panel(bar);
            }
        }
        Message::OpenSubpanel(subpanel) => {
            bar.subpanel = Some(subpanel);
            bar.wifi_prompt = None;
            bar.sink_picker_open = false;
            if subpanel == subpanel::Subpanel::EyeProtection {
                let output = hyprland::focused_monitor().unwrap_or_default();
                return Task::batch([
                    Task::perform(nightlight::gamma(), Message::Gamma),
                    Task::perform(flashbang::is_enabled(), Message::ContentAdjustment),
                    Task::perform(brightness::read(output), Message::Brightness),
                ]);
            }
        }
        Message::CloseSubpanel => bar.subpanel = None,
        Message::SubpanelDetails(subpanel) => {
            let [program, args @ ..] = subpanel.details() else {
                return Task::none();
            };
            if let Err(err) = std::process::Command::new(program).args(args).spawn() {
                eprintln!("zehll: cannot start {program}: {err}");
            }
            return close_panel(bar);
        }
        Message::Mixer(mixer) => {
            for binary in mixer.streams.iter().filter_map(|stream| stream.binary.clone()) {
                bar.app_icons.entry(binary).or_insert_with_key(|binary| tray::lookup(binary, None));
            }
            bar.mixer = Some(mixer);
        }
        Message::SetStreamVolume(index, percent) => return Task::future(mixer::set_volume(index, percent)).discard(),
        Message::DragStreamVolume(index, percent) => bar.volume_drag = Some((index, percent)),
        Message::ReleaseStreamVolume => {
            if let Some((index, percent)) = bar.volume_drag.take() {
                return Task::future(mixer::set_volume(index, percent)).discard();
            }
        }
        Message::ToggleSinkPicker => bar.sink_picker_open = !bar.sink_picker_open,
        Message::ScanFrame => bar.scan_frame = bar.scan_frame.wrapping_add(1),
        Message::SetDefaultSink(name) => {
            bar.sink_picker_open = false;
            return Task::future(mixer::set_default_sink(name)).discard();
        }
        Message::BluetoothDevice(action) => return Task::future(bluetooth::run(action)).discard(),
        Message::Wifi(wifi) => bar.wifi = Some(wifi),
        Message::SelectWifi(ssid) => {
            let Some(wifi) = &bar.wifi else {
                return Task::none();
            };
            let Some(network) = wifi.networks.iter().find(|network| network.ssid == ssid) else {
                return Task::none();
            };
            if network.secure && !network.known {
                bar.wifi_prompt = Some(ssid);
                bar.wifi_password.clear();
                return Task::none();
            }
            bar.wifi_prompt = None;
            return Task::future(network::connect(wifi.device.clone(), network.path.clone(), ssid, None)).discard();
        }
        Message::WifiPassword(password) => bar.wifi_password = password,
        Message::SubmitWifiPassword => {
            let (Some(wifi), Some(ssid)) = (&bar.wifi, bar.wifi_prompt.take()) else {
                return Task::none();
            };
            let Some(network) = wifi.networks.iter().find(|network| network.ssid == ssid) else {
                return Task::none();
            };
            let password = std::mem::take(&mut bar.wifi_password);
            return Task::future(network::connect(wifi.device.clone(), network.path.clone(), ssid, Some(password))).discard();
        }
        Message::CalendarMonth(delta) => {
            bar.calendar_month = panel::shift_month(bar.calendar_month, delta);
            return fetch_meetings(bar);
        }
        Message::FetchMeetings => return fetch_meetings(bar),
        Message::ToggleCalendar => {
            return if bar.calendar.is_some() { close_calendar(bar) } else { open_calendar(bar) };
        }
        Message::SelectDay(day) => {
            bar.calendar_day = day;
            let month = panel::first_of_month(day);
            if month != bar.calendar_month {
                bar.calendar_month = month;
                return fetch_meetings(bar);
            }
        }
        Message::OpenMeetingLink(link) => {
            open_link(&link);
            return close_calendar(bar);
        }
        Message::RingMeetingAlert => {
            if !bar.meeting_alerts.is_empty() {
                bar.ringing = alert::ring(bar.ringing.take());
            }
        }
        Message::MeetingTest => return show_alert_cards(bar, vec![meetings::test_meeting(bar.now.naive_local())]),
        Message::DismissMeetingAlert(id, start) => {
            bar.meeting_alerts.retain(|meeting| meeting.id != id || meeting.start != start);
            return sync_alert_cards(bar);
        }
        Message::JoinMeetingAlert(id, start) => {
            if let Some(meeting) = bar.meeting_alerts.iter().find(|meeting| meeting.id == id && meeting.start == start) {
                open_link(&meeting.link);
            }
            bar.meeting_alerts.retain(|meeting| meeting.id != id || meeting.start != start);
            return sync_alert_cards(bar);
        }
        Message::SignInMeetings => {
            if !bar.meetings_signing_in {
                bar.meetings_signing_in = true;
                return Task::perform(meetings::sign_in(), |()| Message::SignedInMeetings);
            }
        }
        Message::SignedInMeetings => {
            bar.meetings_signing_in = false;
            return fetch_meetings(bar);
        }
        Message::Meetings(range, status, meetings) => {
            // A reply for a month no longer shown would drop the meetings of the current one.
            let current = meetings::fetch_range(bar.calendar_month, bar.now.date_naive());
            if range == current && status != meetings::Status::Error {
                bar.meetings_status = status;
                bar.meetings = meetings;
            }
        }
        Message::ReloadShell => {
            // Start a fresh instance and leave; it takes over the D-Bus names once this one exits.
            match std::env::current_exe().and_then(|exe| std::process::Command::new(exe).spawn()) {
                Ok(_) => std::process::exit(0),
                Err(err) => eprintln!("zehll: cannot restart: {err}"),
            }
        }
        Message::Audio(audio) => bar.audio = Some(audio),
        Message::ToggleMute(target) => return Task::future(audio::toggle_mute(target)).discard(),
        Message::Network(network) => bar.network = Some(network),
        Message::SetWifi(enabled) => return Task::future(network::set_wifi(enabled)).discard(),
        Message::Bluetooth(bluetooth) => bar.bluetooth = Some(bluetooth),
        Message::NightLight(temperature) => bar.night_light = temperature,
        Message::SetNightLight(on) => return Task::perform(nightlight::set(on, bar.night_temperature), Message::NightLight),
        Message::ToggleNightAutomatic => {
            bar.night_automatic = !bar.night_automatic;
            // Re-evaluate the schedule on the next tick when automatic mode comes back.
            bar.night_schedule = None;
            bar.save_state();
        }
        Message::ContentAdjustment(enabled) => bar.content_adjustment = enabled,
        Message::SetContentAdjustment(enabled) => {
            bar.content_adjustment = enabled;
            bar.save_state();
            return Task::perform(flashbang::set(enabled), Message::ContentAdjustment);
        }
        Message::Gamma(gamma) => bar.gamma = gamma,
        Message::Brightness(monitor) => bar.brightness = monitor,
        Message::DragLevel(level, value) => bar.level_drag = Some((level, value)),
        Message::ReleaseLevel => {
            // Like the volume sliders, apply once on release: these commands are slow.
            let Some((level, value)) = bar.level_drag.take() else {
                return Task::none();
            };
            match level {
                subpanel::Level::Temperature => {
                    bar.night_temperature = value;
                    bar.save_state();
                    if bar.night_light.is_some_and(nightlight::is_on) {
                        return Task::perform(nightlight::set(true, value), Message::NightLight);
                    }
                }
                subpanel::Level::Gamma => return Task::perform(nightlight::set_gamma(value), Message::Gamma),
                subpanel::Level::Brightness => {
                    let Some(monitor) = &mut bar.brightness else {
                        return Task::none();
                    };
                    monitor.percent = value;
                    return Task::future(brightness::set(monitor.clone(), value)).discard();
                }
            }
        }
        Message::ToggleKeepAwake => {
            // Dropping the lock releases the inhibitor.
            if bar.keep_awake.take().is_none() {
                return Task::perform(idle::inhibit(), Message::KeepAwake);
            }
            bar.save_state();
        }
        Message::KeepAwake(lock) => {
            bar.keep_awake = lock;
            bar.save_state();
        }
        Message::Notifications(notifications::Event::Ready(handle)) => bar.notifications_handle = Some(handle),
        Message::Notifications(notifications::Event::Notify(notification)) => {
            let (id, timeout) = (notification.id, notification.timeout);
            match bar.notifications.iter_mut().find(|existing| existing.id == id) {
                Some(existing) => *existing = notification,
                None => bar.notifications.insert(0, notification),
            }
            if !bar.do_not_disturb {
                bar.popups.retain(|popup| popup.id != id);
                bar.popups.insert(0, Popup { id, expires: timeout.map(|timeout| Instant::now() + timeout) });
                bar.popups.truncate(popup::MAX_VISIBLE);
                return sync_popups(bar);
            }
        }
        Message::Notifications(notifications::Event::Close(id)) => {
            bar.notifications.retain(|notification| notification.id != id);
            return hide_popups(bar, |popup| popup.id == id);
        }
        Message::DismissNotification(id) => {
            bar.notifications.retain(|notification| notification.id != id);
            return notifications_closed(bar, vec![id]).chain(hide_popups(bar, |popup| popup.id == id));
        }
        Message::ClearNotifications => {
            let ids = bar.notifications.drain(..).map(|notification| notification.id).collect();
            return notifications_closed(bar, ids).chain(hide_popups(bar, |_| true));
        }
        Message::HidePopup(id) => return hide_popups(bar, |popup| popup.id == id),
        Message::SelectTab(tab) => {
            bar.panel_tab = tab;
            if tab == panel::Tab::Todo {
                bar.todo_items = todo::load();
            }
        }
        Message::TodoInput(input) => bar.todo_input = input,
        Message::TogglePomodoro => bar.pomodoro.running = !bar.pomodoro.running,
        Message::ResetPomodoro => bar.pomodoro = pomodoro::Pomodoro::default(),
        Message::SkipPomodoro => {
            bar.pomodoro.skip();
        }
        Message::AddTodo => {
            let content = bar.todo_input.trim().to_owned();
            if !content.is_empty() {
                bar.todo_input.clear();
                update_todos(bar, |items| items.push(todo::Item { content, done: false }));
            }
        }
        Message::ToggleTodo(index, content) => update_todos(bar, |items| todo::toggle(items, index, &content)),
        Message::RemoveTodo(index, content) => update_todos(bar, |items| todo::remove(items, index, &content)),
        Message::InvokeAction(id, key) => {
            // The specification lets the server close a notification once an action ran.
            let invoke = match bar.notifications_handle.clone() {
                Some(handle) => Task::future(handle.invoke(id, key)).discard(),
                None => Task::none(),
            };
            bar.notifications.retain(|notification| notification.id != id);
            return invoke.chain(notifications_closed(bar, vec![id])).chain(hide_popups(bar, |popup| popup.id == id));
        }
        Message::ToggleDoNotDisturb => {
            bar.do_not_disturb = !bar.do_not_disturb;
            bar.save_state();
            if bar.do_not_disturb {
                return hide_popups(bar, |_| true);
            }
        }
        Message::SetBluetooth(adapter, powered) => {
            return Task::future(bluetooth::set_powered(adapter, powered)).discard();
        }
        Message::PowerMenu => {
            if let Err(err) = std::process::Command::new("wlogout").spawn() {
                eprintln!("zehll: cannot start wlogout: {err}");
            }
            return close_panel(bar);
        }
        _ => {}
    }
    Task::none()
}

/// Raises the meeting alerts due now: the heads-up notification 5 minutes before, and the
/// alert card at the start.
fn meeting_alerts(bar: &mut Bar) -> Vec<Task<Message>> {
    let now = bar.now.naive_local();
    let due: Vec<_> = meetings::due_alerts(&bar.meetings, now, &mut bar.meetings_fired)
        .into_iter()
        .map(|(kind, meeting)| (kind, meeting.clone()))
        .collect();
    let mut tasks = Vec::new();
    let mut started = Vec::new();
    for (kind, meeting) in due {
        match kind {
            meetings::AlertKind::Warning => {
                let (summary, body) = meetings::warning_text(&meeting);
                let send = async move { notifications::send("Calendar", "x-office-calendar", &summary, &body).await };
                tasks.push(Task::future(send).discard());
            }
            meetings::AlertKind::Start => started.push(meeting),
        }
    }
    tasks.push(show_alert_cards(bar, started));
    tasks
}

/// Adds alert cards for `started` meetings.
fn show_alert_cards(bar: &mut Bar, started: Vec<meetings::Meeting>) -> Task<Message> {
    if started.is_empty() {
        return Task::none();
    }
    bar.meeting_alerts.extend(started);
    bar.ringing = alert::ring(bar.ringing.take());
    // The cards open where the calendar popover is.
    Task::batch([close_calendar(bar), sync_alert_cards(bar)])
}

/// Opens a meeting's Meet link or event page in the browser.
fn open_link(link: &str) {
    if let Err(err) = std::process::Command::new("xdg-open").arg(link).spawn() {
        eprintln!("zehll: cannot open {link}: {err}");
    }
}

/// Opens, resizes or closes the alert card surface, centered under the clock.
fn sync_alert_cards(bar: &mut Bar) -> Task<Message> {
    if bar.meeting_alerts.is_empty() {
        alert::stop(bar.ringing.take());
    }
    let height = alert::height(bar.meeting_alerts.len());
    match (bar.alert_surface, height) {
        (Some(id), 0) => {
            bar.alert_surface = None;
            Task::done(Message::RemoveWindow(id))
        }
        (Some(id), height) => Task::done(Message::SizeChange { id, size: (alert::WIDTH, height) }),
        (None, 0) => Task::none(),
        (None, height) => {
            let Some(output) = hyprland::focused_monitor() else {
                return Task::none();
            };
            let settings = NewLayerShellSettings {
                size: Some((alert::WIDTH, height)),
                layer: Layer::Overlay,
                anchor: Anchor::Top,
                exclusive_zone: Some(0),
                margin: Some((14, 0, 0, 0)),
                keyboard_interactivity: KeyboardInteractivity::None,
                output_option: OutputOption::OutputName(output),
                events_transparent: false,
                namespace: Some(String::from("zehll-meeting-alerts")),
            };
            let id = Id::unique();
            bar.alert_surface = Some(id);
            bar.surfaces.insert(id, Surface::MeetingAlerts);
            Task::done(Message::NewLayerShell { settings, id })
        }
    }
}

fn pomodoro_finished(finished: pomodoro::Phase) -> Task<Message> {
    let (summary, body) = match finished {
        pomodoro::Phase::Focus => ("Focus done", "Time for a break."),
        pomodoro::Phase::Break | pomodoro::Phase::LongBreak => ("Break over", "Back to focus."),
    };
    Task::future(notifications::send("Zehll", "timer", summary, body)).discard()
}

/// Fetches the meetings of the month shown in the calendar and of the next two days.
fn fetch_meetings(bar: &Bar) -> Task<Message> {
    let range = meetings::fetch_range(bar.calendar_month, bar.now.date_naive());
    Task::perform(meetings::fetch(range.0, range.1), move |(status, meetings)| {
        Message::Meetings(range, status, meetings)
    })
}

/// Applies `change` to a fresh read of the todo file, since ii may have edited it meanwhile.
fn update_todos(bar: &mut Bar, change: impl FnOnce(&mut Vec<todo::Item>)) {
    let mut items = todo::load();
    change(&mut items);
    todo::save(&items);
    bar.todo_items = items;
}

fn hide_popups(bar: &mut Bar, hide: impl Fn(&Popup) -> bool) -> Task<Message> {
    let before = bar.popups.len();
    bar.popups.retain(|popup| !hide(popup));
    if bar.popups.len() == before {
        return Task::none();
    }
    sync_popups(bar)
}

/// Creates, resizes or removes the popup surface to match the visible popups.
fn sync_popups(bar: &mut Bar) -> Task<Message> {
    let visible = bar.popups.iter().filter_map(|popup| bar.notifications.iter().find(|notification| notification.id == popup.id));
    let height = popup::height(visible);
    match (bar.popup_surface, height) {
        (Some(id), 0) => {
            bar.popup_surface = None;
            Task::done(Message::RemoveWindow(id))
        }
        (Some(id), height) => Task::done(Message::SizeChange { id, size: (popup::WIDTH, height) }),
        (None, 0) => Task::none(),
        (None, height) => {
            let Some(output) = hyprland::focused_monitor() else {
                return Task::none();
            };
            let settings = NewLayerShellSettings {
                size: Some((popup::WIDTH, height)),
                layer: Layer::Overlay,
                anchor: Anchor::Top | Anchor::Right,
                exclusive_zone: Some(0),
                margin: Some((8, 12, 0, 0)),
                keyboard_interactivity: KeyboardInteractivity::None,
                output_option: OutputOption::OutputName(output),
                events_transparent: false,
                namespace: Some(String::from("zehll-popups")),
            };
            let id = Id::unique();
            bar.popup_surface = Some(id);
            bar.surfaces.insert(id, Surface::Popups);
            Task::done(Message::NewLayerShell { settings, id })
        }
    }
}

fn notifications_closed(bar: &Bar, ids: Vec<u32>) -> Task<Message> {
    match bar.notifications_handle.clone() {
        Some(handle) => Task::future(handle.closed(ids, notifications::DISMISSED)).discard(),
        None => Task::none(),
    }
}

fn close_tray_menu(bar: &mut Bar) -> Task<Message> {
    match bar.tray_menu.take() {
        Some(id) => Task::done(Message::RemoveWindow(id)),
        None => Task::none(),
    }
}

fn open_panel(bar: &mut Bar) -> Task<Message> {
    let Some(output) = hyprland::focused_monitor() else {
        return Task::none();
    };
    let close_calendar = close_calendar(bar);
    bar.uptime = system::uptime().unwrap_or_default();
    let month = panel::first_of_month(bar.now.date_naive());
    let fetch = match month == bar.calendar_month {
        true => Task::none(),
        false => {
            bar.calendar_month = month;
            fetch_meetings(bar)
        }
    };
    bar.todo_items = todo::load();

    let catcher = catcher_settings(output.clone(), "zehll-panel-catcher");
    let panel = NewLayerShellSettings {
        size: Some((panel::WIDTH, 0)),
        layer: Layer::Overlay,
        anchor: Anchor::Top | Anchor::Bottom | Anchor::Right,
        exclusive_zone: Some(0),
        margin: None,
        keyboard_interactivity: KeyboardInteractivity::OnDemand,
        output_option: OutputOption::OutputName(output),
        events_transparent: false,
        namespace: Some(String::from("zehll-panel")),
    };
    let open = OpenPanel { panel: Id::unique(), catcher: Id::unique() };
    bar.surfaces.insert(open.catcher, Surface::Catcher);
    bar.surfaces.insert(open.panel, Surface::Panel);
    let task = Task::done(Message::NewLayerShell { settings: catcher, id: open.catcher })
        .chain(Task::done(Message::NewLayerShell { settings: panel, id: open.panel }))
        .chain(Task::perform(nightlight::temperature(), Message::NightLight));
    bar.panel = Some(open);
    Task::batch([close_calendar, task, fetch])
}

/// Transparent layer covering the screen that catches clicks outside a panel or popover:
/// layer surfaces have no "click outside" event. Created first so the surface stacks above it.
fn catcher_settings(output: String, namespace: &str) -> NewLayerShellSettings {
    NewLayerShellSettings {
        size: Some((0, 0)),
        layer: Layer::Overlay,
        anchor: Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right,
        exclusive_zone: Some(-1),
        margin: None,
        keyboard_interactivity: KeyboardInteractivity::None,
        output_option: OutputOption::OutputName(output),
        events_transparent: false,
        namespace: Some(String::from(namespace)),
    }
}

/// Opens the calendar popover under the clock on today, closing the panel and the tooltip.
fn open_calendar(bar: &mut Bar) -> Task<Message> {
    let Some(output) = hyprland::focused_monitor() else {
        return Task::none();
    };
    let today = bar.now.date_naive();
    bar.calendar_day = today;
    let month = panel::first_of_month(today);
    let fetch = match month == bar.calendar_month {
        true => Task::none(),
        false => {
            bar.calendar_month = month;
            fetch_meetings(bar)
        }
    };
    let popover = NewLayerShellSettings {
        size: Some((agenda::WIDTH, agenda::HEIGHT)),
        layer: Layer::Overlay,
        // Anchored only to the top, so it is centered under the clock.
        anchor: Anchor::Top,
        exclusive_zone: Some(0),
        margin: Some((14, 0, 0, 0)),
        keyboard_interactivity: KeyboardInteractivity::OnDemand,
        output_option: OutputOption::OutputName(output.clone()),
        events_transparent: false,
        namespace: Some(String::from("zehll-calendar")),
    };
    let open = OpenPanel { panel: Id::unique(), catcher: Id::unique() };
    bar.surfaces.insert(open.catcher, Surface::Catcher);
    bar.surfaces.insert(open.panel, Surface::Calendar);
    let task = Task::done(Message::NewLayerShell {
        settings: catcher_settings(output, "zehll-calendar-catcher"),
        id: open.catcher,
    })
    .chain(Task::done(Message::NewLayerShell { settings: popover, id: open.panel }));
    bar.calendar = Some(open);
    Task::batch([close_panel(bar), Task::done(Message::HideTooltip), task, fetch])
}

fn close_calendar(bar: &mut Bar) -> Task<Message> {
    let Some(open) = bar.calendar.take() else {
        return Task::none();
    };
    Task::batch([open.panel, open.catcher].map(|id| match bar.surfaces.contains_key(&id) {
        true => Task::done(Message::RemoveWindow(id)),
        false => Task::none(),
    }))
}

fn close_panel(bar: &mut Bar) -> Task<Message> {
    let Some(open) = bar.panel.take() else {
        return Task::none();
    };
    bar.subpanel = None;
    bar.panel_closed_at = Some(Instant::now());
    Task::batch([open.panel, open.catcher].map(|id| match bar.surfaces.contains_key(&id) {
        true => Task::done(Message::RemoveWindow(id)),
        false => Task::none(),
    }))
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
    match bar.surfaces.get(&id) {
        Some(Surface::Tooltip(Tooltip::Clock)) => return clock_tooltip(bar),
        Some(Surface::Tooltip(Tooltip::Meeting)) => return meeting_tooltip(bar),
        Some(Surface::TrayMenu(open)) => return tray_menu_view(bar, open),
        Some(Surface::Panel) => return panel::view(bar),
        Some(Surface::Calendar) => return agenda::view(bar),
        Some(Surface::MeetingAlerts) => return alert::view(bar),
        Some(Surface::Popups) => return popup::view(bar),
        Some(Surface::Catcher) => {
            return mouse_area(Space::new().width(Length::Fill).height(Length::Fill))
                .on_press(Message::ClosePanel)
                .on_right_press(Message::ClosePanel)
                .into();
        }
        None => {}
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
        button(text(bar.now.format("%a %d/%m").to_string().to_uppercase()).font(CONDENSED_BOLD).size(15))
            .on_press(Message::ToggleCalendar)
            .padding(0)
            .style(move |_, status| button::Style {
                text_color: match status {
                    button::Status::Hovered | button::Status::Pressed => theme.accent,
                    _ => theme.text_muted,
                },
                ..Default::default()
            }),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    let time = Stack::new()
        .push(container(time).padding(Padding::ZERO.bottom(5.0)))
        .push(shape(shape::Kind::Underline(2.0), theme.accent));
    // The hover area spans the full bar height so the tooltip is easy to trigger.
    let clock = mouse_area(container(time).padding([0, 18]).center_y(Length::Fill))
        .on_enter(Message::ShowTooltip(Tooltip::Clock))
        .on_exit(Message::HideTooltip);

    let content = container(
        row![
            container(window).width(Length::FillPortion(1)).clip(true),
            container(row![clock].push(meeting_pill(bar)).spacing(6).align_y(Alignment::Center))
                .center_x(Length::FillPortion(1)),
            container(tray_view(bar, id)).align_right(Length::FillPortion(1)),
            // Hovering the right edge opens the side panel, like ii.
            mouse_area(Space::new().width(f32::from(BAR_PADDING)).height(Length::Fill)).on_enter(Message::HoverPanel),
        ]
        .height(Length::Fill)
        .align_y(Alignment::Center),
    )
    .height(Length::Fill);

    container(column![content, line(theme.bar_border)])
        .style(move |_| container::background(theme.bar_background))
        .into()
}

/// Next meeting beside the clock, or `SIGN IN` when the calendar token expired.
fn meeting_pill(bar: &Bar) -> Option<Element<'_, Message>> {
    let theme = bar.theme;
    let pill = meetings::pill(bar.meetings_status, &bar.meetings, bar.now.naive_local())?;
    let content = row![
        container(shape(shape::Kind::Diamond, pill.urgency.color())).width(14).height(14),
        icon_sized("calendar_today", 15.0),
        text(pill.label).font(CONDENSED_BOLD).size(16),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let expired = pill.urgency == meetings::Urgency::Expired;
    let pill = button(container(content).padding([0, 12]).center_y(30))
        .padding(0)
        .style(move |_, _| button::Style {
            background: Some(PILL_BACKGROUND.into()),
            text_color: theme.text,
            border: Border { color: theme.border, width: 1.0, radius: 0.0.into() },
            ..Default::default()
        })
        .on_press_maybe(expired.then_some(Message::SignInMeetings));
    if expired {
        return Some(pill.into());
    }
    Some(mouse_area(pill).on_enter(Message::ShowTooltip(Tooltip::Meeting)).on_exit(Message::HideTooltip).into())
}

/// Title and `10:00 – 10:45 · IN 4 MIN` of the meeting shown in the pill.
fn meeting_tooltip(bar: &Bar) -> Element<'_, Message> {
    let theme = bar.theme;
    let now = bar.now.naive_local();
    let content: Element<'_, Message> = match meetings::next_meeting(&bar.meetings, now) {
        Some(meeting) => column![
            text(&meeting.title).font(BODY_BOLD).size(13).wrapping(text::Wrapping::None),
            text(meetings::tooltip(meeting, now)).font(CONDENSED).size(13).color(theme.accent),
        ]
        .spacing(2)
        .into(),
        None => Space::new().into(),
    };
    container(content)
        .padding([8, 12])
        .width(Length::Fill)
        .height(Length::Fill)
        .clip(true)
        .style(move |_| panel_style(theme))
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
            .push_under(shape(shape::Kind::BRACKETS, theme.bracket));
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
    icon_sized(name, 16.0)
}

fn icon_sized(name: &str, size: f32) -> Element<'_, Message> {
    text(name)
        .font(ICON_FONT)
        .size(size)
        .shaping(text::Shaping::Advanced)
        .into()
}

fn style(bar: &Bar, _: &iced::Theme) -> iced::theme::Style {
    iced::theme::Style {
        background_color: Color::TRANSPARENT,
        text_color: bar.theme.text,
    }
}
