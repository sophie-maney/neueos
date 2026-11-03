//! Request, reply and event types of the `neuectl` protocol (ADR 0003, ADR 0007).
//!
//! Changes are additive: new requests, fields, events and error codes. Clients must ignore
//! unknown fields, so none of these types use `deny_unknown_fields`.

use serde::{Deserialize, Serialize};

/// Protocol integer returned by `version`. Bumped only for breaking changes.
pub const PROTOCOL_VERSION: u32 = 1;

/// Identifies a window for the lifetime of the compositor process. Ids are never reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WindowId(pub u64);

impl std::fmt::Display for WindowId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// A request, without its `id` (see [`RequestEnvelope`]).
///
/// On the wire: `{"request": "windows"}` or
/// `{"request": "action", "params": {"name": "close-window", "window": 42}}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "request", content = "params", rename_all = "kebab-case")]
pub enum Request {
    /// Compositor version and protocol integer. Reply: [`VersionInfo`].
    Version,
    /// Mapped windows, bottom to top. Reply: `Vec<WindowInfo>`.
    Windows,
    /// Outputs. Reply: `Vec<OutputInfo>`.
    Outputs,
    /// The window with keyboard focus. Reply: `Option<WindowInfo>`.
    FocusedWindow,
    /// Named actions that `action` accepts. Reply: `Vec<ActionInfo>`.
    Actions,
    /// Run a named action. Reply: `null`.
    Action(ActionParams),
    /// Switch the connection to event mode. Reply: `null`, then only event lines.
    Subscribe(SubscribeParams),
    /// Re-read the config files now (ADR 0004). Reply: `null` once the new config is
    /// active, or a `config_error` error naming the file, line and column; the old config
    /// stays active in that case.
    ReloadConfig,
    /// Stop the compositor. Reply: `null`, sent before the compositor exits.
    Quit,
}

/// A request with the client-chosen `id` that the reply echoes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestEnvelope {
    pub id: u64,
    #[serde(flatten)]
    pub request: Request,
}

/// Parameters of [`Request::Action`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionParams {
    pub name: String,
    /// Target window. Actions that take a window default to the focused one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<WindowId>,
    /// New position in global logical coordinates (`move-window`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<i32>,
    /// New size in logical pixels (`resize-window`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<i32>,
}

/// Parameters of [`Request::Subscribe`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscribeParams {
    pub topics: Vec<Topic>,
}

/// An event source a client can subscribe to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Topic {
    /// [`Event::WindowOpened`], [`Event::WindowChanged`], [`Event::WindowClosed`] and
    /// [`Event::FocusChanged`].
    Windows,
    /// [`Event::ConfigError`] and [`Event::ConfigReloaded`].
    Config,
}

impl Event {
    /// The topic a client must subscribe to in order to receive this event.
    pub fn topic(&self) -> Topic {
        match self {
            Self::WindowOpened(_)
            | Self::WindowChanged(_)
            | Self::WindowClosed { .. }
            | Self::FocusChanged { .. } => Topic::Windows,
            Self::ConfigError(_) | Self::ConfigReloaded { .. } => Topic::Config,
        }
    }
}

/// An event line: `{"event": "window_closed", "data": {"window": 3}}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub enum Event {
    /// A window was mapped.
    WindowOpened(WindowInfo),
    /// Something in a window's [`WindowInfo`] other than `focused` changed: its title,
    /// geometry, or minimised, zoomed or attention state. Sent once the change is done
    /// (at the end of an interactive move, not for every pointer motion).
    WindowChanged(WindowInfo),
    /// A window was unmapped or destroyed.
    WindowClosed { window: WindowId },
    /// Keyboard focus moved. `window` is `None` when nothing is focused.
    FocusChanged { window: Option<WindowId> },
    /// A config file was rejected and the previous config stays active (ADR 0004). Sent for
    /// every failed reload, from the file watcher or `reload-config`.
    ConfigError(ConfigErrorInfo),
    /// A config file was (re)loaded and applied. Clears an earlier `config_error`.
    ConfigReloaded {
        /// File name, e.g. `compositor.toml`.
        file: String,
    },
}

/// Where and why a config file was rejected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigErrorInfo {
    /// Path of the offending file (one of the config layers).
    pub file: String,
    /// 1-based position, when the parser knows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
    pub message: String,
}

impl std::fmt::Display for ConfigErrorInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.file)?;
        if let (Some(line), Some(column)) = (self.line, self.column) {
            write!(f, ":{line}:{column}")?;
        }
        write!(f, ": {}", self.message)
    }
}

