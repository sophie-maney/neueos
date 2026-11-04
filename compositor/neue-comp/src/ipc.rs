//! The `neuectl` socket (ADR 0003, ADR 0007): listener, per-connection buffers, request
//! handlers and event broadcast.
//!
//! Everything runs on the compositor's calloop thread and never blocks: the listener and
//! each connection are non-blocking sources. Connections are registered edge-triggered for
//! read and write, so a callback always drains reads to `WouldBlock` and flushes as much of
//! its write buffer as the socket accepts; the next writable edge continues a partial
//! write. A peer whose unsent output exceeds [`MAX_LINE_BYTES`] is disconnected.
//!
//! Slow work (task 1.16's screenshots) will run on a worker thread and answer through a
//! calloop channel; nothing in 1.3 needs it.

use std::collections::{HashMap, HashSet};
use std::io::{self, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

use neue_ipc::{
    ActionInfo, ActionParams, ErrorBody, ErrorCode, Event, LineBuffer, MAX_LINE_BYTES, Outcome,
    OutputInfo, PROTOCOL_VERSION, Reply, Request, Topic, VersionInfo, WindowArg, WindowId,
    encode_line, parse_request,
};
use serde_json::Value;
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::{Interest, LoopHandle, Mode, PostAction, RegistrationToken};
use tracing::{debug, info, warn};

use crate::state::State;
use crate::{Error, NAME};

/// Where the `neuectl` socket goes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum IpcSocket {
    /// `$XDG_RUNTIME_DIR/neueos/comp.<wayland socket>.sock`, but only when the compositor
    /// listens on a Wayland socket.
    #[default]
    Auto,
    /// No `neuectl` socket.
    Off,
    /// Exactly this path (tests).
    Path(PathBuf),
}

/// The actions `neuectl action` accepts: name, description, window argument and the
/// other required arguments.
const ACTIONS: &[(&str, &str, WindowArg, &[&str])] = &[
    ("quit", "Stop the compositor", WindowArg::None, &[]),
    (
        "reload-config",
        "Re-read compositor.toml now",
        WindowArg::None,
        &[],
    ),
    (
        "close-window",
        "Ask a window to close",
        WindowArg::Optional,
        &[],
    ),
    (
        "focus-window",
        "Raise a window and give it keyboard focus (restores it if minimised)",
        WindowArg::Required,
        &[],
    ),
    (
        "minimize-window",
        "Hide a window and its dialogs",
        WindowArg::Optional,
        &[],
    ),
    (
        "unminimize-window",
        "Show a minimised window again and focus it",
        WindowArg::Required,
        &[],
    ),
    (
        "zoom-window",
        "Toggle between filling the work area and the previous size",
        WindowArg::Optional,
        &[],
    ),
    (
        "fullscreen-window",
        "Toggle between filling the output (no title bar) and the previous size",
        WindowArg::Optional,
        &[],
    ),
    (
        "move-window",
        "Move a window's top-left corner to x, y",
        WindowArg::Optional,
        &["x", "y"],
    ),
    (
        "resize-window",
        "Ask a window to take width x height",
        WindowArg::Optional,
        &["width", "height"],
    ),
];

/// One client of the socket.
#[derive(Debug)]
struct Connection {
    /// Our handle on the socket (a dup of the one inside the event source; both share the
    /// open file description, hence the non-blocking flag).
    stream: UnixStream,
    token: RegistrationToken,
    input: LineBuffer,
    /// Bytes not yet accepted by the socket.
    output: Vec<u8>,
    /// Set by `subscribe`: the connection then only receives these topics' events.
    topics: Option<HashSet<Topic>>,
}

impl Connection {
    /// Queue `bytes`. `false` if the peer is too far behind and must be dropped.
    fn queue(&mut self, bytes: &[u8]) -> bool {
        self.output.extend_from_slice(bytes);
        self.output.len() <= MAX_LINE_BYTES
    }

    /// Write as much queued output as the socket takes. `false` if the connection broke.
    fn flush(&mut self) -> bool {
        while !self.output.is_empty() {
            match self.stream.write(&self.output) {
                Ok(0) => return false,
                Ok(n) => {
                    self.output.drain(..n);
                }
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => return true,
                Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => return false,
            }
        }
        true
    }
}

