//! The dock in the shell process (tasks 3.16 and 3.17a, ADRs 0092 and 0093): a layer
//! surface centred on the bottom, left or right edge of one output (`[dock] edge`) with a
//! [`Dock`](neue_kit::desktop::Dock) on it, fed by the model in `neue_shell::dock`.
//!
//! - Icons are found in the icon theme on a thread of their own, once per name.
//! - A click on an app without windows launches it with an activation token from the
//!   dock's surface (the compositor grants the shell's tokens, ADR 0066 addendum), so its
//!   first window comes up focused; the tile bounces until a window opens or
//!   `LAUNCH_TIMEOUT` passes. A click on a running app activates one of its windows.
//! - Every window's minimise target is its app's icon on the dock
//!   (`set_minimize_rectangle`, ADR 0066 §10), where the dock last reported it (it moves
//!   with magnification), sent when it changes.
//!
//! - Editing (task 3.17b, ADR 0094): a tile dragged to another place is kept in the dock
//!   there, one dragged off is let go, and a tile's menu lists the app's windows, Keep in
//!   Dock, and Open or Quit (which asks each window to close). Each change of the kept
//!   apps is saved as `[dock] pinned` through `neue-settingsd`, on a thread.
//!
//! - The Trash and minimised windows (task 3.17c, ADR 0095): the Trash tile shows whether
//!   the trash has anything in it (polled every `TRASH_POLL` on a thread, since the shell
//!   has no file watcher), opens it with `xdg-open` and empties it from its menu; a
//!   minimised window's tile restores it, from its click or its menu.
//!
//! The dock lives on the first output; when that output goes away it moves to another.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};

use neue_config::{DockConfig, Layers, ShellConfig};
use neue_dbus::settings::Settings as SettingsClient;
use neue_desktop::{AppDatabase, IconLoader, IconLookup, LaunchOptions, Trash, TrashState};
use neue_kit::desktop::{
    Anchor, Dock as DockView, DockBounce, DockTile, DockTileKind, LayerKeyboard, LayerSpec,
    OutputId, ShellLayer, ShellRequest, ToplevelId,
};
use neue_kit::desktop::{DockEdge, DockLayout, Margin, Rect};
use neue_kit::{Cx, Handle, Icon, Menu, MenuItem, WindowId};
use neue_shell::dock::{Click, Dock, Tile, TileKind, minimize_targets};
use neue_shell::windows::{Windows, app_name};

use crate::tray::{FALLBACK_THEME, ICON_THEME};

/// The surface's layer-shell namespace.
const NAMESPACE: &str = "neue-dock";
/// Icons are loaded for this scale, so they stay sharp on a doubled output.
const ICON_SCALE: f64 = 2.0;
/// How often the trash is looked at.
const TRASH_POLL: Duration = Duration::from_secs(2);
/// The `neue-settingsd` key of the apps kept in the dock.
const PINNED_KEY: &str = "shell.dock.pinned";

/// What the surface, the icon loader, the compositor and the timers tell the dock.
#[derive(Debug, Clone)]
pub enum Event {
    /// The surface exists now and can be edited.
    Opened(WindowId),
    /// A tile was clicked.
    Clicked(String),
    /// The icons moved: each tile's key and its icon's rect on the surface.
    Placed(Vec<(String, Rect)>),
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
    /// A tile was dragged to index `to` of the tiles shown.
    Moved { key: String, to: usize },
    /// A tile was dragged off the dock.
    Removed(String),
    /// A tile's menu is wanted.
    MenuWanted(String),
    /// A window was chosen from its app's menu.
    Activate(ToplevelId),
    /// Keep in Dock was chosen: whether the app is now to be kept.
    Keep { key: String, keep: bool },
    /// Quit was chosen: every window of the app is asked to close.
    Quit(String),
    /// Saving `[dock] pinned` finished.
    Saved(Result<(), String>),
    /// The poll looked at the trash (`None`: it can't be read); it looks again later.
    TrashState(Option<TrashState>),
    /// The trash was looked at once, after it was emptied.
    TrashChecked(Option<TrashState>),
    /// Open the Trash was chosen (or its tile clicked).
    OpenTrash,
    /// Empty Trash was chosen.
    EmptyTrash,
    /// Emptying or opening the Trash finished.
    TrashDone(Result<(), String>),
    /// Close was chosen from a minimised window's menu.
    Close(ToplevelId),
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
    /// The edge, the icons' size and magnification, from `shell.toml`.
    layout: DockLayout,
    /// Where the dock last said each tile's icon is, by key.
    placed: HashMap<String, Rect>,
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
    /// The user's trash; `None` without a home directory.
    trash: Option<Trash>,
    /// The trash poll is running.
    polling: bool,
}

