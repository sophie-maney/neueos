//! What the desktop shell gets from a driver beyond windows (task 3.2a, ADR 0067): the
//! outputs, so it can put a layer surface on each, and the compositor's window list and
//! window requests through `neue_shell_v1` (ADR 0066).
//!
//! Thumbnails of windows (task 3.17d, ADR 0096) come from `ext-image-copy-capture` with
//! toplevel sources: [`ShellRequest::Thumbnail`] starts and stops them, and
//! [`ShellEvent::Thumbnail`] brings each new image.
//!
//! Only the Wayland driver fills these in. The winit driver reports no outputs and no
//! shell protocol, and drops shell requests.

/// Identifies an output for as long as it is connected. Never reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OutputId(u64);

impl OutputId {
    /// An id made outside a driver, for fakes in tests. Never mix such ids with a running
    /// driver's.
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }
}

/// An output (a display) as the compositor describes it.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputInfo {
    pub id: OutputId,
    /// The connector name (`eDP-1`, `HEADLESS-1`); empty if the compositor sent none.
    pub name: String,
    /// The human-readable description (make and model); may be empty.
    pub description: String,
    /// Size in logical pixels, which is what a layer surface spanning it measures.
    pub logical_size: (u32, u32),
    /// The integer scale `wl_output` advertises. Surfaces on it learn their exact
    /// (fractional) scale on their own.
    pub scale: i32,
}

/// An output appeared, changed or went away.
#[derive(Debug, Clone, PartialEq)]
pub enum OutputEvent {
    /// Sent once the output's description is complete, including for every output that
    /// existed when the driver started.
    Added(OutputInfo),
    /// Its mode, scale, name or position changed.
    Changed(OutputInfo),
    /// It was unplugged. The compositor closes layer surfaces on it on its own.
    Removed(OutputId),
}

/// Identifies a window in the shell's window list (not one of the app's own windows) for
/// as long as it exists. Never reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ToplevelId(u64);

impl ToplevelId {
    /// An id made outside a driver, for fakes in tests.
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }
}

/// A window in the compositor's window list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Toplevel {
    /// `window-N`, N being the `neuectl` window id; the same string as
    /// `ext-foreign-toplevel-list`'s identifier.
    pub identifier: String,
    /// Empty when unset.
    pub title: String,
    /// Empty when unset.
    pub app_id: String,
    pub minimized: bool,
    /// It asked for focus it wasn't given (the dock bounces its icon).
    pub attention: bool,
    /// Super reaches the window's app as Control (ADR 0079), so its Control shortcuts are
    /// what the user types with ⌘ (`command_key`, ADR 0081). False for apps that take
    /// Super themselves, and with a compositor that doesn't say.
    pub super_as_ctrl: bool,
    /// Where the window's app exports its menus, for the global menu (ADR 0005).
    pub menu: ToplevelMenu,
}

/// Where a window's menus are exported (`neue_shell_v1`'s `menu_*` events, ADR 0025).
/// The strings passed only a syntax check in the compositor.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ToplevelMenu {
    /// Nothing exported: the shell shows the fallback menu.
    #[default]
    None,
    /// `com.canonical.dbusmenu` at `service` and `path` (Qt, Chromium, neue-kit apps).
    DbusMenu { service: String, path: String },
    /// GTK's `org.gtk.Menus` and `org.gtk.Actions` on `bus_name`.
    GMenu {
        bus_name: String,
        menubar_path: Option<String>,
        app_menu_path: Option<String>,
        application_path: Option<String>,
        window_path: Option<String>,
    },
}

/// A picture of a window's content, as [`ShellEvent::Thumbnail`] brings it.
#[derive(Clone, PartialEq, Eq)]
pub struct ToplevelImage {
    /// Width in pixels, at least 1.
    pub width: u32,
    /// Height in pixels, at least 1.
    pub height: u32,
    /// Premultiplied RGBA8, row by row from the top left, `width × height × 4` bytes.
    pub pixels: std::sync::Arc<[u8]>,
}

impl std::fmt::Debug for ToplevelImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToplevelImage")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

