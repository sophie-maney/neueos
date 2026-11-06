//! neueOS's PipeWire client (task 3.14b, ADR 0087).
//!
//! [`Audio::start`] runs a PipeWire main loop on its own thread. It follows the audio sinks
//! and sources, the session manager's `default` metadata (which sink and source are the
//! default), and each one's volume and mute, and reports a fresh [`State`] whenever any of
//! it changes. [`Audio`]'s methods queue changes to that thread: a volume, a mute, the
//! default device.
//!
//! Volumes are on the cubic scale `wpctl` and volume sliders use ([`cubic`]): 0.5 is an
//! eighth of full amplitude, which sounds about half as loud. The thread
//! reconnects when PipeWire restarts; until then the state is empty.

#[cfg(target_os = "linux")]
mod backend;
mod model;

pub use model::{Endpoint, Kind, State, cubic, linear};

#[cfg(target_os = "linux")]
pub use backend::Audio;
