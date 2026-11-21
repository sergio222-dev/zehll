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

/// Bound in Hyprland as `global, zehll:panelToggle`.
const PANEL_TOGGLE: &str = "panelToggle";

/// Emits once per press of the `zehll:panelToggle` global shortcut.
pub fn panel_toggle() -> impl Stream<Item = ()> {
    stream::channel(1, async |mut output| {
        // The protocol needs its own Wayland connection, which blocks while waiting for events.
        let (sender, mut receiver) = mpsc::unbounded();
        thread::spawn(move || {
            if let Err(err) = listen(sender) {
                eprintln!("zehll: global shortcuts unavailable: {err}");
            }
        });
        while receiver.next().await.is_some() {
            if output.send(()).await.is_err() {
                return;
            }
        }
    })
}

struct State {
    pressed: mpsc::UnboundedSender<()>,
}

fn listen(pressed: mpsc::UnboundedSender<()>) -> Result<(), Box<dyn Error>> {
    let connection = Connection::connect_to_env()?;
    let (globals, mut queue) = registry_queue_init::<State>(&connection)?;
    let handle = queue.handle();
    let manager: HyprlandGlobalShortcutsManagerV1 = globals.bind(&handle, 1..=1, ())?;
    manager.register_shortcut(
        PANEL_TOGGLE.to_owned(),
        String::from("zehll"),
        String::from("Toggle the side panel"),
        String::new(),
        &handle,
        (),
    );

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

impl Dispatch<HyprlandGlobalShortcutV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &HyprlandGlobalShortcutV1,
        event: hyprland_global_shortcut_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let hyprland_global_shortcut_v1::Event::Pressed { .. } = event {
            let _ = state.pressed.unbounded_send(());
        }
    }
}
