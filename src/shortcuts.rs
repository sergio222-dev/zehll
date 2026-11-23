use std::error::Error;
use std::thread;

use iced::futures::channel::mpsc;
use iced::futures::{SinkExt, Stream, StreamExt};
use iced::stream;
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols_hyprland::global_shortcuts::v1::client::hyprland_global_shortcut_v1::{self, HyprlandGlobalShortcutV1};
use wayland_protocols_hyprland::global_shortcuts::v1::client::hyprland_global_shortcuts_manager_v1::HyprlandGlobalShortcutsManagerV1;

/// Hyprland global shortcuts, bound as `global, zehll:<name>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shortcut {
    PanelToggle,
    MeetingTest,
    LauncherToggle,
    /// The release of a SUPER tap: the Hyprland config arms it on SUPER, cancels it on any
    /// other key and sends it from a release bind, which reaches Zehll as `released`.
    LauncherTap,
    Screenshot,
    /// The launcher on its Wallpapers page.
    Wallpapers,
    /// A random wallpaper from the wallpaper folder.
    WallpaperRandom,
    /// The launcher on its Wallhaven page.
    Wallhaven,
    /// The launcher on its Emojis page.
    Emojis,
    /// The launcher on its Clipboard page.
    Clipboard,
}

impl Shortcut {
    const ALL: [Shortcut; 10] = [
        Shortcut::PanelToggle,
        Shortcut::MeetingTest,
        Shortcut::LauncherToggle,
        Shortcut::LauncherTap,
        Shortcut::Screenshot,
        Shortcut::Wallpapers,
        Shortcut::WallpaperRandom,
        Shortcut::Wallhaven,
        Shortcut::Emojis,
        Shortcut::Clipboard,
    ];

    fn name(self) -> &'static str {
        match self {
            Shortcut::PanelToggle => "panelToggle",
            Shortcut::MeetingTest => "meetingTest",
            Shortcut::LauncherToggle => "launcherToggle",
            Shortcut::LauncherTap => "launcherTap",
            Shortcut::Screenshot => "screenshot",
            Shortcut::Wallpapers => "wallpapers",
            Shortcut::WallpaperRandom => "wallpaperRandom",
            Shortcut::Wallhaven => "wallhaven",
            Shortcut::Emojis => "emojis",
            Shortcut::Clipboard => "clipboard",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Shortcut::PanelToggle => "Toggle the side panel",
            Shortcut::MeetingTest => "Show a test meeting alert",
            Shortcut::LauncherToggle => "Toggle the app launcher",
            Shortcut::LauncherTap => "Toggle the app launcher when released (SUPER tap)",
            Shortcut::Screenshot => "Snip a zone of the screen",
            Shortcut::Wallpapers => "Toggle the launcher on its Wallpapers page",
            Shortcut::WallpaperRandom => "Apply a random wallpaper",
            Shortcut::Wallhaven => "Toggle the launcher on its Wallhaven page",
            Shortcut::Emojis => "Toggle the launcher on its Emojis page",
            Shortcut::Clipboard => "Toggle the launcher on its Clipboard page",
        }
    }

    /// Whether a `pressed` (or else `released`) event of this shortcut triggers it.
    fn fires(self, pressed: bool) -> bool {
        pressed != (self == Shortcut::LauncherTap)
    }
}

/// Emits each press of a `zehll:` global shortcut.
pub fn events() -> impl Stream<Item = Shortcut> {
    stream::channel(1, async |mut output| {
        // The protocol needs its own Wayland connection, which blocks while waiting for events.
        let (sender, mut receiver) = mpsc::unbounded();
        thread::spawn(move || {
            if let Err(err) = listen(sender) {
                eprintln!("zehll: global shortcuts unavailable: {err}");
            }
        });
        while let Some(shortcut) = receiver.next().await {
            if output.send(shortcut).await.is_err() {
                return;
            }
        }
    })
}

struct State {
    pressed: mpsc::UnboundedSender<Shortcut>,
}

fn listen(pressed: mpsc::UnboundedSender<Shortcut>) -> Result<(), Box<dyn Error>> {
    let connection = Connection::connect_to_env()?;
    let (globals, mut queue) = registry_queue_init::<State>(&connection)?;
    let handle = queue.handle();
    let manager: HyprlandGlobalShortcutsManagerV1 = globals.bind(&handle, 1..=1, ())?;
    for shortcut in Shortcut::ALL {
        manager.register_shortcut(
            shortcut.name().to_owned(),
            String::from("zehll"),
            shortcut.description().to_owned(),
            String::new(),
            &handle,
            shortcut,
        );
    }

    let mut state = State { pressed };
    loop {
        queue.blocking_dispatch(&mut state)?;
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(_: &mut Self, _: &WlRegistry, _: <WlRegistry as wayland_client::Proxy>::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<HyprlandGlobalShortcutsManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &HyprlandGlobalShortcutsManagerV1,
        _: <HyprlandGlobalShortcutsManagerV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<HyprlandGlobalShortcutV1, Shortcut> for State {
    fn event(
        state: &mut Self,
        _: &HyprlandGlobalShortcutV1,
        event: hyprland_global_shortcut_v1::Event,
        shortcut: &Shortcut,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let pressed = match event {
            hyprland_global_shortcut_v1::Event::Pressed { .. } => true,
            hyprland_global_shortcut_v1::Event::Released { .. } => false,
            _ => return,
        };
        if shortcut.fires(pressed) {
            let _ = state.pressed.unbounded_send(*shortcut);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_fire_on_press_and_the_super_tap_on_release() {
        for shortcut in Shortcut::ALL {
            let tap = shortcut == Shortcut::LauncherTap;
            assert_eq!(shortcut.fires(true), !tap, "{shortcut:?} pressed");
            assert_eq!(shortcut.fires(false), tap, "{shortcut:?} released");
        }
    }
}
