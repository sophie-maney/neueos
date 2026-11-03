//! Headless backend: virtual outputs and no input devices. Used by tests.
//!
//! It starts with one output, [`OUTPUT_NAME`]; tests add, remove and resize more with
//! `State::headless_*` to exercise layout and hotplug.
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
use tracing::info;

use crate::Error;
use crate::state::{Backend, State};

/// Size of the virtual output the headless backend starts with.
pub const OUTPUT_SIZE: (i32, i32) = (1920, 1080);
/// Name of the virtual output the headless backend starts with.
pub const OUTPUT_NAME: &str = "HEADLESS-1";
/// Refresh rate of the virtual outputs, in mHz.
const REFRESH_MHZ: i32 = 60_000;

/// State owned by the headless backend.
#[derive(Debug)]
pub struct HeadlessData {
    outputs: Vec<Output>,
}

impl HeadlessData {
    pub fn outputs(&self) -> &[Output] {
        &self.outputs
    }
}

/// A virtual output called `name` with a `size` mode.
fn virtual_output(name: &str, size: (i32, i32)) -> Output {
    let mode = Mode {
        size: size.into(),
        refresh: REFRESH_MHZ,
    };
    let output = Output::new(
        name.to_owned(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "neueOS".into(),
            model: "Headless".into(),
            serial_number: "0".into(),
        },
    );
    output.change_current_state(Some(mode), None, None, None);
    output.set_preferred(mode);
    output
}

/// Create the first virtual output.
pub fn init() -> Backend {
    info!(size = ?OUTPUT_SIZE, "headless backend ready");
    Backend::Headless(HeadlessData {
        outputs: vec![virtual_output(OUTPUT_NAME, OUTPUT_SIZE)],
    })
}

/// Simulated hotplug, for tests. Each fails with a message on a non-headless backend or
/// an unknown (or, when adding, duplicate) output name.
impl State {
    /// Connect a virtual output called `name` with a `size` mode.
    pub fn headless_add_output(&mut self, name: &str, size: (i32, i32)) -> Result<(), String> {
        let Backend::Headless(data) = &mut self.backend else {
            return Err("not the headless backend".into());
        };
        if data.outputs.iter().any(|o| o.name() == name) {
            return Err(format!("output {name} exists"));
        }
        let output = virtual_output(name, size);
        data.outputs.push(output.clone());
        self.add_output(output);
        Ok(())
    }

    /// Disconnect the virtual output called `name`.
    pub fn headless_remove_output(&mut self, name: &str) -> Result<(), String> {
        let Backend::Headless(data) = &mut self.backend else {
            return Err("not the headless backend".into());
        };
        let index = data
            .outputs
            .iter()
            .position(|o| o.name() == name)
            .ok_or_else(|| format!("no output {name}"))?;
        let output = data.outputs.remove(index);
        self.remove_output(&output);
        Ok(())
    }

    /// Give the virtual output called `name` a new mode of `size`, like a monitor whose
    /// preferred mode changed (or a resized VM window).
    pub fn headless_set_mode(&mut self, name: &str, size: (i32, i32)) -> Result<(), String> {
        let Backend::Headless(data) = &self.backend else {
            return Err("not the headless backend".into());
        };
        let output = data
            .outputs
            .iter()
            .find(|o| o.name() == name)
            .cloned()
            .ok_or_else(|| format!("no output {name}"))?;
        let mode = Mode {
            size: size.into(),
            refresh: REFRESH_MHZ,
        };
        output.change_current_state(Some(mode), None, None, None);
        output.set_preferred(mode);
        self.output_mode_changed(&output);
        Ok(())
    }
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
    let time = state.start_time.elapsed();
    for output in state.outputs() {
        crate::outputs::send_frames(&state.space, output, time);
    }
}
