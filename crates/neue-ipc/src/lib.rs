//! The `neuectl` wire protocol: serde types, newline-delimited JSON framing and a small
//! blocking client (ADR 0003, ADR 0007).
//!
//! The compositor (`neue-comp`) serves this protocol on a Unix socket; `neuectl`, tests and
//! scripts speak it. The crate is portable (builds on macOS) and has no async runtime.
//!
//! ```no_run
//! # fn main() -> Result<(), neue_ipc::ClientError> {
//! let mut client = neue_ipc::Client::connect_default()?;
//! for window in client.windows()? {
//!     println!("{} {:?}", window.id, window.title);
//! }
//! # Ok(()) }
//! ```

mod client;
mod codec;
mod types;

pub use client::{
    Client, ClientError, SOCKET_ENV, Subscription, default_socket_path, socket_path_for_display,
};
pub use codec::{LineBuffer, LineTooLong, MAX_LINE_BYTES, encode_line, parse_request, read_line};
pub use types::*;