impl<M: Send + 'static> DockHost<M> {
    /// Keeping the apps `shell.toml` pins.
    pub fn new(wrap: fn(Event) -> M) -> Self {
        let config = config(&Layers::standard());
        Self {
            wrap,
            layout: layout(&config),
            placed: HashMap::new(),
            model: Dock::new(config.pinned),
            surface: None,
            icons: HashMap::new(),
            loading: HashSet::new(),
            shown: Vec::new(),
            sent: BTreeMap::new(),
            timer: None,
            trash: Trash::home(),
            polling: false,
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
            .layout(self.layout)
            .on_click(move |key| Some(wrap(Event::Clicked(key.to_owned()))))
            .on_placed(move |icons| Some(wrap(Event::Placed(icons.to_vec()))))
            .on_move(move |key, to| {
                Some(wrap(Event::Moved {
                    key: key.to_owned(),
                    to,
                }))
            })
            .on_remove(move |key| Some(wrap(Event::Removed(key.to_owned()))))
            .on_menu(move |key| Some(wrap(Event::MenuWanted(key.to_owned()))))
            .bind(&mut view);
        let size = self.layout.size(self.shown.len());
        let insets = self.layout.pass_through();
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the dock's size in whole logical pixels"
        )]
        let (width, height, zone) = (
            size.width.ceil() as u32,
            size.height.ceil() as u32,
            self.layout.band().ceil() as i32,
        );
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the room the pointer passes through, in whole logical pixels"
        )]
        let pass_through = Margin {
            top: insets.y0.floor() as i32,
            right: insets.x1.floor() as i32,
            bottom: insets.y1.floor() as i32,
            left: insets.x0.floor() as i32,
        };
        let anchor = match self.layout.edge {
            DockEdge::Bottom => Anchor::BOTTOM,
            DockEdge::Left => Anchor::LEFT,
            DockEdge::Right => Anchor::RIGHT,
        };
        let layer = LayerSpec {
            layer: ShellLayer::Top,
            // Maximised windows stop at the shelf; the bounce room is over them.
            exclusive_zone: zone,
            // Clicking an icon never takes the keyboard from the app being used.
            keyboard: LayerKeyboard::None,
            output: Some(output),
            fit_content: true,
            // The pointer reaches the dock at rest only: the room for bounces, grown
            // icons and the dock growing at its ends is over the windows.
            pass_through,
            ..LayerSpec::new(NAMESPACE, anchor)
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
        self.placed.clear();
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
                // The dock may open again on another output: one poll serves them all.
                if !std::mem::replace(&mut self.polling, true) {
                    self.look_at_trash(cx, Duration::ZERO, Event::TrashState);
                }
            }
            Event::Clicked(key) => self.click(cx, &key, windows, apps),
            Event::Placed(icons) => {
                self.placed = icons.into_iter().collect();
                if let Some(surface) = self.surface.as_ref().filter(|s| s.ready) {
                    let tiles = self.model.tiles(windows, apps);
                    self.send_targets(cx, surface.window, &tiles);
                }
            }
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
            Event::Moved { key, to } => {
                let changed = self.model.move_tile(&key, to, windows, apps);
                self.kept_changed(cx, changed, windows, apps);
            }
            Event::Removed(key) => {
                let changed = self.model.unpin(&key, windows, apps);
                self.kept_changed(cx, changed, windows, apps);
            }
            Event::Keep { key, keep } => {
                let changed = if keep {
                    self.model.pin(&key, windows, apps)
                } else {
                    self.model.unpin(&key, windows, apps)
                };
                self.kept_changed(cx, changed, windows, apps);
            }
            Event::MenuWanted(key) => self.show_menu(cx, &key, windows, apps),
            Event::Activate(window) => cx.shell(ShellRequest::Activate(window)),
            Event::Quit(key) => {
                let tiles = self.model.tiles(windows, apps);
                for tile in tiles.iter().filter(|t| t.key == key) {
                    for &window in &tile.windows {
                        cx.shell(ShellRequest::Close(window));
                    }
                }
            }
            Event::TrashState(state) => {
                if self.model.set_trash(state) {
                    self.sync(cx, windows, apps);
                }
                self.look_at_trash(cx, TRASH_POLL, Event::TrashState);
            }
            Event::TrashChecked(state) => {
                if self.model.set_trash(state) {
                    self.sync(cx, windows, apps);
                }
            }
            Event::OpenTrash => self.open_trash(cx),
            Event::EmptyTrash => {
                let (wrap, trash) = (self.wrap, self.trash.clone());
                cx.spawn(move |_| {
                    wrap(Event::TrashDone(
                        trash
                            .ok_or_else(|| "there is no trash".to_owned())
                            .and_then(|t| t.empty().map(drop).map_err(|e| e.to_string())),
                    ))
                });
            }
            Event::TrashDone(result) => {
                if let Err(error) = result {
                    tracing::warn!(%error, "the Trash didn't open or empty");
                }
                // The can changes as soon as it is emptied, not at the next poll.
                self.look_at_trash(cx, Duration::ZERO, Event::TrashChecked);
            }
            Event::Close(window) => cx.shell(ShellRequest::Close(window)),
            Event::Saved(Ok(())) => tracing::debug!("saved the apps kept in the dock"),
            Event::Saved(Err(error)) => tracing::warn!(
                %error,
                "the apps kept in the dock were not saved: is neue-settingsd running?"
            ),
        }
    }

    /// The kept apps changed (`changed`) or a drag was refused: shows the tiles as they
    /// are now (a refused drag slides back) and saves `[dock] pinned` if it changed.
    fn kept_changed(
        &mut self,
        cx: &mut Cx<'_, M>,
        changed: bool,
        windows: &Windows,
        apps: &AppDatabase,
    ) {
        self.sync(cx, windows, apps);
        if !changed {
            return;
        }
        let pinned = self.model.pinned().to_vec();
        tracing::debug!(?pinned, "the apps kept in the dock changed");
        let wrap = self.wrap;
        cx.spawn(move |_| {
            wrap(Event::Saved(
                SettingsClient::session()
                    .and_then(|settings| settings.set_strings(PINNED_KEY, &pinned))
                    .map_err(|e| e.to_string()),
            ))
        });
    }

    /// Shows tile `key`'s menu: the app's windows (the active one checked), Keep in Dock,
    /// and Quit while it runs or Open while it doesn't.
    fn show_menu(&mut self, cx: &mut Cx<'_, M>, key: &str, windows: &Windows, apps: &AppDatabase) {
        let Some(surface) = self.surface.as_ref().filter(|s| s.ready) else {
            return;
        };
        let tiles = self.model.tiles(windows, apps);
        let Some(tile) = tiles.iter().find(|t| t.key == key) else {
            return;
        };
        let wrap = self.wrap;
        let mut menu = Menu::new();
        match tile.kind {
            TileKind::App => {}
            TileKind::Divider => return,
            TileKind::Minimised(id) => {
                menu = menu
                    .with(
                        MenuItem::new("Restore").on_choose_with(move || wrap(Event::Activate(id))),
                    )
                    .separator()
                    .with(MenuItem::new("Close").on_choose_with(move || wrap(Event::Close(id))));
                surface.view.show_menu(cx, key, menu);
                return;
            }
            TileKind::Trash(state) => {
                menu = menu
                    .with(MenuItem::new("Open").on_choose_with(move || wrap(Event::OpenTrash)))
                    .separator()
                    .with(
                        MenuItem::new("Empty Trash")
                            .disabled(state == TrashState::Empty)
                            .on_choose_with(move || wrap(Event::EmptyTrash)),
                    );
                surface.view.show_menu(cx, key, menu);
                return;
            }
        }
        let active = windows.active_id();
        for &id in &tile.windows {
            let Some((_, window)) = windows.iter().find(|(w, _)| *w == id) else {
                continue;
            };
            let title = if window.title.trim().is_empty() {
                app_name(window)
            } else {
                window.title.clone()
            };
            menu = menu.with(
                MenuItem::new(title)
                    .checked(Some(id) == active)
                    .on_choose_with(move || wrap(Event::Activate(id))),
            );
        }
        if tile.running() {
            menu = menu.separator();
        }
        let (keep, owned) = (!tile.pinned, key.to_owned());
        menu = menu.with(
            MenuItem::new("Keep in Dock")
                .checked(tile.pinned)
                .disabled(tile.entry.is_none())
                .on_choose_with(move || {
                    wrap(Event::Keep {
                        key: owned.clone(),
                        keep,
                    })
                }),
        );
        menu = menu.separator();
        let owned = key.to_owned();
        menu = if tile.running() {
            menu.with(
                MenuItem::new("Quit").on_choose_with(move || wrap(Event::Quit(owned.clone()))),
            )
        } else {
            menu.with(
                MenuItem::new("Open").on_choose_with(move || wrap(Event::Clicked(owned.clone()))),
            )
        };
        surface.view.show_menu(cx, key, menu);
    }

    /// Opens the Trash in the default file manager, on a thread.
    fn open_trash(&self, cx: &mut Cx<'_, M>) {
        let wrap = self.wrap;
        cx.spawn(move |_| {
            wrap(Event::TrashDone(
                std::process::Command::new("xdg-open")
                    .arg("trash:///")
                    .status()
                    .map_err(|e| format!("can't run xdg-open: {e}"))
                    .and_then(|status| {
                        status
                            .success()
                            .then_some(())
                            .ok_or_else(|| format!("xdg-open: {status}"))
                    }),
            ))
        });
    }

    /// Looks at the trash after `wait`, on a thread, and tells the dock with `make`.
    fn look_at_trash(
        &self,
        cx: &mut Cx<'_, M>,
        wait: Duration,
        make: fn(Option<TrashState>) -> Event,
    ) {
        let (wrap, Some(trash)) = (self.wrap, self.trash.clone()) else {
            return;
        };
        cx.spawn(move |_| {
            std::thread::sleep(wait);
            let state = trash
                .state()
                .inspect_err(|error| tracing::debug!(%error, "can't read the trash"))
                .ok();
            wrap(make(state))
        });
    }

    fn click(&mut self, cx: &mut Cx<'_, M>, key: &str, windows: &Windows, apps: &AppDatabase) {
        match self.model.click(key, windows, apps) {
            Click::OpenTrash => self.open_trash(cx),
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
            kind: match tile.kind {
                TileKind::App | TileKind::Trash(_) => DockTileKind::App,
                TileKind::Minimised(_) => DockTileKind::Minimised,
                TileKind::Divider => DockTileKind::Divider,
            },
            fixed: tile.kind != TileKind::App,
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
        // Sharp at the largest size an icon grows to.
        let icon_size = self.layout.magnified.unwrap_or(0.0).max(self.layout.icon);
        cx.spawn(move |_| {
            let mut loader = IconLoader::new(IconLookup::system(ICON_THEME, Some(FALLBACK_THEME)));
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "the dock's icon size, at most 256 px"
            )]
            let size = icon_size.ceil() as u32;
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
        let rest = self.layout.rest(tiles.len());
        for (window, index) in targets {
            // Where the dock said the icon is; at rest until it has said.
            let placed = tiles.get(index).and_then(|t| self.placed.get(&t.key));
            let Some(&r) = placed.or_else(|| rest.icons.get(index)) else {
                continue;
            };
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

/// The dock's layout from `[dock]`.
fn layout(config: &DockConfig) -> DockLayout {
    let edge = match config.edge {
        neue_config::DockEdge::Bottom => DockEdge::Bottom,
        neue_config::DockEdge::Left => DockEdge::Left,
        neue_config::DockEdge::Right => DockEdge::Right,
    };
    let magnified = config
        .magnification
        .then_some(f64::from(config.magnified_size));
    DockLayout::new(edge, f64::from(config.size), magnified)
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