/// Error codes. They are `snake_case` strings on the wire and stable once shipped; a code
/// this client does not know deserializes as [`ErrorCode::Other`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// The line was not valid JSON, or had no usable `id` or `request`.
    InvalidRequest,
    /// The `request` name is not known.
    UnknownRequest,
    /// The request is known but its `params` are missing or malformed.
    InvalidParams,
    /// `action` named an action that does not exist.
    UnknownAction,
    /// A window id does not name a mapped window.
    UnknownWindow,
    /// The action needs a window and none was given or focused.
    NoWindow,
    /// The line exceeded [`crate::MAX_LINE_BYTES`].
    LineTooLong,
    /// `reload-config` found a config file it cannot use; the message has the file, line
    /// and column.
    ConfigError,
    /// The compositor failed in a way that is not the client's fault.
    Internal,
    /// A code added by a newer compositor.
    #[serde(other)]
    Other,
}

/// The body of an error reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
}

impl ErrorBody {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// The outcome half of a reply line: `{"ok": …}` or `{"error": …}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Ok(serde_json::Value),
    Error(ErrorBody),
}

/// A reply line. `id` is `null` when the request line could not be parsed far enough to
/// find one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reply {
    pub id: Option<u64>,
    #[serde(flatten)]
    pub outcome: Outcome,
}

/// Payload of `version`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionInfo {
    /// Compositor name, e.g. `neue-comp`.
    pub compositor: String,
    /// Compositor version (semver).
    pub version: String,
    /// Protocol integer ([`PROTOCOL_VERSION`] at build time).
    pub protocol: u32,
}

/// A mapped window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowInfo {
    pub id: WindowId,
    pub title: Option<String>,
    pub app_id: Option<String>,
    /// Position of the window geometry in global logical coordinates.
    pub x: i32,
    pub y: i32,
    /// Size of the window geometry (without client-side shadows).
    pub width: i32,
    pub height: i32,
    /// Whether the window has keyboard focus.
    pub focused: bool,
    /// Minimised windows are hidden and keep the position they had (task 1.5).
    #[serde(default)]
    pub minimized: bool,
    /// Zoomed windows fill the work area (`xdg_toplevel` maximized state).
    #[serde(default)]
    pub zoomed: bool,
    /// The window asked for focus and was refused (xdg-activation). Cleared when it is
    /// focused. The dock will bounce its icon.
    #[serde(default)]
    pub demands_attention: bool,
    /// The window this one is a child of (`xdg_toplevel.set_parent`), e.g. a dialog.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<WindowId>,
}

/// An output (monitor).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputInfo {
    pub name: String,
    pub make: String,
    pub model: String,
    /// Position in global logical coordinates.
    pub x: i32,
    pub y: i32,
    /// Current mode in physical pixels.
    pub width: i32,
    pub height: i32,
    /// Refresh rate in mHz (60 Hz is 60000).
    pub refresh_mhz: i32,
    pub scale: f64,
}

/// Whether and how an action takes a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowArg {
    /// The action ignores windows.
    None,
    /// Defaults to the focused window.
    Optional,
    /// A window id must be given.
    Required,
}