/// The socket and its connections. Part of [`State`].
#[derive(Debug)]
pub struct IpcServer {
    handle: LoopHandle<'static, State>,
    /// Socket file, once listening; removed again on drop.
    path: Option<PathBuf>,
    connections: HashMap<u64, Connection>,
    next_connection: u64,
    /// The last focus change sent as an event, to suppress duplicates.
    pub(crate) reported_focus: Option<WindowId>,
}

impl IpcServer {
    pub fn new(handle: LoopHandle<'static, State>) -> Self {
        Self {
            handle,
            path: None,
            connections: HashMap::new(),
            next_connection: 0,
            reported_focus: None,
        }
    }

    /// The socket path, exported to children as `NEUE_SOCKET`.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Create the socket at `path` (mode 0600 in a 0700 directory) and start accepting.
    pub fn listen(&mut self, path: &Path) -> Result<(), Error> {
        let listener = bind(path).map_err(Error::init("creating the neuectl socket"))?;
        self.handle
            .insert_source(
                Generic::new(listener, Interest::READ, Mode::Level),
                |_, listener, state: &mut State| {
                    loop {
                        match listener.accept() {
                            Ok((stream, _)) => {
                                if let Err(err) = state.ipc_accept(stream) {
                                    warn!(%err, "failed to accept a neuectl client");
                                }
                            }
                            Err(err) if err.kind() == io::ErrorKind::WouldBlock => break,
                            Err(err) => {
                                warn!(%err, "accepting a neuectl client failed");
                                break;
                            }
                        }
                    }
                    Ok(PostAction::Continue)
                },
            )
            .map_err(|e| e.error)
            .map_err(Error::init("adding the neuectl socket source"))?;
        self.path = Some(path.to_owned());
        info!(path = %path.display(), "neuectl socket ready");
        Ok(())
    }

