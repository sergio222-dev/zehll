mod agenda;
mod alert;
mod apps;
mod audio;
mod bluetooth;
mod bluetooth_agent;
mod brightness;
mod cache;
mod calc;
mod clipboard;
mod calendar;
mod cava;
mod dbus;
mod emoji;
mod files;
mod flame;
mod flashbang;
mod fuzzy;
mod gallery;
mod hyprland;
mod idle;
mod keybinds;
mod launcher;
mod media;
mod meetings;
mod mpris;
mod menu;
mod mixer;
mod network;
mod nightlight;
mod notifications;
mod osd;
mod panel;
mod pomodoro;
mod popup;
mod shape;
mod shortcuts;
mod snip;
mod state;
mod subpanel;
mod sysmon;
mod system;
mod theme;
mod todo;
mod trail;
mod tray;
mod usage;
mod wallhaven;
mod wallpaper;
mod workspaces;

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
// Pause in typing before the Files page runs fd.
const FILES_DEBOUNCE: Duration = Duration::from_millis(150);
/// Wallhaven is searched once typing stops for this long.
const WALLHAVEN_DEBOUNCE: Duration = Duration::from_millis(600);
// Wait before opening the bar of a monitor plugged in while Zehll runs (see `set_monitors`).
const NEW_MONITOR_DELAY: Duration = Duration::from_millis(500);
// How often the system indicator reads /proc.
const USAGE_INTERVAL: Duration = Duration::from_secs(2);
const PILL_BACKGROUND: Color = Color::from_rgb8(0x0d, 0x14, 0x30);
// Time for closing popovers to leave the screen before the snip freezes it.
const SNIP_SETTLE: Duration = Duration::from_millis(150);

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
// Two 15 px rows with 10 px padding; long titles are clipped.
const MEETING_TOOLTIP_SIZE: (u32, u32) = (360, 64);
const MEETING_TIME: Color = Color::from_rgb8(0xc9, 0xd2, 0xe8);

