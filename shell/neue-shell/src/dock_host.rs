//! The dock in the shell process (task 3.16, ADR 0092): a layer surface at the bottom
//! centre of one output with a [`Dock`](neue_kit::desktop::Dock) on it, fed by the model in
//! `neue_shell::dock`.
//!
//! - Icons are found in the icon theme on a thread of their own, once per name.
//! - A click on an app without windows launches it with an activation token from the
//!   dock's surface (the compositor grants the shell's tokens, ADR 0066 addendum), so its
//!   first window comes up focused; the tile bounces until a window opens or
//!   `LAUNCH_TIMEOUT` passes. A click on a running app activates one of its windows.
//! - Every window's minimise target is its app's icon on the dock
//!   (`set_minimize_rectangle`, ADR 0066 §10), sent when it changes.
//!
//! The dock lives on the first output; when that output goes away it moves to another.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Instant;

use neue_config::{DockConfig, Layers, ShellConfig};
use neue_desktop::{AppDatabase, IconLoader, IconLookup, LaunchOptions};
use neue_kit::desktop::{
    Anchor, Dock as DockView, DockBounce, DockTile, LayerKeyboard, LayerSpec, OutputId, ShellLayer,
    ShellRequest, ToplevelId, dock_icon_rect, dock_size,
};
use neue_kit::tokens::materials::dock::{BOUNCE_ROOM, ICON};
use neue_kit::{Cx, Handle, Icon, WindowId};
use neue_shell::dock::{Click, Dock, Tile, minimize_targets};
use neue_shell::windows::Windows;

use crate::tray::{FALLBACK_THEME, ICON_THEME};

/// The surface's layer-shell namespace.
const NAMESPACE: &str = "neue-dock";
/// Icons are loaded for this scale, so they stay sharp on a doubled output.
const ICON_SCALE: f64 = 2.0;

/// What the surface, the icon loader, the compositor and the timers tell the dock.
#[derive(Debug, Clone)]
pub enum Event {
    /// The surface exists now and can be edited.
    Opened(WindowId),
    /// A tile was clicked.
    Clicked(String),
    /// Icons were looked up, by name (`None`: not found).
    Icons(Vec<(String, Option<Icon>)>),
    /// The compositor answered the token request for launching `entry` (tile `key`).
    Token {
        key: String,
        entry: String,
        token: Option<String>,
    },
    /// A launch may have run out of time.
    Deadline,
}

struct Surface<M> {
    window: WindowId,
    output: OutputId,
    view: Handle<DockView<M>>,
    /// The driver has created the window: its widgets can be edited.
    ready: bool,
}

/// The dock surface and its state.
pub struct DockHost<M> {
    wrap: fn(Event) -> M,
    model: Dock,
    surface: Option<Surface<M>>,
    /// Icons by theme name or path; `None`: looked up and not found.
    icons: HashMap<String, Option<Icon>>,
    /// Names being looked up.
    loading: HashSet<String>,
    /// What the dock shows now.
    shown: Vec<DockTile>,
    /// The minimise rectangle last sent for each window.
    sent: BTreeMap<ToplevelId, (i32, i32, i32, i32)>,
    /// The launch deadline a timer is running for.
    timer: Option<Instant>,
}