/// What the compositor tells the shell (`neue_shell_v1`, and window captures).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellEvent {
    /// A window appeared or changed: its whole state, sent when the compositor finishes a
    /// batch of changes. The first one for an id is the window appearing.
    Toplevel(ToplevelId, Toplevel),
    /// The window is gone.
    Closed(ToplevelId),
    /// The window whose app owns the menu bar changed. `None`: no window is active (the
    /// desktop was clicked, or the last window closed or was minimised). Always sent after
    /// the window's own [`ShellEvent::Toplevel`].
    Active(Option<ToplevelId>),
    /// A [`ShellRequest::RunAction`] failed.
    ActionFailed { name: String, message: String },
    /// The user asked to reach the menu bar from the keyboard (Ctrl+F2): open the first
    /// menu of the bar on `output` (`None`: any) from the keyboard. Opening it right away
    /// lets the driver grab with this event's serial.
    FocusMenuBar { output: Option<OutputId> },
    /// A volume, brightness or media key (task 3.14a). The compositor routes these to the
    /// shell whatever has the keyboard. The driver repeats the stepping keys
    /// ([`SystemKey::repeats`]) at the keyboard's rate while they're held.
    SystemKey {
        key: SystemKey,
        state: SystemKeyState,
        /// The modifiers held (Shift+Alt steps finer, as on the Mac).
        modifiers: ChordModifiers,
    },
    /// A new picture of a window asked for with [`ShellRequest::Thumbnail`]: the first
    /// once the capture starts, then one each time the window changes, at most every
    /// half second.
    Thumbnail(ToplevelId, ToplevelImage),
}

/// A key the compositor hands to the shell (`neue_shell_v1`'s `system_key`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SystemKey {
    VolumeUp,
    VolumeDown,
    Mute,
    MicMute,
    BrightnessUp,
    BrightnessDown,
    KeyboardBrightnessUp,
    KeyboardBrightnessDown,
    Play,
    Pause,
    Next,
    Previous,
    Stop,
    Eject,
}

impl SystemKey {
    /// Whether holding it repeats: the keys that step a level up or down. Toggles (mute)
    /// and media keys don't.
    pub const fn repeats(self) -> bool {
        matches!(
            self,
            Self::VolumeUp
                | Self::VolumeDown
                | Self::BrightnessUp
                | Self::BrightnessDown
                | Self::KeyboardBrightnessUp
                | Self::KeyboardBrightnessDown
        )
    }
}

/// What happened to a [`SystemKey`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemKeyState {
    Pressed,
    /// Held past the keyboard's repeat delay; again at its rate until released.
    Repeated,
    Released,
}

/// Modifiers of a key chord for [`ShellRequest::SendShortcut`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "four independent modifiers, as in the protocol's bitfield"
)]
pub struct ChordModifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    /// The Super key (⌘ on neueOS, Q1).
    pub logo: bool,
}

/// What the shell asks the compositor to do (`neue_shell_v1`). Ignored when the
/// compositor doesn't offer the protocol to this client ([`crate::Ctx::has_shell`]), and
/// for windows that have closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellRequest {
    /// Raise and focus the window, restoring it if it's minimised.
    Activate(ToplevelId),
    /// Minimise the window and its dialogs.
    Minimize(ToplevelId),
    Unminimize(ToplevelId),
    /// Ask the window to close, as its close button does.
    Close(ToplevelId),
    /// Run a compositor action, as `neuectl action` does. `window: None` acts on the
    /// active window, or on none for actions that take no window. A failure comes back as
    /// [`ShellEvent::ActionFailed`].
    RunAction {
        name: String,
        window: Option<ToplevelId>,
    },
    /// Type a key chord into the active window (the fallback Edit menu). `keysym` is an
    /// XKB keysym (`0x63` for c), so it works in any layout.
    SendShortcut {
        modifiers: ChordModifiers,
        keysym: u32,
    },
    /// Where `toplevel` minimises to and restores from: `rect` (x, y, width, height in
    /// logical pixels) on the layer surface of the app's window `surface`, the dock tile of
    /// its app. It follows the surface when that moves. `None` unsets it. Dropped when
    /// `surface` isn't open.
    SetMinimizeRectangle {
        toplevel: ToplevelId,
        surface: crate::WindowId,
        rect: Option<(i32, i32, i32, i32)>,
    },
    /// Keep sending [`ShellEvent::Thumbnail`]s of `toplevel`, each scaled down to fit in
    /// `max_side` pixels square (never up); `None` stops them. Asking again changes the
    /// size. They stop on their own when the window closes, and never come when the
    /// compositor can't capture windows.
    Thumbnail {
        toplevel: ToplevelId,
        max_side: Option<u32>,
    },
}
