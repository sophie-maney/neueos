//! Headless backend: a virtual output and no input devices. Used by tests.
//!
//! There is no renderer yet: clients draw into shm buffers, which the compositor tracks
//! without reading them, so window management can be tested anywhere. When pixels are
//! needed (screencopy goldens, task 1.16) this backend gains an optional GLES renderer on
//! an EGL surfaceless/device display. Until then no linux-dmabuf global is advertised, so
//! clients always fall back to shm.
//!
//! A repeating timer stands in for vblank and sends frame callbacks, so real clients that
//! wait for `wl_surface.frame` keep drawing.

use std::time::Duration;

use smithay::output::{Mode, Output, PhysicalProperties, Subpixel};
use smithay::reexports::calloop::LoopHandle;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::wayland_server::DisplayHandle;
use tracing::info;

use crate::Error;
use crate::state::{Backend, State};

/// Size of the virtual output until outputs are configurable (task 1.7).
pub const OUTPUT_SIZE: (i32, i32) = (1920, 1080);
/// Refresh rate of the virtual output, in mHz.
const REFRESH_MHZ: i32 = 60_000;

/// State owned by the headless backend.
#[derive(Debug)]
pub struct HeadlessData {
    output: Output,
}

impl HeadlessData {
    pub fn output(&self) -> &Output {
        &self.output
    }
}

/// Create the virtual output.
pub fn init(dh: &DisplayHandle) -> Backend {
    let mode = Mode {
        size: OUTPUT_SIZE.into(),
        refresh: REFRESH_MHZ,
    };
    let output = Output::new(
        "HEADLESS-1".to_owned(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "neueOS".into(),
            model: "Headless".into(),
            serial_number: "0".into(),
        },
    );
    // The output global lives as long as the display; we never remove it.
    let _global = output.create_global::<State>(dh);
    output.change_current_state(Some(mode), None, None, Some((0, 0).into()));
    output.set_preferred(mode);
    info!(size = ?mode.size, "headless backend ready");
    Backend::Headless(HeadlessData { output })
}

/// Start the vblank timer that sends frame callbacks.
pub fn start_frame_timer(handle: &LoopHandle<'static, State>) -> Result<(), Error> {
    let interval = Duration::from_micros(1_000_000_000 / REFRESH_MHZ as u64);
    handle
        .insert_source(Timer::from_duration(interval), move |_, _, state| {
            send_frames(state);
            TimeoutAction::ToDuration(interval)
        })
        .map(|_| ())
        .map_err(|e| e.error)
        .map_err(Error::init("adding the headless frame timer"))
}

fn send_frames(state: &mut State) {
    // Only the headless backend starts this timer.
    let output = state.backend.output();
    let time = state.start_time.elapsed();
    for window in state.space.elements() {
        window.send_frame(output, time, Some(Duration::ZERO), |_, _| {
            Some(output.clone())
        });
    }
}
