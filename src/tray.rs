use std::path::{Path, PathBuf};
use std::sync::Arc;

use iced::futures::{SinkExt, Stream};
use iced::stream;
use iced::widget::{image, svg};
use system_tray::client::{Client, Event, UpdateEvent};
use system_tray::item::{IconPixmap, StatusNotifierItem};
use system_tray::menu::TrayMenu;

pub const ICON_SIZE: u16 = 18;

#[derive(Debug, Clone)]
pub enum TrayEvent {
    Ready(Arc<Client>),
    Event(Event),
}

#[derive(Debug, Clone)]
pub enum Icon {
    Raster(image::Handle),
    Svg(svg::Handle),
}

#[derive(Debug, Clone)]
pub struct TrayItem {
    pub title: Option<String>,
    pub icon: Option<Icon>,
    pub menu: Option<TrayMenu>,
    pub menu_path: Option<String>,
    icon_theme_path: Option<String>,
}

impl TrayItem {
    pub fn new(item: &StatusNotifierItem) -> Self {
        let mut tray_item = Self {
            title: item.title.clone(),
            icon: None,
            menu: None,
            menu_path: item.menu.clone(),
            icon_theme_path: item.icon_theme_path.clone(),
        };
        tray_item.set_icon(item.icon_name.as_deref(), item.icon_pixmap.as_deref());
        tray_item
    }

    /// Prefers the named icon from the icon theme and falls back to the raw pixmap.
    pub fn set_icon(&mut self, name: Option<&str>, pixmaps: Option<&[IconPixmap]>) {
        self.icon = name
            .filter(|name| !name.is_empty())
            .and_then(|name| find_icon(name, self.icon_theme_path.as_deref()))
            .map(|path| match path.extension().and_then(|ext| ext.to_str()) {
                Some("svg") => Icon::Svg(svg::Handle::from_path(path)),
                _ => Icon::Raster(image::Handle::from_path(path)),
            })
            .or_else(|| pixmaps.and_then(pixmap_icon));
    }
}

/// Registers as a StatusNotifierHost and streams item changes.
pub fn events() -> impl Stream<Item = TrayEvent> {
    stream::channel(32, async |mut output| {
        let client = match Client::new().await {
            Ok(client) => Arc::new(client),
            Err(err) => {
                eprintln!("zehll: system tray unavailable: {err}");
                return;
            }
        };
        let mut events = client.subscribe();

        let initial: Vec<_> = client
            .items()
            .lock()
            .unwrap()
            .iter()
            .flat_map(|(address, (item, menu))| {
                let add = Event::Add(address.clone(), Box::new(item.clone()));
                let menu = menu.clone().map(|menu| Event::Update(address.clone(), UpdateEvent::Menu(menu)));
                std::iter::once(add).chain(menu)
            })
            .collect();
        if output.send(TrayEvent::Ready(client.clone())).await.is_err() {
            return;
        }
        for event in initial {
            if output.send(TrayEvent::Event(event)).await.is_err() {
                return;
            }
        }

        while let Ok(event) = events.recv().await {
            if output.send(TrayEvent::Event(event)).await.is_err() {
                return;
            }
        }
    })
}

fn find_icon(name: &str, theme_path: Option<&str>) -> Option<PathBuf> {
    if Path::new(name).is_absolute() {
        return Some(PathBuf::from(name));
    }
    // Some apps ship their icons in a private directory instead of the icon theme.
    if let Some(dir) = theme_path.filter(|dir| !dir.is_empty()) {
        for ext in ["svg", "png"] {
            let path = Path::new(dir).join(format!("{name}.{ext}"));
            if path.exists() {
                return Some(path);
            }
        }
    }
    let theme = freedesktop_icons::default_theme_gtk().unwrap_or_else(|| String::from("hicolor"));
    freedesktop_icons::lookup(name)
        .with_theme(&theme)
        .with_size(ICON_SIZE * 2)
        .with_cache()
        .find()
}

/// Converts the largest ARGB32 (network byte order) pixmap into an RGBA image.
fn pixmap_icon(pixmaps: &[IconPixmap]) -> Option<Icon> {
    let pixmap = pixmaps.iter().max_by_key(|pixmap| pixmap.width * pixmap.height)?;
    let rgba: Vec<u8> = pixmap
        .pixels
        .chunks_exact(4)
        .flat_map(|argb| [argb[1], argb[2], argb[3], argb[0]])
        .collect();
    Some(Icon::Raster(image::Handle::from_rgba(pixmap.width as u32, pixmap.height as u32, rgba)))
}