pub fn main() -> Result<(), iced_layershell::Error> {
    daemon(Bar::new, namespace, update, view)
        .style(style)
        .subscription(subscription)
        .settings(Settings {
            // No surface at start: Zehll opens one bar per Hyprland monitor (see `bar_settings`),
            // since an `AllScreens` surface does not say which monitor it is on.
            layer_settings: LayerShellSettings { start_mode: StartMode::Background, ..Default::default() },
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
    // Bar surfaces and the monitor each is on.
    bars: HashMap<Id, String>,
    monitors: Vec<hyprland::Monitor>,
    // Workspace indicator of each monitor, by name.
    workspaces: HashMap<String, workspaces::Indicator>,
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
    // Active MPRIS player; the media button is hidden without one.
    media: Option<mpris::Player>,
    // Player popover and its click catcher.
    media_popover: Option<OpenPanel>,
    usage: usage::Usage,
    // System monitor sub-panel and its click catcher.
    system_popover: Option<OpenPanel>,
    // Cover art URL last requested and its image once loaded.
    media_art_url: String,
    media_art: Option<image::Handle>,
    // Position in seconds while the seek slider is being dragged.
    media_seek: Option<f32>,
    // Latest cava frame, drawn behind the player popover.
    media_spectrum: Vec<f32>,
    // Flame burst around the media button: when playback last started or stopped, and which.
    media_flare: Option<(Instant, bool)>,
    launcher: launcher::Launcher,
    // Launcher surface and its click catcher.
    launcher_surface: Option<OpenPanel>,
    // Snip selector surfaces, one per monitor, and the frame each froze.
    snips: HashMap<Id, snip::Shot>,
    // The monitors are being captured; the selector opens once they are.
    snip_capturing: bool,
    snip_save: bool,
    wallpaper: wallpaper::State,
    // The current wallpaper scaled for each monitor, by monitor name.
    wallpaper_scaled: HashMap<String, wallpaper::Scaled>,
    // Each monitor's wallpaper layer, by monitor name.
    wallpaper_layers: HashMap<String, wallpaper::Layer>,
    // A wallpaper is being scaled; a monitor without its size waits for it instead of asking again.
    wallpaper_preparing: bool,
    // Volume indicator shown when the output's volume or mute changes.
    osd: osd::Osd,
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
    Media,
    System,
    Launcher,
    Snip,
    // A monitor's wallpaper, by monitor name.
    Wallpaper(String),
    MeetingAlerts,
    Popups,
    Osd,
    // Transparent full-screen layer below the panel; clicking it closes the panel.
    Catcher,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tooltip {
    Clock,
    Meeting,
    Media,
    System,
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
            bars: HashMap::new(),
            monitors: Vec::new(),
            workspaces: HashMap::new(),
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
            media: None,
            media_popover: None,
            usage: usage::Usage::default(),
            system_popover: None,
            media_art_url: String::new(),
            media_art: None,
            media_seek: None,
            media_spectrum: Vec::new(),
            media_flare: None,
            launcher: launcher::Launcher::load(),
            launcher_surface: None,
            snips: HashMap::new(),
            snip_capturing: false,
            snip_save: state.snip_save,
            wallpaper: wallpaper::load(),
            wallpaper_scaled: HashMap::new(),
            wallpaper_layers: HashMap::new(),
            wallpaper_preparing: false,
            osd: osd::Osd::default(),
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
        // The last wallpaper comes back (scaled sizes are cached) without recoloring again.
        let mut bar = bar;
        let restore = match bar.wallpaper.path.clone() {
            Some(path) => Task::batch([restore, prepare_wallpaper(&mut bar, path, false)]),
            None => restore,
        };
        // Loaded now too, so the first opening of the launcher already lists them.
        (bar, Task::batch([restore, load_apps()]))
    }

    fn save_state(&self) {
        state::State {
            keep_awake: self.keep_awake.is_some(),
            do_not_disturb: self.do_not_disturb,
            night_temperature: self.night_temperature,
            night_automatic: self.night_automatic,
            content_adjustment: self.content_adjustment,
            snip_save: self.snip_save,
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
    // Checks whether the volume indicator is due to hide.
    OsdTick,
    HideOsd,
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
    MediaPlayer(Option<mpris::Player>),
    MediaArt(String, Option<image::Handle>),
    ToggleMedia,
    Media(mpris::Command),
    DragMediaSeek(f32),
    MediaSpectrum(Vec<f32>),
    // Redraws the live wave of the player popover and the flame burst of the media button.
    MediaFrame,
    ReleaseMediaSeek,
    WindowOpened(Id),
    Monitors(Vec<hyprland::Monitor>),
    // Opens the bar of a monitor unless it is gone or already has one.
    OpenBar(String),
    // Redraws the workspace indicators while one is moving.
    WorkspaceFrame,
    FocusWorkspace(i64),
    UsageTick,
    UsageDetails((Vec<usage::Disk>, Option<usage::Gpu>)),
    ToggleSystem,
    ToggleLauncher,
    LauncherQuery(String),
    LauncherNav(launcher::Nav),
    LauncherActivate(usize),
    LauncherApps(Vec<apps::App>),
    // qalc's answer and the search it was computed for.
    CalcAnswer(String, Option<calc::Answer>),
    // Rows to scroll and the list length.
    LauncherScroll(i32, usize),
    FilesRecent(Vec<files::Entry>),
    // A search whose debounce ended; it runs only if it is still the one shown.
    FilesSearch(String),
    // fd's results and the search they were computed for.
    FilesFound(String, Vec<files::Entry>),
    FileDetails(String, Option<files::Details>),
    // A clipboard history read and the request number it answers.
    ClipboardList(u64, Vec<clipboard::Entry>),
    ClipboardDecoded(u64, Option<clipboard::Decoded>),
    ClipboardDelete(clipboard::Entry),
    ClipboardClear,
    ClipboardPinnedOnly,
    ToggleSnip,
    SnipCaptured(Vec<snip::Shot>),
    // The selection on a snip surface, and whether it goes to the editor.
    Snip(Id, snip::Rect, bool),
    CancelSnip,
    ToggleSnipSave,
    // A wallpaper and the index of its accent among matugen's source colors.
    ApplyWallpaper(std::path::PathBuf, usize),
    GalleryList(Vec<gallery::Entry>),
    // An image and its thumbnail, `None` when it cannot be made.
    GalleryThumb(std::path::PathBuf, Option<std::path::PathBuf>),
    GalleryColors(std::path::PathBuf, Vec<String>),
    // A tile, by its position among the tiles shown: selects it, or applies it when selected.
    GalleryClick(usize),
    GalleryScroll(i32),
    /// A click on the emoji at this index of the shown ones: picks it.
    EmojiClick(usize),
    EmojiScroll(i32),
    GalleryAccent(usize),
    OpenWallpaperFolder,
    // Opens the launcher on a page, or closes it when it is open there.
    ToggleLauncherPage(launcher::Page),
    // Applies a random wallpaper, once the folder is listed.
    RandomWallpaper,
    RandomWallpaperFrom(Vec<gallery::Entry>),
    // The Wallhaven page's buttons.
    WallhavenKey,
    WallhavenCategory(usize),
    WallhavenPurity(usize),
    WallhavenSorting,
    WallhavenOrder,
    WallhavenReload,
    // A result, by its position: selects it, applies it when selected, retries a failed thumbnail.
    WallhavenClick(usize),
    WallhavenScroll(i32),
    // The search after typing stopped.
    WallhavenSearch(String),
    WallhavenResults(u64, Result<wallhaven::Results, String>),
    WallhavenThumb(String, Option<std::path::PathBuf>),
    // A downloaded wallpaper: its id, and its file or why it failed.
    WallhavenFetched(String, Result<std::path::PathBuf, String>),
    // The wallpaper scaled, whether to recolor the desktop, and its sizes (`None` if unreadable).
    WallpaperPrepared(std::path::PathBuf, bool, Option<Vec<wallpaper::Scaled>>),
    // A frame of a wallpaper sliding to its new place.
    WallpaperFrame,
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
    // Every key press, even those the search field captures (it takes Escape), so navigation
    // works while typing.
    let launcher_keys = match bar.launcher_surface {
        Some(_) => iced::event::listen_with(|event, _, _| match event {
            iced::Event::Keyboard(key) => launcher::nav_event(&key).map(Message::LauncherNav),
            _ => None,
        }),
        None => Subscription::none(),
    };
    let calendar_keys = match bar.calendar.is_some() || bar.media_popover.is_some() || bar.system_popover.is_some() {
        true => keyboard::listen().map(Message::Key),
        false => Subscription::none(),
    };
    // cava and the live wave run only while the player popover shows a playing player.
    let playing = bar.media_popover.is_some() && bar.media.as_ref().is_some_and(|player| player.playing);
    let media = match playing {
        true => Subscription::batch([
            Subscription::run(cava::events).map(Message::MediaSpectrum),
            match bar.media.as_ref().is_some_and(|player| player.length.is_none()) {
                true => time::every(Duration::from_millis(33)).map(|_| Message::MediaFrame),
                false => Subscription::none(),
            },
        ]),
        false => Subscription::none(),
    };
    let wallpaper_pan = match bar.wallpaper_layers.values().any(|layer| layer.pan.animating(Instant::now())) {
        true => time::every(Duration::from_millis(16)).map(|_| Message::WallpaperFrame),
        false => Subscription::none(),
    };
    let workspace_move = match bar.workspaces.values().any(|indicator| indicator.animating(Instant::now())) {
        true => time::every(Duration::from_millis(16)).map(|_| Message::WorkspaceFrame),
        false => Subscription::none(),
    };
    let flare = match bar.media_flare {
        Some(_) => time::every(Duration::from_millis(16)).map(|_| Message::MediaFrame),
        None => Subscription::none(),
    };
    let osd = match bar.osd.surface {
        Some(_) => time::every(Duration::from_millis(100)).map(|_| Message::OsdTick),
        None => Subscription::none(),
    };
    let panel = match bar.panel {
        Some(_) => Subscription::batch([
            mixer,
            scan_animation,
            keyboard::listen().map(Message::Key),
            Subscription::run(network::events).map(Message::Network),
            Subscription::run(bluetooth::events).map(Message::Bluetooth),
        ]),
        None => Subscription::none(),
    };
    Subscription::batch([
        time::every(Duration::from_secs(1)).map(|_| Message::Tick),
        time::every(MEETINGS_POLL).map(|_| Message::FetchMeetings),
        time::every(USAGE_INTERVAL).map(|_| Message::UsageTick),
        Subscription::run(hyprland::active_window_events).map(Message::ActiveWindow),
        Subscription::run(hyprland::monitor_events).map(Message::Monitors),
        Subscription::run(tray::events).map(Message::Tray),
        Subscription::run(shortcuts::events).map(|shortcut| match shortcut {
            shortcuts::Shortcut::PanelToggle => Message::TogglePanel,
            shortcuts::Shortcut::MeetingTest => Message::MeetingTest,
            shortcuts::Shortcut::LauncherToggle | shortcuts::Shortcut::LauncherTap => Message::ToggleLauncher,
            shortcuts::Shortcut::Screenshot => Message::ToggleSnip,
            shortcuts::Shortcut::Wallpapers => Message::ToggleLauncherPage(launcher::Page::Wallpapers),
            shortcuts::Shortcut::WallpaperRandom => Message::RandomWallpaper,
            shortcuts::Shortcut::Wallhaven => Message::ToggleLauncherPage(launcher::Page::Wallhaven),
            shortcuts::Shortcut::Emojis => Message::ToggleLauncherPage(launcher::Page::Emojis),
            shortcuts::Shortcut::Clipboard => Message::ToggleLauncherPage(launcher::Page::Clipboard),
            shortcuts::Shortcut::Keybinds => Message::ToggleLauncherPage(launcher::Page::Keybinds),
        }),
        Subscription::run(notifications::events).map(Message::Notifications),
        Subscription::run(bluetooth_agent::serve).map(|never| match never {}),
        // Always on: the volume indicator follows the output even with the panel closed.
        Subscription::run(audio::events).map(Message::Audio),
        Subscription::run(mpris::events).map(Message::MediaPlayer),
        window::close_events().map(Message::WindowClosed),
        window::open_events().map(Message::WindowOpened),
        launcher_keys,
        panel,
        calendar_keys,
        ring,
        media,
        flare,
        workspace_move,
        wallpaper_pan,
        osd,
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
            // The tooltips open under the clock, where the calendar and player popovers are.
            if bar.tooltip.is_some() || bar.calendar.is_some() || bar.media_popover.is_some() || bar.system_popover.is_some() {
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
                Tooltip::Media => media::TOOLTIP_SIZE,
                Tooltip::System => sysmon::TOOLTIP_SIZE,
            };
            // The clock sits right of the centered workspaces, so the tooltips hang under the
            // pointer, like the tray menus, instead of under the bar's center.
            let cursor = hyprland::cursor_x_on_focused_monitor().unwrap_or_default();
            let width = hyprland::focused_monitor_width().unwrap_or(size.0 as i32);
            let settings = NewLayerShellSettings {
                size: Some(size),
                layer: Layer::Overlay,
                anchor: Anchor::Top | Anchor::Left,
                exclusive_zone: Some(0),
                margin: Some((8, 0, 0, hang_under(cursor, size.0 as i32, width))),
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
        Message::Tray(tray::TrayEvent::Reset) => {
            bar.tray_items.clear();
            if bar.open_menu().is_some() {
                return close_tray_menu(bar);
            }
        }
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
            match bar.surfaces.remove(&id) {
                Some(Surface::Wallpaper(name)) => {
                    bar.wallpaper_layers.remove(&name);
                }
                // The reading stays until the surface is gone, so it closes showing it.
                Some(Surface::Osd) if bar.osd.surface.is_none() => bar.osd.shown = None,
                _ => {}
            }
            bar.bars.remove(&id);
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
            if bar.osd.surface == Some(id) {
                bar.osd.surface = None;
                bar.osd.shown = None;
            }
            if bar.panel.as_ref().is_some_and(|open| open.panel == id || open.catcher == id) {
                return close_panel(bar);
            }
            if bar.calendar.as_ref().is_some_and(|open| open.panel == id || open.catcher == id) {
                return close_calendar(bar);
            }
            if bar.media_popover.as_ref().is_some_and(|open| open.panel == id || open.catcher == id) {
                return close_media(bar);
            }
            if bar.system_popover.as_ref().is_some_and(|open| open.panel == id || open.catcher == id) {
                return close_system(bar);
            }
            // The compositor dropped one selector surface: the snip is over on every monitor.
            if bar.snips.remove(&id).is_some() {
                return close_snip(bar);
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
        // Sent by the click catcher, which the panel and the popovers use.
        Message::ClosePanel => {
            return Task::batch([close_panel(bar), close_calendar(bar), close_media(bar), close_system(bar), close_launcher(bar)]);
        }
        Message::Key(keyboard::Event::KeyPressed { key: keyboard::Key::Named(keyboard::key::Named::Escape), .. }) => {
            if bar.calendar.is_some() || bar.media_popover.is_some() || bar.system_popover.is_some() {
                return Task::batch([close_calendar(bar), close_media(bar), close_system(bar)]);
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
            if let Err(err) = system::spawn(program, args) {
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
        Message::MediaPlayer(player) => {
            let art_url = player.as_ref().map(|player| player.art_url.clone()).unwrap_or_default();
            // Burst when the shown player starts or stops, not when another player takes over.
            if let (Some(old), Some(new)) = (&bar.media, &player)
                && old.service == new.service
                && old.playing != new.playing
            {
                bar.media_flare = Some((Instant::now(), new.playing));
            }
            bar.media = player;
            let mut tasks = Vec::new();
            if art_url != bar.media_art_url {
                bar.media_art_url = art_url.clone();
                bar.media_art = None;
                if !art_url.is_empty() {
                    tasks.push(Task::perform(mpris::art(art_url.clone()), move |art| Message::MediaArt(art_url.clone(), art)));
                }
            }
            if bar.media.is_none() {
                tasks.push(close_media(bar));
                if matches!(bar.tooltip.and_then(|id| bar.surfaces.get(&id)), Some(Surface::Tooltip(Tooltip::Media))) {
                    tasks.push(Task::done(Message::HideTooltip));
                }
            }
            return Task::batch(tasks);
        }
        // A late download for a track that already changed is dropped.
        Message::MediaArt(url, art) => {
            if url == bar.media_art_url {
                bar.media_art = art;
            }
        }
        Message::ToggleMedia => {
            return if bar.media_popover.is_some() { close_media(bar) } else { open_media(bar) };
        }
        // The search field exists only once its surface is open, so it is focused then.
        Message::WindowOpened(id) => {
            if bar.launcher_surface.as_ref().is_some_and(|open| open.panel == id) {
                return iced::widget::operation::focus(launcher::input_id());
            }
        }
        Message::Monitors(monitors) => return set_monitors(bar, monitors),
        Message::OpenBar(name) => {
            if !bar.monitors.iter().any(|monitor| monitor.name == name) || bar.bars.values().any(|open| *open == name) {
                return Task::none();
            }
            let id = Id::unique();
            bar.bars.insert(id, name.clone());
            let wallpaper = open_wallpaper(bar, name.clone());
            return Task::batch([Task::done(Message::NewLayerShell { settings: bar_settings(name), id }), wallpaper]);
        }
        Message::UsageTick => {
            bar.usage.sample(Instant::now());
            if bar.system_popover.is_some() {
                return Task::perform(usage::details(), Message::UsageDetails);
            }
        }
        Message::UsageDetails((disks, gpu)) => bar.usage.set_details(disks, gpu),
        Message::ToggleSystem => {
            return if bar.system_popover.is_some() { close_system(bar) } else { open_system(bar) };
        }
        Message::FocusWorkspace(workspace) => {
            hyprland::focus_workspace(workspace);
        }
        Message::WorkspaceFrame => {
            let now = Instant::now();
            bar.workspaces.values_mut().for_each(|indicator| indicator.settle(now));
        }
        Message::ToggleLauncher => {
            return if bar.launcher_surface.is_some() { close_launcher(bar) } else { open_launcher(bar, launcher::Page::Home) };
        }
        Message::ToggleLauncherPage(page) => {
            if bar.launcher_surface.is_none() {
                return open_launcher(bar, page);
            }
            if bar.launcher.page() == page {
                return close_launcher(bar);
            }
            bar.launcher.reset();
            return Task::batch([enter_page(bar, page), resize_launcher(bar)]);
        }
        Message::RandomWallpaper => return Task::perform(gallery::list(), Message::RandomWallpaperFrom),
        Message::RandomWallpaperFrom(entries) => {
            let current = bar.wallpaper.path.clone();
            let pick = gallery::pick_other(entries.len(), |at| Some(&entries[at].path) == current.as_ref(), gallery::seed());
            if let Some(at) = pick {
                return Task::done(Message::ApplyWallpaper(entries[at].path.clone(), 0));
            }
        }
        Message::LauncherQuery(query) => return set_launcher_query(bar, query),
        Message::CalcAnswer(query, answer) => bar.launcher.set_answer(query, answer),
        Message::LauncherNav(nav) if bar.launcher.page() == launcher::Page::Wallpapers => return gallery_nav(bar, nav),
        Message::LauncherNav(nav) if bar.launcher.page() == launcher::Page::Wallhaven => return wallhaven_nav(bar, nav),
        Message::LauncherNav(nav) if bar.launcher.page() == launcher::Page::Emojis => return emoji_nav(bar, nav),
        Message::LauncherNav(nav) => {
            let len = launcher::items(&bar.launcher).len();
            match nav {
                launcher::Nav::Close => return close_launcher(bar),
                launcher::Nav::Back => {
                    if !bar.launcher.back() {
                        return close_launcher(bar);
                    }
                    // Escape takes the focus from the search field.
                    return Task::batch([resize_launcher(bar), focus_launcher()]);
                }
                launcher::Nav::Move(delta) => {
                    bar.launcher.move_selection(delta, len);
                    return Task::batch([load_preview(bar), focus_launcher()]);
                }
                launcher::Nav::Activate { secondary } if bar.launcher.page() == launcher::Page::Calculator => {
                    return calculator_enter(bar, secondary);
                }
                launcher::Nav::Activate { secondary } => return activate_launcher_item(bar, bar.launcher.selected, secondary),
                // Ctrl+1..9 counts the rows on screen.
                launcher::Nav::Pick(row) => return activate_launcher_item(bar, bar.launcher.first + row, false),
                // Only grids use these.
                launcher::Nav::Sideways(_) | launcher::Nav::Accent(_) | launcher::Nav::Random => return Task::none(),
            }
        }
        Message::LauncherActivate(index) => return activate_launcher_item(bar, index, false),
        Message::GalleryList(entries) => {
            let missing: Vec<gallery::Entry> = entries.iter().filter(|entry| !bar.launcher.gallery.thumbs.contains_key(&entry.path)).cloned().collect();
            bar.launcher.gallery.entries = entries;
            // The current wallpaper starts selected, if it is listed.
            let current = bar.wallpaper.path.as_ref().and_then(|path| bar.launcher.gallery_shown().iter().position(|&index| bar.launcher.gallery.entries[index].path == *path));
            bar.launcher.gallery_select(current.unwrap_or(0));
            let (thumbs, handle) = Task::run(gallery::thumbnails(missing), |(path, thumb)| Message::GalleryThumb(path, thumb)).abortable();
            bar.launcher.gallery.track(handle);
            // An empty listing may be a read error: never prune everything for it.
            let listed = bar.launcher.gallery.entries.clone();
            let prune = match listed.is_empty() {
                true => Task::none(),
                false => Task::future(tokio::task::spawn_blocking(move || gallery::prune_thumbs(&listed))).discard(),
            };
            return Task::batch([thumbs, gallery_colors(bar), prune]);
        }
        Message::GalleryThumb(path, Some(thumb)) => {
            bar.launcher.gallery.thumbs.insert(path, thumb);
        }
        Message::GalleryThumb(path, None) => eprintln!("zehll: cannot make a thumbnail of {}", path.display()),
        Message::GalleryColors(path, colors) => {
            bar.launcher.gallery.colors.insert(path, colors);
        }
        Message::GalleryClick(at) if at == bar.launcher.selected => return gallery_nav(bar, launcher::Nav::Activate { secondary: false }),
        Message::GalleryClick(at) => {
            bar.launcher.gallery_select(at);
            return gallery_colors(bar);
        }
        Message::GalleryScroll(rows) => bar.launcher.gallery_scroll(rows),
        Message::EmojiClick(index) => return pick_emoji(bar, index, false),
        Message::EmojiScroll(rows) => bar.launcher.emoji_scroll(rows),
        Message::GalleryAccent(index) => bar.launcher.gallery.accent = index,
        Message::WallhavenKey => {
            let browser = &mut bar.launcher.wallhaven;
            if browser.editing_key.is_some() {
                return wallhaven_nav(bar, launcher::Nav::Back);
            }
            browser.editing_key = Some(bar.launcher.query.clone());
            // The saved key fills the field, hidden, so Enter keeps it.
            let key = browser.filters.apikey.clone().unwrap_or_default();
            bar.launcher.set_query(key);
            return focus_launcher();
        }
        Message::WallhavenCategory(index) => {
            bar.launcher.wallhaven.filters.toggle_category(index);
            return wallhaven_filters_changed(bar);
        }
        Message::WallhavenPurity(index) => {
            bar.launcher.wallhaven.filters.toggle_purity(index);
            return wallhaven_filters_changed(bar);
        }
        Message::WallhavenSorting => {
            let filters = &mut bar.launcher.wallhaven.filters;
            filters.sorting = filters.sorting.next();
            return wallhaven_filters_changed(bar);
        }
        Message::WallhavenOrder => {
            let filters = &mut bar.launcher.wallhaven.filters;
            filters.descending = !filters.descending;
            return wallhaven_filters_changed(bar);
        }
        Message::WallhavenReload => return wallhaven_search(bar),
        Message::WallhavenClick(at) => {
            let Some(wallpaper) = bar.launcher.wallhaven.results.get(at).cloned() else {
                return Task::none();
            };
            if matches!(bar.launcher.wallhaven.thumbs.get(&wallpaper.id), Some(None)) {
                bar.launcher.wallhaven.thumbs.remove(&wallpaper.id);
                return wallhaven_thumbs(bar, vec![wallpaper]);
            }
            if at == bar.launcher.selected {
                return wallhaven_nav(bar, launcher::Nav::Activate { secondary: false });
            }
            bar.launcher.wallhaven_select(at);
        }
        Message::WallhavenScroll(rows) => {
            bar.launcher.wallhaven_scroll(rows);
            let last_shown = (bar.launcher.wallhaven.first_row + wallhaven::ROWS) * wallhaven::COLUMNS - 1;
            return wallhaven_more(bar, last_shown);
        }
        Message::WallhavenSearch(query) => {
            let browser = &bar.launcher.wallhaven;
            if query == bar.launcher.query && browser.editing_key.is_none() && bar.launcher.page() == launcher::Page::Wallhaven {
                return wallhaven_search(bar);
            }
        }
        Message::WallhavenResults(request, results) => {
            let new: Vec<wallhaven::Wallpaper> = results.as_ref().map(|results| results.wallpapers.clone()).unwrap_or_default();
            let first_page = results.as_ref().is_ok_and(|results| results.page <= 1);
            if !bar.launcher.wallhaven.accept(request, results) {
                return Task::none();
            }
            if first_page {
                bar.launcher.wallhaven_select(0);
            }
            return wallhaven_thumbs(bar, new);
        }
        Message::WallhavenThumb(id, thumb) => {
            bar.launcher.wallhaven.thumbs.insert(id, thumb);
        }
        Message::WallhavenFetched(id, result) => {
            let browser = &mut bar.launcher.wallhaven;
            if browser.applying.as_deref() == Some(id.as_str()) {
                browser.applying = None;
            }
            match result {
                Ok(path) => return Task::done(Message::ApplyWallpaper(path, 0)),
                Err(error) => browser.error = Some(format!("Download failed: {error}")),
            }
        }
        Message::OpenWallpaperFolder => {
            let folder = gallery::folder();
            let _ = std::fs::create_dir_all(&folder);
            if !hyprland::exec(&format!("xdg-open {}", files::shell_quote(&folder.to_string_lossy()))) {
                eprintln!("zehll: cannot open {}", folder.display());
            }
            return close_launcher(bar);
        }
        Message::LauncherApps(apps) => bar.launcher.apps = apps,
        Message::LauncherScroll(rows, len) => bar.launcher.scroll(rows, len),
        Message::FilesRecent(recent) => {
            bar.launcher.recent = recent;
            return load_preview(bar);
        }
        Message::FilesSearch(query) => {
            if query == bar.launcher.query && bar.launcher.page() == launcher::Page::Files {
                let home = bar.launcher.home.clone();
                return Task::perform(files::search(query.clone(), home), move |found| Message::FilesFound(query.clone(), found));
            }
        }
        Message::FilesFound(query, found) => {
            bar.launcher.set_found(query, found);
            return load_preview(bar);
        }
        Message::FileDetails(path, Some(details)) => bar.launcher.set_details(path, details),
        Message::FileDetails(_, None) => {}
        Message::ClipboardList(request, clips) => {
            bar.launcher.set_clips(request, clips);
            return load_preview(bar);
        }
        Message::ClipboardDecoded(id, Some(decoded)) => bar.launcher.set_decoded(id, decoded),
        Message::ClipboardDecoded(_, None) => {}
        Message::ClipboardDelete(entry) => {
            bar.launcher.remove_clip(entry.id);
            let request = bar.launcher.begin_clips();
            let reload = Task::perform(
                async move {
                    clipboard::delete(entry).await;
                    clipboard::list().await
                },
                move |clips| Message::ClipboardList(request, clips),
            );
            return Task::batch([reload, load_preview(bar), focus_launcher()]);
        }
        Message::ClipboardClear => {
            if !bar.launcher.confirm_clear() {
                return focus_launcher();
            }
            bar.launcher.clear_clips();
            let request = bar.launcher.begin_clips();
            let reload = Task::perform(
                async {
                    clipboard::wipe().await;
                    clipboard::list().await
                },
                move |clips| Message::ClipboardList(request, clips),
            );
            return Task::batch([reload, focus_launcher()]);
        }
        Message::ClipboardPinnedOnly => {
            bar.launcher.toggle_pinned_only();
            return Task::batch([load_preview(bar), focus_launcher()]);
        }
        Message::Media(command) => {
            if let Some(player) = bar.media.clone() {
                return Task::future(mpris::run(player, command)).discard();
            }
        }
        Message::MediaSpectrum(points) => bar.media_spectrum = points,
        Message::MediaFrame => {
            if bar.media_flare.is_some_and(|(started, _)| started.elapsed() >= flame::DURATION) {
                bar.media_flare = None;
            }
        }
        Message::DragMediaSeek(position) => {
            if bar.media.as_ref().is_some_and(|player| player.can_seek) {
                bar.media_seek = Some(position);
            }
        }
        // Seek once on release, like the volume sliders.
        Message::ReleaseMediaSeek => {
            if let (Some(position), Some(player)) = (bar.media_seek.take(), bar.media.clone()) {
                let command = mpris::Command::Seek(Duration::from_secs_f32(position));
                return Task::future(mpris::run(player, command)).discard();
            }
        }
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
        Message::Audio(audio) => {
            let task = show_osd(bar, &audio.output);
            bar.audio = Some(audio);
            return task;
        }
        Message::OsdTick => {
            if bar.osd.shown.as_ref().is_some_and(|(_, hide_at)| Instant::now() >= *hide_at) {
                return hide_osd(bar);
            }
        }
        Message::HideOsd => return hide_osd(bar),
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
            let dropped = notifications::push(&mut bar.notifications, notification, notifications::MAX_HISTORY);
            let shown = bar.popups.len();
            bar.popups.retain(|popup| !dropped.contains(&popup.id));
            let closed = match (dropped.is_empty(), bar.notifications_handle.clone()) {
                (false, Some(handle)) => Task::future(handle.closed(dropped, notifications::UNDEFINED)).discard(),
                _ => Task::none(),
            };
            if !bar.do_not_disturb {
                bar.popups.retain(|popup| popup.id != id);
                bar.popups.insert(0, Popup { id, expires: timeout.map(|timeout| Instant::now() + timeout) });
                bar.popups.truncate(popup::MAX_VISIBLE);
                return Task::batch([closed, sync_popups(bar)]);
            }
            if bar.popups.len() != shown {
                return Task::batch([closed, sync_popups(bar)]);
            }
            return closed;
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
        Message::ToggleSnip => {
            if !bar.snips.is_empty() {
                return close_snip(bar);
            }
            if !bar.snip_capturing {
                return start_snip(bar);
            }
        }
        Message::SnipCaptured(shots) => return open_snip(bar, shots),
        Message::Snip(id, rect, edit) => {
            let Some(shot) = bar.snips.get(&id) else {
                return Task::none();
            };
            let frame = shot.handle.clone();
            let area = snip::dimensions(&frame).and_then(|size| snip::crop(rect, shot.monitor.scale, size));
            let action = if edit { snip::Action::Edit } else { snip::Action::Copy { save: bar.snip_save } };
            let close = close_snip(bar);
            return match area {
                Some(area) => Task::batch([close, Task::future(snip::deliver(frame, area, action)).discard()]),
                None => close,
            };
        }
        Message::CancelSnip => return close_snip(bar),
        Message::ToggleSnipSave => {
            bar.snip_save = !bar.snip_save;
            bar.save_state();
        }
        Message::ApplyWallpaper(path, accent) => {
            bar.wallpaper.path = Some(path.clone());
            bar.wallpaper.accent = accent;
            wallpaper::save(&bar.wallpaper);
            return prepare_wallpaper(bar, path, true);
        }
        Message::WallpaperPrepared(path, recolor, crops) => {
            bar.wallpaper_preparing = false;
            // A newer choice is already being prepared.
            if bar.wallpaper.path.as_ref() != Some(&path) {
                return Task::none();
            }
            let Some(scaled) = crops else {
                return Task::none();
            };
            bar.wallpaper_scaled = scaled.into_iter().map(|scaled| (scaled.monitor.clone(), scaled)).collect();
            // Layers already open take the new wallpaper; monitors whose bar is open get one.
            for (name, layer) in bar.wallpaper_layers.iter_mut() {
                let slot = bar.monitors.iter().find(|monitor| monitor.name == *name).and_then(|monitor| workspaces::slot(monitor.workspace));
                if let Some(scaled) = bar.wallpaper_scaled.get(name) {
                    layer.replace(scaled.clone(), slot.unwrap_or(0));
                }
            }
            let names: Vec<String> = bar.bars.values().cloned().collect();
            let show = Task::batch(names.into_iter().map(|name| open_wallpaper(bar, name)));
            return match recolor {
                true => show.chain(Task::future(wallpaper::recolor(path, bar.wallpaper.accent)).discard()),
                false => show,
            };
        }
        Message::WallpaperFrame => {}
        Message::PowerMenu => {
            if let Err(err) = system::spawn("wlogout", &[]) {
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
    if let Err(err) = system::spawn("xdg-open", &[link]) {
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

/// Shows the volume indicator when the output's volume or mute changed, or keeps it up longer.
/// Not while the side panel is open: its audio tile already shows the volume.
fn show_osd(bar: &mut Bar, output: &audio::Device) -> Task<Message> {
    let changed = osd::changed(bar.osd.last.as_ref(), output);
    bar.osd.last = Some(output.clone());
    if !changed || bar.panel.is_some() {
        return Task::none();
    }
    bar.osd.shown = Some((output.clone(), Instant::now() + osd::HOLD));
    if bar.osd.surface.is_some() {
        return Task::none();
    }
    let Some(output) = hyprland::focused_monitor() else {
        return Task::none();
    };
    let settings = NewLayerShellSettings {
        size: Some(osd::SIZE),
        layer: Layer::Overlay,
        anchor: Anchor::Top,
        exclusive_zone: Some(0),
        margin: Some((14, 0, 0, 0)),
        keyboard_interactivity: KeyboardInteractivity::None,
        output_option: OutputOption::OutputName(output),
        events_transparent: false,
        namespace: Some(String::from(osd::NAMESPACE)),
    };
    let id = Id::unique();
    bar.osd.surface = Some(id);
    bar.surfaces.insert(id, Surface::Osd);
    Task::done(Message::NewLayerShell { settings, id })
}

fn hide_osd(bar: &mut Bar) -> Task<Message> {
    match bar.osd.surface.take() {
        Some(id) => Task::done(Message::RemoveWindow(id)),
        None => Task::none(),
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
    let close_calendar = Task::batch([close_calendar(bar), close_launcher(bar)]);
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
/// Moves the workspace indicators and opens or closes bars as monitors come and go.
fn set_monitors(bar: &mut Bar, monitors: Vec<hyprland::Monitor>) -> Task<Message> {
    let now = Instant::now();
    bar.workspaces.retain(|name, _| monitors.iter().any(|monitor| monitor.name == *name));
    for monitor in &monitors {
        let Some(slot) = workspaces::slot(monitor.workspace) else {
            continue;
        };
        bar.workspaces
            .entry(monitor.name.clone())
            .and_modify(|indicator| indicator.move_to(slot, now))
            .or_insert_with(|| workspaces::Indicator::new(slot));
    }

    let mut tasks = Vec::new();
    let gone: Vec<Id> = bar
        .bars
        .iter()
        .filter(|(_, name)| !monitors.iter().any(|monitor| monitor.name == **name))
        .map(|(id, _)| *id)
        .collect();
    for id in gone {
        bar.bars.remove(&id);
        tasks.push(Task::done(Message::RemoveWindow(id)));
    }
    // At start every output is known; a monitor plugged in later reaches Hyprland's socket
    // before its wl_output name reaches layershellev, which would put the bar on the focused one.
    let starting = bar.bars.is_empty() && bar.monitors.is_empty();
    for monitor in monitors.iter().filter(|monitor| !bar.bars.values().any(|name| *name == monitor.name)) {
        let name = monitor.name.clone();
        tasks.push(match starting {
            true => Task::done(Message::OpenBar(name)),
            false => Task::perform(tokio::time::sleep(NEW_MONITOR_DELAY), move |()| Message::OpenBar(name.clone())),
        });
    }
    bar.monitors = monitors;
    // Each wallpaper slides to its monitor's workspace; an unplugged monitor's layer goes.
    let gone: Vec<String> = bar.wallpaper_layers.keys().filter(|name| !bar.monitors.iter().any(|monitor| monitor.name == **name)).cloned().collect();
    for name in gone {
        if let Some(layer) = bar.wallpaper_layers.remove(&name) {
            tasks.push(Task::done(Message::RemoveWindow(layer.id)));
        }
    }
    for monitor in &bar.monitors {
        if let (Some(layer), Some(slot)) = (bar.wallpaper_layers.get_mut(&monitor.name), workspaces::slot(monitor.workspace)) {
            layer.slide_to(slot, now);
        }
    }
    // A monitor plugged in after the wallpaper was scaled needs its own size.
    let missing = bar.monitors.iter().any(|monitor| !bar.wallpaper_scaled.contains_key(&monitor.name));
    if missing && !bar.wallpaper_scaled.is_empty() && !bar.wallpaper_preparing
        && let Some(path) = bar.wallpaper.path.clone()
    {
        tasks.push(prepare_wallpaper(bar, path, false));
    }
    Task::batch(tasks)
}

/// Scales `path` for every monitor in the background, then shows it.
fn prepare_wallpaper(bar: &mut Bar, path: std::path::PathBuf, recolor: bool) -> Task<Message> {
    bar.wallpaper_preparing = true;
    Task::perform(wallpaper::prepare(path.clone()), move |scaled| Message::WallpaperPrepared(path.clone(), recolor, scaled))
}

/// Opens the wallpaper layer of monitor `name`, once its wallpaper is scaled.
fn open_wallpaper(bar: &mut Bar, name: String) -> Task<Message> {
    let Some(scaled) = bar.wallpaper_scaled.get(&name).cloned() else {
        return Task::none();
    };
    if bar.wallpaper_layers.contains_key(&name) {
        return Task::none();
    }
    let slot = bar.monitors.iter().find(|monitor| monitor.name == name).and_then(|monitor| workspaces::slot(monitor.workspace)).unwrap_or(0);
    let id = Id::unique();
    bar.surfaces.insert(id, Surface::Wallpaper(name.clone()));
    bar.wallpaper_layers.insert(name.clone(), wallpaper::Layer::new(id, scaled, slot));
    Task::done(Message::NewLayerShell { settings: wallpaper::layer_settings(name), id })
}

fn bar_settings(output: String) -> NewLayerShellSettings {
    NewLayerShellSettings {
        size: Some((0, BAR_HEIGHT)),
        layer: Layer::Top,
        anchor: Anchor::Top | Anchor::Left | Anchor::Right,
        exclusive_zone: Some(BAR_HEIGHT as i32),
        margin: None,
        keyboard_interactivity: KeyboardInteractivity::OnDemand,
        output_option: OutputOption::OutputName(output),
        events_transparent: false,
        namespace: Some(namespace()),
    }
}

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
    Task::batch([close_panel(bar), close_media(bar), close_system(bar), Task::done(Message::HideTooltip), task, fetch])
}

/// Opens the player popover under the media button, closing the panel, the calendar and the tooltip.
fn open_media(bar: &mut Bar) -> Task<Message> {
    let Some(output) = hyprland::focused_monitor() else {
        return Task::none();
    };
    let popover = NewLayerShellSettings {
        size: Some((media::WIDTH, media::HEIGHT)),
        layer: Layer::Overlay,
        // Anchored only to the top, so it is centered under the media button and the clock.
        anchor: Anchor::Top,
        exclusive_zone: Some(0),
        margin: Some((14, 0, 0, 0)),
        keyboard_interactivity: KeyboardInteractivity::OnDemand,
        output_option: OutputOption::OutputName(output.clone()),
        events_transparent: false,
        namespace: Some(String::from("zehll-media")),
    };
    let open = OpenPanel { panel: Id::unique(), catcher: Id::unique() };
    bar.surfaces.insert(open.catcher, Surface::Catcher);
    bar.surfaces.insert(open.panel, Surface::Media);
    let task = Task::done(Message::NewLayerShell {
        settings: catcher_settings(output, "zehll-media-catcher"),
        id: open.catcher,
    })
    .chain(Task::done(Message::NewLayerShell { settings: popover, id: open.panel }));
    bar.media_popover = Some(open);
    Task::batch([close_panel(bar), close_calendar(bar), close_system(bar), Task::done(Message::HideTooltip), task])
}

/// Opens the system monitor under the bar, closing the panel, the other popovers and the tooltip.
fn open_system(bar: &mut Bar) -> Task<Message> {
    let Some(output) = hyprland::focused_monitor() else {
        return Task::none();
    };
    let popover = NewLayerShellSettings {
        size: Some((sysmon::WIDTH, sysmon::HEIGHT)),
        layer: Layer::Overlay,
        // Anchored only to the top, so it is centered under the bar like the player.
        anchor: Anchor::Top,
        exclusive_zone: Some(0),
        margin: Some((14, 0, 0, 0)),
        keyboard_interactivity: KeyboardInteractivity::OnDemand,
        output_option: OutputOption::OutputName(output.clone()),
        events_transparent: false,
        namespace: Some(String::from("zehll-system")),
    };
    let open = OpenPanel { panel: Id::unique(), catcher: Id::unique() };
    bar.surfaces.insert(open.catcher, Surface::Catcher);
    bar.surfaces.insert(open.panel, Surface::System);
    let task = Task::done(Message::NewLayerShell {
        settings: catcher_settings(output, "zehll-system-catcher"),
        id: open.catcher,
    })
    .chain(Task::done(Message::NewLayerShell { settings: popover, id: open.panel }));
    bar.system_popover = Some(open);
    bar.uptime = system::uptime().unwrap_or_default();
    Task::batch([
        close_panel(bar),
        close_calendar(bar),
        close_media(bar),
        Task::done(Message::HideTooltip),
        task,
        Task::perform(usage::details(), Message::UsageDetails),
    ])
}

fn close_system(bar: &mut Bar) -> Task<Message> {
    let Some(open) = bar.system_popover.take() else {
        return Task::none();
    };
    Task::batch([open.panel, open.catcher].map(|id| match bar.surfaces.contains_key(&id) {
        true => Task::done(Message::RemoveWindow(id)),
        false => Task::none(),
    }))
}

/// Opens the launcher centered on the focused monitor, on Home, closing every popover.
fn open_launcher(bar: &mut Bar, page: launcher::Page) -> Task<Message> {
    let Some(output) = hyprland::focused_monitor() else {
        return Task::none();
    };
    bar.launcher.reset();
    bar.launcher.now = Local::now().timestamp();
    bar.launcher.running = hyprland::clients().into_iter().map(|(_, class)| class).collect();
    bar.launcher.previous_window = hyprland::active_address();
    // A page opened by a shortcut sizes the surface from the start.
    let page_task = match page {
        launcher::Page::Home => Task::none(),
        page => enter_page(bar, page),
    };
    let surface = NewLayerShellSettings {
        size: Some(launcher::size(page)),
        layer: Layer::Overlay,
        // No anchor centers it on the output.
        anchor: Anchor::empty(),
        exclusive_zone: Some(0),
        margin: None,
        keyboard_interactivity: KeyboardInteractivity::Exclusive,
        output_option: OutputOption::OutputName(output.clone()),
        events_transparent: false,
        namespace: Some(String::from("zehll-launcher")),
    };
    let open = OpenPanel { panel: Id::unique(), catcher: Id::unique() };
    bar.surfaces.insert(open.catcher, Surface::Catcher);
    bar.surfaces.insert(open.panel, Surface::Launcher);
    let task = Task::done(Message::NewLayerShell {
        settings: catcher_settings(output, "zehll-launcher-catcher"),
        id: open.catcher,
    })
    .chain(Task::done(Message::NewLayerShell { settings: surface, id: open.panel }))
    .chain(page_task);
    bar.launcher_surface = Some(open);
    Task::batch([close_panel(bar), close_calendar(bar), close_media(bar), close_system(bar), Task::done(Message::HideTooltip), task, load_apps()])
}

/// Closes every popover, then freezes the monitors; the selector opens with the frames.
fn start_snip(bar: &mut Bar) -> Task<Message> {
    let popover_open = bar.panel.is_some()
        || bar.calendar.is_some()
        || bar.media_popover.is_some()
        || bar.system_popover.is_some()
        || bar.launcher_surface.is_some()
        || bar.tray_menu.is_some()
        || bar.tooltip.is_some();
    let close = Task::batch([
        close_panel(bar),
        close_calendar(bar),
        close_media(bar),
        close_system(bar),
        close_launcher(bar),
        close_tray_menu(bar),
        Task::done(Message::HideTooltip),
    ]);
    bar.snip_capturing = true;
    // A closing popover would still be in the frame without a moment to go away.
    let settle = if popover_open { SNIP_SETTLE } else { Duration::ZERO };
    let capture = Task::perform(
        async move {
            tokio::time::sleep(settle).await;
            snip::capture().await
        },
        Message::SnipCaptured,
    );
    Task::batch([close, capture])
}

/// One full-screen selector per frozen monitor; only the focused one takes the keyboard.
fn open_snip(bar: &mut Bar, shots: Vec<snip::Shot>) -> Task<Message> {
    bar.snip_capturing = false;
    if shots.is_empty() {
        return Task::none();
    }
    let tasks = shots.into_iter().map(|shot| {
        let settings = NewLayerShellSettings {
            size: Some((0, 0)),
            layer: Layer::Overlay,
            anchor: Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right,
            exclusive_zone: Some(-1),
            margin: None,
            keyboard_interactivity: match shot.monitor.focused {
                true => KeyboardInteractivity::Exclusive,
                false => KeyboardInteractivity::None,
            },
            output_option: OutputOption::OutputName(shot.monitor.name.clone()),
            events_transparent: false,
            namespace: Some(String::from("zehll-snip")),
        };
        let id = Id::unique();
        bar.surfaces.insert(id, Surface::Snip);
        bar.snips.insert(id, shot);
        Task::done(Message::NewLayerShell { settings, id })
    });
    Task::batch(tasks.collect::<Vec<_>>())
}

fn close_snip(bar: &mut Bar) -> Task<Message> {
    let ids: Vec<Id> = bar.snips.drain().map(|(id, _)| id).collect();
    Task::batch(ids.into_iter().map(|id| match bar.surfaces.contains_key(&id) {
        true => Task::done(Message::RemoveWindow(id)),
        false => Task::none(),
    }))
}

/// Reads the installed apps off the UI thread; the launcher keeps the previous list meanwhile.
fn load_apps() -> Task<Message> {
    Task::perform(async { tokio::task::spawn_blocking(apps::load).await.unwrap_or_default() }, Message::LauncherApps)
}

fn close_launcher(bar: &mut Bar) -> Task<Message> {
    let Some(open) = bar.launcher_surface.take() else {
        return Task::none();
    };
    // Thumbnails still missing are made again the next time a page needs them.
    bar.launcher.gallery.stop();
    bar.launcher.wallhaven.stop();
    Task::batch([open.panel, open.catcher].map(|id| match bar.surfaces.contains_key(&id) {
        true => Task::done(Message::RemoveWindow(id)),
        false => Task::none(),
    }))
}

/// Enters `page` from Home and loads what it shows. The caller fits the surface to it.
fn enter_page(bar: &mut Bar, page: launcher::Page) -> Task<Message> {
    if page == launcher::Page::Keybinds {
        bar.launcher.keybinds = keybinds::load();
    }
    bar.launcher.open(page);
    match page {
        launcher::Page::Files => Task::batch([Task::perform(files::recent(), Message::FilesRecent), focus_launcher()]),
        launcher::Page::Clipboard => {
            let request = bar.launcher.begin_clips();
            Task::batch([Task::perform(clipboard::list(), move |clips| Message::ClipboardList(request, clips)), focus_launcher()])
        }
        launcher::Page::Wallpapers => Task::batch([Task::perform(gallery::list(), Message::GalleryList), focus_launcher()]),
        // Results stay between openings; the first opening searches. Closing the launcher stopped
        // the thumbnail downloads, so the missing ones start again.
        launcher::Page::Wallhaven => {
            let browser = &bar.launcher.wallhaven;
            let missing: Vec<wallhaven::Wallpaper> =
                browser.results.iter().filter(|wallpaper| !browser.thumbs.contains_key(&wallpaper.id)).cloned().collect();
            let search = match (browser.results.is_empty() && !browser.loading, missing.is_empty()) {
                (true, _) => wallhaven_search(bar),
                (false, false) => wallhaven_thumbs(bar, missing),
                (false, true) => Task::none(),
            };
            bar.launcher.wallhaven_select(0);
            Task::batch([search, focus_launcher()])
        }
        _ => focus_launcher(),
    }
}

/// Fits the launcher's surface to its page: the Wallpapers grid is larger.
fn resize_launcher(bar: &Bar) -> Task<Message> {
    match &bar.launcher_surface {
        Some(open) => Task::done(Message::SizeChange { id: open.panel, size: launcher::size(bar.launcher.page()) }),
        None => Task::none(),
    }
}

/// Keys on the Wallpapers page: arrows move among the tiles, Alt+arrows pick the accent,
/// Enter applies the selected wallpaper and Ctrl+R a random one.
fn gallery_nav(bar: &mut Bar, nav: launcher::Nav) -> Task<Message> {
    match nav {
        launcher::Nav::Close => return close_launcher(bar),
        launcher::Nav::Back => {
            if !bar.launcher.back() {
                return close_launcher(bar);
            }
            return Task::batch([resize_launcher(bar), focus_launcher()]);
        }
        launcher::Nav::Move(rows) => bar.launcher.gallery_move(rows * gallery::COLUMNS as i32),
        launcher::Nav::Sideways(delta) => bar.launcher.gallery_move(delta),
        launcher::Nav::Accent(delta) => {
            let count = bar.launcher.gallery_selected().and_then(|entry| bar.launcher.gallery.colors.get(&entry.path)).map_or(0, Vec::len);
            bar.launcher.gallery.accent = gallery::cycle(bar.launcher.gallery.accent, delta, count);
            return Task::none();
        }
        launcher::Nav::Random => {
            let shown = bar.launcher.gallery_shown();
            let others: Vec<usize> = (0..shown.len()).filter(|&at| Some(&bar.launcher.gallery.entries[shown[at]].path) != bar.wallpaper.path.as_ref()).collect();
            if others.is_empty() {
                return Task::none();
            }
            let Some(at) = gallery::pick_other(others.len(), |_| false, gallery::seed()) else {
                return Task::none();
            };
            bar.launcher.gallery_select(others[at]);
            return gallery_nav(bar, launcher::Nav::Activate { secondary: false });
        }
        launcher::Nav::Activate { .. } => {
            let Some(entry) = bar.launcher.gallery_selected() else {
                return Task::none();
            };
            return Task::done(Message::ApplyWallpaper(entry.path.clone(), bar.launcher.gallery.accent));
        }
        launcher::Nav::Pick(_) => return Task::none(),
    }
    gallery_colors(bar)
}

/// Keys on the Emojis page: arrows move in the grid, Alt+Left/Right change the skin tone,
/// Enter types the emoji and Ctrl+Enter copies it.
fn emoji_nav(bar: &mut Bar, nav: launcher::Nav) -> Task<Message> {
    match nav {
        launcher::Nav::Close => return close_launcher(bar),
        launcher::Nav::Back => {
            if !bar.launcher.back() {
                return close_launcher(bar);
            }
            return Task::batch([resize_launcher(bar), focus_launcher()]);
        }
        launcher::Nav::Move(rows) => bar.launcher.emoji_vertical(rows),
        launcher::Nav::Sideways(delta) => bar.launcher.emoji_sideways(delta),
        launcher::Nav::Accent(delta) => {
            bar.launcher.emoji.tone = emoji::cycle_tone(bar.launcher.emoji.tone, delta);
            bar.launcher.emoji.save();
        }
        launcher::Nav::Activate { secondary } => return pick_emoji(bar, bar.launcher.selected, secondary),
        launcher::Nav::Pick(_) | launcher::Nav::Random => {}
    }
    focus_launcher()
}

/// Counts a pick of the emoji at `index`; Enter closes the launcher and types it into the window
/// below (and copies it), Ctrl+Enter only copies it. MOST USED is laid out again on the next
/// opening or search, so the grid doesn't shift under the selection.
fn pick_emoji(bar: &mut Bar, index: usize, copy_only: bool) -> Task<Message> {
    let (Some(text), Some(base)) = (bar.launcher.emoji.text(index), bar.launcher.emoji.shown.get(index).copied()) else {
        return focus_launcher();
    };
    emoji::bump(&mut bar.launcher.emoji.counts, base);
    bar.launcher.emoji.save();
    match copy_only {
        true => Task::batch([Task::future(clipboard::copy(text.to_owned())).discard(), focus_launcher()]),
        false => {
            let window = bar.launcher.previous_window.clone();
            Task::batch([close_launcher(bar), Task::future(emoji::paste(text.to_owned(), window)).discard()])
        }
    }
}

/// Keys on the Wallhaven page: like the Wallpapers grid, Ctrl+Enter also keeps the download in
/// the wallpaper folder. While the API key is typed, Enter saves it and Shift+Escape cancels.
fn wallhaven_nav(bar: &mut Bar, nav: launcher::Nav) -> Task<Message> {
    if let Some(search) = bar.launcher.wallhaven.editing_key.clone() {
        let entered = bar.launcher.query.trim().to_owned();
        bar.launcher.wallhaven.editing_key = None;
        bar.launcher.set_query(search);
        return match nav {
            launcher::Nav::Activate { .. } => {
                let filters = &mut bar.launcher.wallhaven.filters;
                filters.apikey = Some(entered).filter(|key| !key.is_empty());
                if !filters.has_key() {
                    filters.purity = [true, false, false];
                }
                Task::batch([wallhaven_filters_changed(bar), focus_launcher()])
            }
            launcher::Nav::Close => close_launcher(bar),
            _ => focus_launcher(),
        };
    }
    match nav {
        launcher::Nav::Close => return close_launcher(bar),
        launcher::Nav::Back => {
            if !bar.launcher.back() {
                return close_launcher(bar);
            }
            return Task::batch([resize_launcher(bar), focus_launcher()]);
        }
        launcher::Nav::Move(rows) => bar.launcher.wallhaven_move(rows * wallhaven::COLUMNS as i32),
        launcher::Nav::Sideways(delta) => bar.launcher.wallhaven_move(delta),
        launcher::Nav::Activate { secondary } => {
            let browser = &mut bar.launcher.wallhaven;
            let Some(wallpaper) = browser.results.get(bar.launcher.selected).cloned() else {
                return Task::none();
            };
            browser.applying = Some(wallpaper.id.clone());
            browser.error = None;
            let id = wallpaper.id.clone();
            return Task::perform(wallhaven::fetch(wallpaper, gallery::folder(), secondary), move |result| Message::WallhavenFetched(id.clone(), result));
        }
        launcher::Nav::Accent(_) | launcher::Nav::Random | launcher::Nav::Pick(_) => return Task::none(),
    }
    let selected = bar.launcher.selected;
    wallhaven_more(bar, selected)
}

/// Downloads the thumbnails of `wallpapers`, stopped by the next search or by closing the launcher.
fn wallhaven_thumbs(bar: &mut Bar, wallpapers: Vec<wallhaven::Wallpaper>) -> Task<Message> {
    let (task, handle) = Task::run(wallhaven::thumbnails(wallpapers), |(id, thumb)| Message::WallhavenThumb(id, thumb)).abortable();
    bar.launcher.wallhaven.track(handle);
    task
}

/// A new first page for the search and filters.
fn wallhaven_search(bar: &mut Bar) -> Task<Message> {
    let request = bar.launcher.wallhaven.begin();
    let url = wallhaven::search_url(&bar.launcher.query, &bar.launcher.wallhaven.filters, 1);
    Task::perform(wallhaven::search(url), move |results| Message::WallhavenResults(request, results))
}

/// The next page, when `index` is close to the end of the results.
fn wallhaven_more(bar: &mut Bar, index: usize) -> Task<Message> {
    if !bar.launcher.wallhaven.near_end(index) {
        return Task::none();
    }
    let Some((request, page)) = bar.launcher.wallhaven.begin_more() else {
        return Task::none();
    };
    let url = wallhaven::search_url(&bar.launcher.query, &bar.launcher.wallhaven.filters, page);
    Task::perform(wallhaven::search(url), move |results| Message::WallhavenResults(request, results))
}

/// Saves the filters and searches again with them.
fn wallhaven_filters_changed(bar: &mut Bar) -> Task<Message> {
    wallpaper::save_section("wallhaven", bar.launcher.wallhaven.filters.to_json());
    wallhaven_search(bar)
}

/// Asks matugen for the selected wallpaper's colors, unless they are known.
fn gallery_colors(bar: &Bar) -> Task<Message> {
    let Some(entry) = bar.launcher.gallery_selected() else {
        return Task::none();
    };
    if bar.launcher.gallery.colors.contains_key(&entry.path) {
        return Task::none();
    }
    let path = entry.path.clone();
    Task::perform(gallery::source_colors(path.clone()), move |colors| Message::GalleryColors(path.clone(), colors))
}

fn focus_launcher() -> Task<Message> {
    iced::widget::operation::focus(launcher::input_id())
}

/// Runs the action of the launcher row at `index`; `secondary` is Ctrl+Enter.
fn activate_launcher_item(bar: &mut Bar, index: usize, secondary: bool) -> Task<Message> {
    let Some(item) = launcher::items(&bar.launcher).into_iter().nth(index) else {
        return focus_launcher();
    };
    match item.action {
        launcher::Action::None => focus_launcher(),
        launcher::Action::Recall(index) => match bar.launcher.history.get(index) {
            Some(entry) => Task::batch([set_launcher_query(bar, entry.input.clone()), focus_launcher()]),
            None => focus_launcher(),
        },
        launcher::Action::Open(page) => Task::batch([enter_page(bar, page), resize_launcher(bar)]),
        launcher::Action::App(index) => {
            let Some(app) = bar.launcher.apps.get(index).cloned() else {
                return focus_launcher();
            };
            launch_app(&app, secondary);
            apps::bump(&mut bar.launcher.scores, &app.id, Local::now().timestamp());
            apps::save_scores(&bar.launcher.scores);
            close_launcher(bar)
        }
        launcher::Action::Power(power) => {
            if !bar.launcher.confirm(power) {
                return focus_launcher();
            }
            let ran = match power.command() {
                Some(command) => hyprland::exec(command),
                None => hyprland::exit(),
            };
            if !ran {
                eprintln!("zehll: cannot run {power:?}");
            }
            close_launcher(bar)
        }
        launcher::Action::File(entry) if secondary => Task::batch([set_launcher_query(bar, files::path_text(&entry)), focus_launcher()]),
        launcher::Action::File(entry) => {
            if !hyprland::exec(&format!("xdg-open {}", files::shell_quote(&entry.path))) {
                eprintln!("zehll: cannot open {}", entry.path);
            }
            close_launcher(bar)
        }
        launcher::Action::Clip(entry) if secondary => {
            bar.launcher.toggle_pin(&entry);
            clipboard::save_pins(&bar.launcher.pins);
            Task::batch([load_preview(bar), focus_launcher()])
        }
        launcher::Action::Clip(entry) => Task::batch([Task::future(clipboard::copy_entry(entry)).discard(), close_launcher(bar)]),
    }
}

/// Sets the launcher search; on the Calculator page qalc evaluates it, on the Files page fd
/// searches it once typing pauses.
fn set_launcher_query(bar: &mut Bar, query: String) -> Task<Message> {
    bar.launcher.set_query(query.clone());
    // The search moves the selection to the first matching wallpaper.
    if bar.launcher.page() == launcher::Page::Wallpapers {
        return gallery_colors(bar);
    }
    if bar.launcher.page() == launcher::Page::Wallhaven {
        return match bar.launcher.wallhaven.editing_key {
            Some(_) => Task::none(),
            None => Task::perform(tokio::time::sleep(WALLHAVEN_DEBOUNCE), move |()| Message::WallhavenSearch(query.clone())),
        };
    }
    if query.trim().is_empty() {
        return load_preview(bar);
    }
    match bar.launcher.page() {
        launcher::Page::Calculator => Task::perform(calc::evaluate(query.clone()), move |answer| Message::CalcAnswer(query.clone(), answer)),
        launcher::Page::Files => Task::batch([
            load_preview(bar),
            Task::perform(tokio::time::sleep(FILES_DEBOUNCE), move |()| Message::FilesSearch(query.clone())),
        ]),
        _ => Task::none(),
    }
}

/// Reads what the preview shows for the selected row, unless it is known: a Files entry's
/// details or a Clipboard entry's content.
fn load_preview(bar: &Bar) -> Task<Message> {
    if bar.launcher.page() == launcher::Page::Clipboard {
        return match launcher::selected_clip(&bar.launcher) {
            Some(entry) if bar.launcher.decoded(entry.id).is_none() => {
                let id = entry.id;
                Task::perform(clipboard::decode(entry), move |decoded| Message::ClipboardDecoded(id, decoded))
            }
            _ => Task::none(),
        };
    }
    if bar.launcher.page() != launcher::Page::Files {
        return Task::none();
    }
    match launcher::selected_file(&bar.launcher) {
        Some(entry) if bar.launcher.details(&entry.path).is_none() => {
            let path = entry.path;
            Task::perform(files::details(path.clone()), move |details| Message::FileDetails(path.clone(), details))
        }
        _ => Task::none(),
    }
}

/// Enter copies the answer and remembers it, or recalls the picked past calculation;
/// Ctrl+Enter puts the answer back in the search.
fn calculator_enter(bar: &mut Bar, secondary: bool) -> Task<Message> {
    if secondary {
        let Some(result) = bar.launcher.answer().map(|answer| answer.result.clone()) else {
            return focus_launcher();
        };
        return Task::batch([set_launcher_query(bar, format!("({result})")), focus_launcher()]);
    }
    match launcher::calculator_enter(&bar.launcher) {
        launcher::CalcEnter::Copy(answer) => {
            let entry = calc::Entry { input: bar.launcher.query.clone(), result: answer.result.clone() };
            calc::remember(&mut bar.launcher.history, entry);
            calc::save_history(&bar.launcher.history);
            Task::batch([Task::future(clipboard::copy(answer.result)).discard(), close_launcher(bar)])
        }
        launcher::CalcEnter::Recall(input) => Task::batch([set_launcher_query(bar, input), focus_launcher()]),
        launcher::CalcEnter::Nothing => focus_launcher(),
    }
}

/// Focuses an open window of `app`, else starts it; `secondary` starts its first desktop action.
fn launch_app(app: &apps::App, secondary: bool) {
    let window = match secondary {
        true => None,
        false => hyprland::clients().into_iter().find(|(_, class)| apps::owns_window(app, class)),
    };
    let done = match window {
        Some((address, _)) => hyprland::focus_window(&address),
        None => hyprland::exec(&apps::launch_command(app, secondary, &apps::terminal())),
    };
    if !done {
        eprintln!("zehll: cannot launch {}", app.id);
    }
}

fn close_media(bar: &mut Bar) -> Task<Message> {
    bar.media_seek = None;
    bar.media_spectrum.clear();
    let Some(open) = bar.media_popover.take() else {
        return Task::none();
    };
    Task::batch([open.panel, open.catcher].map(|id| match bar.surfaces.contains_key(&id) {
        true => Task::done(Message::RemoveWindow(id)),
        false => Task::none(),
    }))
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
        Some(Surface::Media) => return media::view(bar),
        Some(Surface::System) => return sysmon::view(bar),
        Some(Surface::Tooltip(Tooltip::System)) => return sysmon::tooltip(bar),
        Some(Surface::Launcher) => return launcher::view(bar),
        Some(Surface::Snip) => return snip::view(bar, id),
        Some(Surface::Wallpaper(name)) => {
            return match bar.wallpaper_layers.get(name) {
                Some(layer) => wallpaper::view(layer, Instant::now()),
                None => Space::new().into(),
            };
        }
        Some(Surface::Tooltip(Tooltip::Media)) => return media::tooltip(bar),
        Some(Surface::MeetingAlerts) => return alert::view(bar),
        Some(Surface::Popups) => return popup::view(bar),
        Some(Surface::Osd) => return osd::view(bar),
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
            // The workspaces sit on the bar's center: both sides of the middle third fill equally.
            row![
                container(Row::new().push(sysmon::bar_button(bar)).push(media::bar_button(bar)).spacing(6).align_y(Alignment::Center))
                    .align_right(Length::Fill),
                workspace_indicator(bar, id),
                container(Row::new().push(clock).push(meeting_pill(bar)).spacing(6).align_y(Alignment::Center))
                    .align_left(Length::Fill),
            ]
            .spacing(6)
            .width(Length::FillPortion(1))
            .align_y(Alignment::Center),
            row![
                container(tray_view(bar, id)).align_right(Length::Fill),
                // Hovering the right edge opens the side panel, like ii.
                mouse_area(Space::new().width(f32::from(BAR_PADDING)).height(Length::Fill)).on_enter(Message::HoverPanel),
            ]
            .width(Length::FillPortion(1))
            .align_y(Alignment::Center),
        ]
        .height(Length::Fill)
        .align_y(Alignment::Center),
    )
    .height(Length::Fill);

    container(column![content, line(theme.bar_border)])
        .style(move |_| container::background(theme.bar_background))
        .into()
}

/// The workspaces of the monitor the bar `id` is on.
fn workspace_indicator(bar: &Bar, id: Id) -> Element<'_, Message> {
    let name = bar.bars.get(&id);
    let occupied = bar
        .monitors
        .iter()
        .find(|monitor| Some(&monitor.name) == name)
        .map(|monitor| workspaces::occupied_slots(&monitor.occupied))
        .unwrap_or_default();
    let active = bar.monitors.iter().find(|monitor| Some(&monitor.name) == name).map(|monitor| monitor.workspace);
    let select = move |slot| active.and_then(|active| workspaces::target(active, slot)).map(Message::FocusWorkspace);
    workspaces::view(name.and_then(|name| bar.workspaces.get(name)), occupied, bar.theme, select)
}

/// Left margin of a surface `width` wide, centered under the pointer and kept on the monitor.
fn hang_under(cursor: i32, width: i32, monitor: i32) -> i32 {
    const EDGE: i32 = 8;
    (cursor - width / 2).min(monitor - width - EDGE).max(EDGE)
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
        Some(meeting) => {
            let glyph = |name| container(icon_sized(name, 16.0)).style(move |_| container::Style {
                text_color: Some(theme.text_muted),
                ..Default::default()
            });
            column![
                row![glyph("event"), text(&meeting.title).font(BODY_BOLD).size(15).wrapping(text::Wrapping::None)]
                    .spacing(10)
                    .align_y(Alignment::Center),
                row![
                    glyph("schedule"),
                    text(meetings::time_range(meeting)).font(CONDENSED).size(15).color(MEETING_TIME),
                    Space::new().width(Length::Fill),
                    text(meetings::countdown(meeting, now)).font(CONDENSED_BOLD).size(15).color(theme.accent),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            ]
            .spacing(6)
            .into()
        }
        None => Space::new().into(),
    };
    container(content)
        .padding([10, 14])
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltips_hang_centered_under_the_pointer_inside_the_monitor() {
        assert_eq!(hang_under(1000, 360, 2560), 820);
        assert_eq!(hang_under(100, 360, 2560), 8);
        assert_eq!(hang_under(2550, 360, 2560), 2192);
    }
}