impl<M: Send + 'static> DockHost<M> {
    /// Keeping the apps `shell.toml` pins.
    pub fn new(wrap: fn(Event) -> M) -> Self {
        Self {
            wrap,
            model: Dock::new(config(&Layers::standard()).pinned),
            surface: None,
            icons: HashMap::new(),
            loading: HashSet::new(),
            shown: Vec::new(),
            sent: BTreeMap::new(),
            timer: None,
        }
    }

    /// Opens the dock on `output` unless it is open already.
    pub fn open(&mut self, cx: &mut Cx<'_, M>, output: OutputId) {
        if self.surface.is_some() {
            return;
        }
        let wrap = self.wrap;
        let mut view = Handle::default();
        let root = DockView::new(self.shown.clone())
            .on_click(move |key| Some(wrap(Event::Clicked(key.to_owned()))))
            .bind(&mut view);
        let size = dock_size(self.shown.len());
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the dock's size in whole logical pixels"
        )]
        let (width, height, zone, room) = (
            size.width.ceil() as u32,
            size.height.ceil() as u32,
            (size.height - BOUNCE_ROOM).ceil() as i32,
            BOUNCE_ROOM as u32,
        );
        let layer = LayerSpec {
            layer: ShellLayer::Top,
            // Maximised windows stop above the shelf; the bounce room is over them.
            exclusive_zone: zone,
            // Clicking an icon never takes the keyboard from the app being used.
            keyboard: LayerKeyboard::None,
            output: Some(output),
            fit_content: true,
            pass_through_top: room,
            ..LayerSpec::new(NAMESPACE, Anchor::BOTTOM)
        };
        let window = neue_kit::Window::new("Dock", (width, height), root).layer(layer);
        let window = cx.open_window(window);
        self.surface = Some(Surface {
            window,
            output,
            view,
            ready: false,
        });
        // The rectangles are on the new surface.
        self.sent.clear();
        // Delivered after the driver has created the window.
        cx.spawn(move |_| wrap(Event::Opened(window)));
    }

    /// `output` went away: the dock moves to `next`, if there is another.
    pub fn output_removed(&mut self, cx: &mut Cx<'_, M>, output: OutputId, next: Option<OutputId>) {
        if self.surface.as_ref().is_none_or(|s| s.output != output) {
            return;
        }
        if let Some(surface) = self.surface.take() {
            cx.close_window(surface.window);
        }
        if let Some(next) = next {
            self.open(cx, next);
        }
    }

    pub fn event(
        &mut self,
        cx: &mut Cx<'_, M>,
        event: Event,
        windows: &Windows,
        apps: &AppDatabase,
    ) {
        match event {
            Event::Opened(window) => {
                if let Some(surface) = self.surface.as_mut().filter(|s| s.window == window) {
                    surface.ready = true;
                    surface.view.set_tiles(cx, self.shown.clone());
                }
                self.sync(cx, windows, apps);
            }
            Event::Clicked(key) => self.click(cx, &key, windows, apps),
            Event::Icons(found) => {
                for (name, icon) in found {
                    self.loading.remove(&name);
                    self.icons.insert(name, icon);
                }
                self.sync(cx, windows, apps);
            }
            Event::Token { key, entry, token } => {
                let launched = apps.by_id(&entry).map(|entry| {
                    let options = LaunchOptions {
                        activation_token: token.as_deref(),
                        ..LaunchOptions::default()
                    };
                    neue_desktop::launch::launch(entry, &options)
                });
                match launched {
                    Some(Ok(pid)) => tracing::debug!(%entry, pid, "launched from the dock"),
                    Some(Err(error)) => {
                        tracing::warn!(%error, %entry, "can't launch from the dock");
                        self.model.cancel_launch(&key);
                        self.sync(cx, windows, apps);
                    }
                    None => {
                        tracing::warn!(%entry, "the app to launch has gone");
                        self.model.cancel_launch(&key);
                        self.sync(cx, windows, apps);
                    }
                }
            }
            Event::Deadline => {
                self.timer = None;
                self.sync(cx, windows, apps);
            }
        }
    }

    fn click(&mut self, cx: &mut Cx<'_, M>, key: &str, windows: &Windows, apps: &AppDatabase) {
        match self.model.click(key, windows, apps) {
            Click::Launch(entry) => {
                // Bouncing at once, while the token is on its way.
                self.model.launched(key, Instant::now());
                self.sync(cx, windows, apps);
                let Some(surface) = &self.surface else {
                    return;
                };
                let (wrap, key) = (self.wrap, key.to_owned());
                cx.activation_token(surface.window, move |token| {
                    wrap(Event::Token { key, entry, token })
                });
            }
            Click::Activate(window) => cx.shell(ShellRequest::Activate(window)),
            Click::Nothing => {}
        }
    }

    /// Shows the window list and the apps as they are now: the tiles, their icons (looking
    /// up new ones), the minimise targets, and a timer for the next launch to give up.
    pub fn sync(&mut self, cx: &mut Cx<'_, M>, windows: &Windows, apps: &AppDatabase) {
        let now = Instant::now();
        self.model.update(windows, apps, now);
        let tiles = self.model.tiles(windows, apps);
        self.load_icons(cx, &tiles);
        let shown: Vec<DockTile> = tiles.iter().map(|tile| self.view(tile)).collect();
        if let Some(surface) = self.surface.as_ref().filter(|s| s.ready) {
            if shown != self.shown {
                surface.view.set_tiles(cx, shown.clone());
            }
            self.send_targets(cx, surface.window, &tiles);
        }
        self.shown = shown;
        if let Some(deadline) = self.model.next_deadline()
            && self.timer.is_none_or(|t| deadline < t)
        {
            self.timer = Some(deadline);
            let wrap = self.wrap;
            cx.spawn(move |_| {
                std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
                wrap(Event::Deadline)
            });
        }
    }

    fn view(&self, tile: &Tile) -> DockTile {
        let bounce = if tile.launching {
            DockBounce::Launch
        } else if tile.attention {
            DockBounce::Attention
        } else {
            DockBounce::None
        };
        DockTile {
            key: tile.key.clone(),
            name: tile.name.clone(),
            icon: tile
                .icon
                .as_ref()
                .and_then(|name| self.icons.get(name).cloned().flatten()),
            running: tile.running(),
            bounce,
        }
    }

    /// Looks up the icons of `tiles` not looked up yet, on a thread.
    fn load_icons(&mut self, cx: &mut Cx<'_, M>, tiles: &[Tile]) {
        let wanted: Vec<String> = tiles
            .iter()
            .filter_map(|t| t.icon.clone())
            .filter(|name| !self.icons.contains_key(name) && !self.loading.contains(name))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        if wanted.is_empty() {
            return;
        }
        self.loading.extend(wanted.iter().cloned());
        let wrap = self.wrap;
        cx.spawn(move |_| {
            let mut loader = IconLoader::new(IconLookup::system(ICON_THEME, Some(FALLBACK_THEME)));
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "the dock's 48 px icon"
            )]
            let size = ICON as u32;
            let found = wanted
                .into_iter()
                .map(|name| {
                    let icon = loader.load(&name, size, ICON_SCALE);
                    if icon.is_none() {
                        tracing::debug!(%name, "no icon for a dock tile");
                    }
                    (name, icon)
                })
                .collect();
            wrap(Event::Icons(found))
        });
    }

    /// Tells the compositor where each window minimises to, where that changed.
    fn send_targets(&mut self, cx: &mut Cx<'_, M>, surface: WindowId, tiles: &[Tile]) {
        let targets = minimize_targets(tiles);
        self.sent.retain(|window, _| targets.contains_key(window));
        for (window, index) in targets {
            let r = dock_icon_rect(index);
            #[expect(
                clippy::cast_possible_truncation,
                reason = "an icon's rect on the dock, in whole logical pixels"
            )]
            let rect = (
                r.x0.round() as i32,
                r.y0.round() as i32,
                r.width().round() as i32,
                r.height().round() as i32,
            );
            if self.sent.get(&window) != Some(&rect) {
                self.sent.insert(window, rect);
                cx.shell(ShellRequest::SetMinimizeRectangle {
                    toplevel: window,
                    surface,
                    rect: Some(rect),
                });
            }
        }
    }
}

/// `shell.toml`'s `[dock]`; the defaults when the file doesn't load.
fn config(layers: &Layers) -> DockConfig {
    match neue_config::load::<ShellConfig>(layers) {
        Ok(loaded) => loaded.config.dock,
        Err(error) => {
            tracing::warn!(%error, "shell.toml is invalid: using the default dock");
            DockConfig::default()
        }
    }
}