    /// Send `event` to every connection subscribed to its topic. Connections that fall
    /// too far behind are dropped.
    pub fn broadcast(&mut self, event: &Event) {
        let topic = event.topic();
        let line = encode_line(event);
        let mut dead = Vec::new();
        for (&id, conn) in &mut self.connections {
            if conn.topics.as_ref().is_some_and(|t| t.contains(&topic))
                && !(conn.queue(&line) && conn.flush())
            {
                dead.push(id);
            }
        }
        for id in dead {
            debug!(id, "dropping a slow or broken neuectl subscriber");
            if let Some(conn) = self.connections.remove(&id) {
                self.handle.remove(conn.token);
            }
        }
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        if let Some(path) = &self.path {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Bind the listening socket, replacing a stale socket file but refusing to steal one that
/// another compositor still serves.
fn bind(path: &Path) -> io::Result<UnixListener> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    if path.exists() {
        if UnixStream::connect(path).is_ok() {
            return Err(io::Error::new(
                io::ErrorKind::AddrInUse,
                format!("{} is served by another compositor", path.display()),
            ));
        }
        std::fs::remove_file(path)?;
    }
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}

impl State {
    /// Register a freshly accepted connection.
    fn ipc_accept(&mut self, stream: UnixStream) -> io::Result<()> {
        stream.set_nonblocking(true)?;
        let ours = stream.try_clone()?;
        let id = self.ipc.next_connection;
        self.ipc.next_connection += 1;
        let handle = self.ipc.handle.clone();
        let token = handle
            .insert_source(
                Generic::new(stream, Interest::BOTH, Mode::Edge),
                move |_, _, state: &mut State| {
                    Ok(if state.ipc_ready(id) {
                        PostAction::Continue
                    } else {
                        PostAction::Remove
                    })
                },
            )
            .map_err(|e| io::Error::other(e.error))?;
        self.ipc.connections.insert(
            id,
            Connection {
                stream: ours,
                token,
                input: LineBuffer::new(),
                output: Vec::new(),
                topics: None,
            },
        );
        debug!(id, "neuectl client connected");
        Ok(())
    }

    /// A connection became readable or writable. `false` means drop it.
    fn ipc_ready(&mut self, id: u64) -> bool {
        // Take the connection out while handling requests: handlers need `&mut State`, and
        // events they cause skip it (a connection that sends requests is not subscribed).
        let Some(mut conn) = self.ipc.connections.remove(&id) else {
            return false;
        };
        let alive = self.ipc_read(&mut conn) && conn.flush();
        if alive {
            self.ipc.connections.insert(id, conn);
        } else {
            debug!(id, "neuectl client disconnected");
        }
        alive
    }

    /// Read until `WouldBlock`, answering each complete line. `false` on EOF or error.
    fn ipc_read(&mut self, conn: &mut Connection) -> bool {
        let mut chunk = [0u8; 4096];
        loop {
            match conn.stream.read(&mut chunk) {
                Ok(0) => return false,
                Ok(n) => conn.input.push(&chunk[..n]),
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => return true,
                Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => return false,
            }
            loop {
                match conn.input.pop_line() {
                    Ok(Some(line)) => {
                        // Event mode carries nothing from the client; discard it.
                        if conn.topics.is_none() {
                            let reply = self.ipc_answer(conn, &line);
                            if !conn.queue(&encode_line(&reply)) {
                                return false;
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(_) => {
                        let reply = error_reply(
                            None,
                            ErrorCode::LineTooLong,
                            format!("request line exceeds {MAX_LINE_BYTES} bytes"),
                        );
                        conn.queue(&encode_line(&reply));
                        conn.flush();
                        return false;
                    }
                }
            }
        }
    }

    /// Answer one request line.
    fn ipc_answer(&mut self, conn: &mut Connection, line: &[u8]) -> Reply {
        let envelope = match parse_request(line) {
            Ok(envelope) => envelope,
            Err(reply) => return reply,
        };
        let outcome = match envelope.request {
            Request::Subscribe(params) => {
                conn.topics = Some(params.topics.into_iter().collect());
                Ok(Value::Null)
            }
            request => self.ipc_request(request),
        };
        Reply {
            id: Some(envelope.id),
            outcome: match outcome {
                Ok(value) => Outcome::Ok(value),
                Err(body) => Outcome::Error(body),
            },
        }
    }

    /// Run a request (other than `subscribe`) and return its payload.
    fn ipc_request(&mut self, request: Request) -> Result<Value, ErrorBody> {
        match request {
            Request::Version => json(&VersionInfo {
                compositor: NAME.to_owned(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
                protocol: PROTOCOL_VERSION,
            }),
            Request::Windows => json(&self.windows()),
            Request::Outputs => json(&self.outputs_info()),
            Request::FocusedWindow => json(&self.windows().into_iter().find(|w| w.focused)),
            Request::Actions => json(
                &ACTIONS
                    .iter()
                    .map(|&(name, description, window, args)| ActionInfo {
                        name: name.to_owned(),
                        description: description.to_owned(),
                        window,
                        args: args.iter().map(|&a| a.to_owned()).collect(),
                    })
                    .collect::<Vec<_>>(),
            ),
            Request::Action(params) => self.run_action(params).map(|()| Value::Null),
            Request::ReloadConfig => self
                .reload_config()
                .map(|()| Value::Null)
                .map_err(|info| ErrorBody::new(ErrorCode::ConfigError, info.to_string())),
            Request::Quit => {
                info!("quit requested over neuectl");
                self.loop_signal.stop();
                Ok(Value::Null)
            }
            // Handled by the caller, which owns the connection.
            Request::Subscribe(_) => Ok(Value::Null),
        }
    }

    pub(crate) fn run_action(&mut self, params: ActionParams) -> Result<(), ErrorBody> {
        match params.name.as_str() {
            "quit" => {
                self.loop_signal.stop();
                Ok(())
            }
            "reload-config" => self
                .reload_config()
                .map_err(|info| ErrorBody::new(ErrorCode::ConfigError, info.to_string())),
            "close-window" => {
                let window = self.window_arg(params.window, WindowArg::Optional)?;
                if let Some(toplevel) = window.toplevel() {
                    toplevel.send_close();
                }
                Ok(())
            }
            "focus-window" => {
                let window = self.window_arg(params.window, WindowArg::Required)?;
                self.focus_window(&window);
                Ok(())
            }
            "minimize-window" => {
                let window = self.window_arg(params.window, WindowArg::Optional)?;
                self.minimize_window(&window);
                Ok(())
            }
            "unminimize-window" => {
                let window = self.window_arg(params.window, WindowArg::Required)?;
                self.unminimize_window(&window);
                Ok(())
            }
            "zoom-window" => {
                let window = self.window_arg(params.window, WindowArg::Optional)?;
                let zoomed = Self::with_wm_data(&window, |d| d.zoomed);
                self.set_zoomed(&window, !zoomed, None);
                Ok(())
            }
            "fullscreen-window" => {
                let window = self.window_arg(params.window, WindowArg::Optional)?;
                if Self::is_fullscreen(&window) {
                    self.unset_fullscreen(&window);
                } else {
                    self.set_fullscreen(&window, None);
                }
                Ok(())
            }
            "move-window" => {
                let (x, y) = (required(params.x, "x")?, required(params.y, "y")?);
                let window = self.window_arg(params.window, WindowArg::Optional)?;
                if Self::is_fullscreen(&window) {
                    self.unset_fullscreen(&window);
                }
                if Self::with_wm_data(&window, |d| d.zoomed) {
                    self.set_zoomed(&window, false, None);
                }
                self.move_window_to(&window, (x, y).into());
                self.notify_window_changed(&window);
                Ok(())
            }
            "resize-window" => {
                let width = required(params.width, "width")?;
                let height = required(params.height, "height")?;
                if width <= 0 || height <= 0 {
                    return Err(ErrorBody::new(
                        ErrorCode::InvalidParams,
                        "width and height must be positive",
                    ));
                }
                let window = self.window_arg(params.window, WindowArg::Optional)?;
                self.resize_window(&window, (width, height).into());
                Ok(())
            }
            other => Err(ErrorBody::new(
                ErrorCode::UnknownAction,
                format!("unknown action `{other}` (see `neuectl actions`)"),
            )),
        }
    }

    /// Resolve an action's window argument.
    fn window_arg(
        &self,
        id: Option<WindowId>,
        arg: WindowArg,
    ) -> Result<smithay::desktop::Window, ErrorBody> {
        let id = match (id, arg) {
            (Some(id), _) => id,
            (None, WindowArg::Optional) => self
                .focused_window_id()
                .ok_or_else(|| ErrorBody::new(ErrorCode::NoWindow, "no window is focused"))?,
            (None, _) => {
                return Err(ErrorBody::new(
                    ErrorCode::NoWindow,
                    "this action needs a window id",
                ));
            }
        };
        self.window_by_id(id).ok_or_else(|| {
            ErrorBody::new(ErrorCode::UnknownWindow, format!("no window with id {id}"))
        })
    }

    /// The outputs, for `neuectl outputs`.
    fn outputs_info(&self) -> Vec<OutputInfo> {
        self.outputs().map(|o| self.output_info(o)).collect()
    }

    /// Emit `focus_changed` if `window` differs from the last reported focus.
    pub(crate) fn report_focus(&mut self, window: Option<WindowId>) {
        if window != self.ipc.reported_focus {
            self.ipc.reported_focus = window;
            self.emit(Event::FocusChanged { window });
        }
    }

    /// Send `event` to subscribers.
    pub(crate) fn emit(&mut self, event: Event) {
        self.ipc.broadcast(&event);
    }
}

/// Whether `name` is an action `neuectl action` (and `[keybindings]`) accepts.
pub(crate) fn is_action(name: &str) -> bool {
    ACTIONS.iter().any(|&(n, ..)| n == name)
}

/// An action argument that must be present.
fn required(value: Option<i32>, name: &str) -> Result<i32, ErrorBody> {
    value.ok_or_else(|| {
        ErrorBody::new(
            ErrorCode::InvalidParams,
            format!("this action needs `{name}`"),
        )
    })
}

fn json<T: serde::Serialize>(value: &T) -> Result<Value, ErrorBody> {
    serde_json::to_value(value).map_err(|err| {
        ErrorBody::new(
            ErrorCode::Internal,
            format!("encoding the reply failed: {err}"),
        )
    })
}

fn error_reply(id: Option<u64>, code: ErrorCode, message: String) -> Reply {
    Reply {
        id,
        outcome: Outcome::Error(ErrorBody::new(code, message)),
    }
}