/// An entry of `actions`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionInfo {
    pub name: String,
    pub description: String,
    pub window: WindowArg,
    /// Required [`ActionParams`] fields besides `window`, e.g. `["x", "y"]`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(value: serde_json::Value) -> Result<RequestEnvelope, serde_json::Error> {
        serde_json::from_value(value)
    }

    #[test]
    fn unit_request_without_params() {
        let env = parse(json!({"id": 1, "request": "windows"})).unwrap();
        assert_eq!(env.id, 1);
        assert_eq!(env.request, Request::Windows);
        let env = parse(json!({"id": 2, "request": "focused-window"})).unwrap();
        assert_eq!(env.request, Request::FocusedWindow);
    }

    #[test]
    fn action_request_matches_adr_example() {
        let env = parse(json!({
            "id": 2,
            "request": "action",
            "params": {"name": "close-window", "window": 42}
        }))
        .unwrap();
        assert_eq!(
            env.request,
            Request::Action(ActionParams {
                name: "close-window".into(),
                window: Some(WindowId(42)),
                ..ActionParams::default()
            })
        );
    }

    #[test]
    fn action_window_is_optional() {
        let env = parse(json!({"id": 1, "request": "action", "params": {"name": "quit"}})).unwrap();
        assert_eq!(
            env.request,
            Request::Action(ActionParams {
                name: "quit".into(),
                ..ActionParams::default()
            })
        );
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let env = parse(json!({
            "id": 1, "request": "action", "params": {"name": "x", "future": true}, "extra": 1
        }))
        .unwrap();
        assert!(matches!(env.request, Request::Action(_)));
    }

    #[test]
    fn unknown_request_and_missing_params_fail() {
        assert!(parse(json!({"id": 1, "request": "nope"})).is_err());
        assert!(parse(json!({"id": 1, "request": "action"})).is_err());
        assert!(parse(json!({"request": "windows"})).is_err());
    }

    #[test]
    fn request_round_trips() {
        let requests = [
            Request::Version,
            Request::Quit,
            Request::ReloadConfig,
            Request::Subscribe(SubscribeParams {
                topics: vec![Topic::Windows],
            }),
            Request::Action(ActionParams {
                name: "focus-window".into(),
                window: Some(WindowId(7)),
                ..ActionParams::default()
            }),
            Request::Action(ActionParams {
                name: "move-window".into(),
                x: Some(-10),
                y: Some(20),
                ..ActionParams::default()
            }),
        ];
        for request in requests {
            let env = RequestEnvelope { id: 9, request };
            let text = serde_json::to_string(&env).unwrap();
            assert_eq!(serde_json::from_str::<RequestEnvelope>(&text).unwrap(), env);
        }
    }

    #[test]
    fn unit_request_serializes_without_params() {
        let env = RequestEnvelope {
            id: 1,
            request: Request::Windows,
        };
        assert_eq!(
            serde_json::to_value(&env).unwrap(),
            json!({"id": 1, "request": "windows"})
        );
    }

    #[test]
    fn replies_match_adr_shape() {
        let ok = Reply {
            id: Some(1),
            outcome: Outcome::Ok(json!([])),
        };
        assert_eq!(
            serde_json::to_value(&ok).unwrap(),
            json!({"id": 1, "ok": []})
        );
        let err = Reply {
            id: Some(2),
            outcome: Outcome::Error(ErrorBody::new(ErrorCode::UnknownWindow, "no window 5")),
        };
        assert_eq!(
            serde_json::to_value(&err).unwrap(),
            json!({"id": 2, "error": {"code": "unknown_window", "message": "no window 5"}})
        );
        let back: Reply = serde_json::from_value(serde_json::to_value(&err).unwrap()).unwrap();
        assert_eq!(back, err);
    }

    #[test]
    fn ok_null_survives_a_round_trip() {
        let reply = Reply {
            id: Some(3),
            outcome: Outcome::Ok(serde_json::Value::Null),
        };
        let text = serde_json::to_string(&reply).unwrap();
        assert_eq!(serde_json::from_str::<Reply>(&text).unwrap(), reply);
    }

    #[test]
    fn unknown_error_code_is_other() {
        let reply: Reply = serde_json::from_value(
            json!({"id": 1, "error": {"code": "from_the_future", "message": "?"}}),
        )
        .unwrap();
        assert!(matches!(
            reply.outcome,
            Outcome::Error(ErrorBody {
                code: ErrorCode::Other,
                ..
            })
        ));
    }

    #[test]
    fn events_match_adr_shape() {
        let event = Event::WindowClosed {
            window: WindowId(3),
        };
        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            json!({"event": "window_closed", "data": {"window": 3}})
        );
        let focus = Event::FocusChanged { window: None };
        assert_eq!(
            serde_json::to_value(&focus).unwrap(),
            json!({"event": "focus_changed", "data": {"window": null}})
        );
        assert_eq!(focus.topic(), Topic::Windows);
        let error = Event::ConfigError(ConfigErrorInfo {
            file: "/etc/neueos/compositor.toml".into(),
            line: Some(3),
            column: Some(1),
            message: "bad".into(),
        });
        assert_eq!(
            serde_json::to_value(&error).unwrap(),
            json!({"event": "config_error", "data": {
                "file": "/etc/neueos/compositor.toml", "line": 3, "column": 1, "message": "bad"
            }})
        );
        assert_eq!(error.topic(), Topic::Config);
        assert_eq!(
            serde_json::to_value(Event::ConfigReloaded {
                file: "compositor.toml".into()
            })
            .unwrap(),
            json!({"event": "config_reloaded", "data": {"file": "compositor.toml"}})
        );
    }
}
