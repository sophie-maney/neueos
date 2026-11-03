//! Blocking client for the `neuectl` socket.

use std::io::{self, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use crate::codec::{encode_line, read_line};
use crate::types::{
    ActionInfo, ActionParams, ErrorBody, Event, Outcome, OutputInfo, Reply, Request,
    RequestEnvelope, SubscribeParams, Topic, VersionInfo, WindowId, WindowInfo,
};

/// Environment variable that holds the socket path. The compositor sets it for children.
pub const SOCKET_ENV: &str = "NEUE_SOCKET";

/// Directory (below `XDG_RUNTIME_DIR`) that holds the socket.
const SOCKET_DIR: &str = "neueos";

/// Socket path for a Wayland display name: `<runtime_dir>/neueos/comp.<display>.sock`.
pub fn socket_path_for_display(runtime_dir: &Path, wayland_display: &str) -> PathBuf {
    runtime_dir
        .join(SOCKET_DIR)
        .join(format!("comp.{wayland_display}.sock"))
}

/// The socket to talk to: `NEUE_SOCKET` if set, else derived from `XDG_RUNTIME_DIR` and
/// `WAYLAND_DISPLAY`. `None` if neither is available.
pub fn default_socket_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(SOCKET_ENV).filter(|p| !p.is_empty()) {
        return Some(path.into());
    }
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").filter(|p| !p.is_empty())?;
    let display = std::env::var("WAYLAND_DISPLAY")
        .ok()
        .filter(|d| !d.is_empty())?;
    Some(socket_path_for_display(Path::new(&runtime_dir), &display))
}

/// Why a client call failed.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// Neither `NEUE_SOCKET` nor `XDG_RUNTIME_DIR` + `WAYLAND_DISPLAY` is set.
    #[error("no socket path: set NEUE_SOCKET, or run inside a neueOS session")]
    NoSocketPath,
    #[error("socket I/O failed: {0}")]
    Io(#[from] io::Error),
    /// The compositor answered with an error reply.
    #[error("{}: {}", .0.code_name(), .0.message)]
    Remote(ErrorBody),
    /// The compositor sent something that is not valid protocol JSON.
    #[error("malformed message from the compositor: {0}")]
    Protocol(String),
    /// The compositor closed the connection.
    #[error("the compositor closed the connection")]
    Closed,
}

impl ErrorBody {
    /// The wire name of the code, for messages.
    fn code_name(&self) -> String {
        serde_json::to_value(self.code)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_else(|| "error".into())
    }
}

/// A connection that sends requests and waits for each reply.
#[derive(Debug)]
pub struct Client {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    next_id: u64,
}

impl Client {
    /// Connect to the socket at `path`.
    pub fn connect(path: impl AsRef<Path>) -> Result<Self, ClientError> {
        let stream = UnixStream::connect(path)?;
        let writer = stream.try_clone()?;
        Ok(Self {
            reader: BufReader::new(stream),
            writer,
            next_id: 1,
        })
    }

    /// Connect to [`default_socket_path`].
    pub fn connect_default() -> Result<Self, ClientError> {
        Self::connect(default_socket_path().ok_or(ClientError::NoSocketPath)?)
    }

    /// Send `request` and return the `ok` payload of its reply.
    pub fn call(&mut self, request: Request) -> Result<serde_json::Value, ClientError> {
        let id = self.next_id;
        self.next_id += 1;
        self.writer
            .write_all(&encode_line(&RequestEnvelope { id, request }))?;
        let line = read_line(&mut self.reader)?.ok_or(ClientError::Closed)?;
        let reply: Reply =
            serde_json::from_slice(&line).map_err(|err| ClientError::Protocol(err.to_string()))?;
        // Replies come in order, so an id mismatch means the stream is out of sync.
        if reply.id != Some(id) {
            // An error reply to a line the server could not parse carries no id; show it.
            if let (None, Outcome::Error(body)) = (reply.id, &reply.outcome) {
                return Err(ClientError::Remote(body.clone()));
            }
            return Err(ClientError::Protocol(format!(
                "reply id {:?} does not match request id {id}",
                reply.id
            )));
        }
        match reply.outcome {
            Outcome::Ok(value) => Ok(value),
            Outcome::Error(body) => Err(ClientError::Remote(body)),
        }
    }

    fn call_typed<T: serde::de::DeserializeOwned>(
        &mut self,
        request: Request,
    ) -> Result<T, ClientError> {
        serde_json::from_value(self.call(request)?)
            .map_err(|err| ClientError::Protocol(err.to_string()))
    }

    pub fn version(&mut self) -> Result<VersionInfo, ClientError> {
        self.call_typed(Request::Version)
    }

    pub fn windows(&mut self) -> Result<Vec<WindowInfo>, ClientError> {
        self.call_typed(Request::Windows)
    }

    pub fn outputs(&mut self) -> Result<Vec<OutputInfo>, ClientError> {
        self.call_typed(Request::Outputs)
    }

    pub fn focused_window(&mut self) -> Result<Option<WindowInfo>, ClientError> {
        self.call_typed(Request::FocusedWindow)
    }

    pub fn actions(&mut self) -> Result<Vec<ActionInfo>, ClientError> {
        self.call_typed(Request::Actions)
    }

    /// Run the named action, on `window` if given.
    pub fn action(&mut self, name: &str, window: Option<WindowId>) -> Result<(), ClientError> {
        self.action_with(ActionParams {
            name: name.to_owned(),
            window,
            ..ActionParams::default()
        })
    }

    /// Run an action with arguments (e.g. `move-window` with `x` and `y`).
    pub fn action_with(&mut self, params: ActionParams) -> Result<(), ClientError> {
        self.call(Request::Action(params)).map(drop)
    }

    /// Ask the compositor to exit.
    pub fn quit(&mut self) -> Result<(), ClientError> {
        self.call(Request::Quit).map(drop)
    }

    /// Subscribe to `topics`. The connection then carries only events, so this consumes
    /// the client.
    pub fn subscribe(mut self, topics: Vec<Topic>) -> Result<Subscription, ClientError> {
        self.call(Request::Subscribe(SubscribeParams { topics }))?;
        Ok(Subscription {
            reader: self.reader,
        })
    }
}

/// A connection in event mode. Iterating blocks until the next event and ends when the
/// compositor disconnects.
#[derive(Debug)]
pub struct Subscription {
    reader: BufReader<UnixStream>,
}

impl Subscription {
    /// The next event, or `Ok(None)` when the compositor closed the connection.
    pub fn next_event(&mut self) -> Result<Option<Event>, ClientError> {
        let Some(line) = read_line(&mut self.reader)? else {
            return Ok(None);
        };
        serde_json::from_slice(&line)
            .map(Some)
            .map_err(|err| ClientError::Protocol(err.to_string()))
    }

    /// Set a read timeout so [`Subscription::next_event`] fails with a timeout error
    /// instead of blocking forever (used by tests).
    pub fn set_read_timeout(&self, timeout: Option<std::time::Duration>) -> io::Result<()> {
        self.reader.get_ref().set_read_timeout(timeout)
    }
}

impl Iterator for Subscription {
    type Item = Result<Event, ClientError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_event().transpose()
    }
}
