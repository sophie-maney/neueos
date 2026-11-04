//! Headless backend: virtual outputs and no input devices. Used by tests.
//!
//! It starts with one output, [`OUTPUT_NAME`]; tests add, remove and resize more with
//! `State::headless_*` to exercise layout and hotplug.
//!
//! Rendering is optional ([`crate::Options::headless_renderer`], ADR 0021). Without it,
//! clients draw into shm buffers that the compositor tracks without reading, so window
//! management can be tested anywhere. With it, a GLES renderer on Mesa's software EGL
//! device draws an output into an offscreen buffer on demand
//! ([`State::headless_render`]) and reads the pixels back. Either way no linux-dmabuf
//! global is advertised, so clients always use shm.
//!
//! A repeating timer stands in for vblank: it sends frame callbacks, so real clients that
//! wait for `wl_surface.frame` keep drawing, and presentation feedback (no flags: nothing
//! is really shown). It never renders.

use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

use smithay::backend::allocator::Fourcc;
use smithay::backend::egl::{EGLContext, EGLDevice, EGLDisplay};
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::gles::{GlesRenderbuffer, GlesRenderer};
use smithay::backend::renderer::{Bind, ExportMem, Offscreen};
use smithay::desktop::{Space, Window};
use smithay::output::{Mode, Output, PhysicalProperties, Subpixel};
use smithay::reexports::calloop::LoopHandle;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::wayland_protocols::wp::presentation_time::server::wp_presentation_feedback;
use smithay::utils::{Physical, Rectangle, Size};
use tracing::info;

use crate::Error;
use crate::decorations::Chrome;
use crate::render::CLEAR_COLOR;
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
    /// Ticks of the frame timer: the "vblank" sequence number of presentation feedback.
    sequence: u64,
    /// Offscreen rendering, when [`crate::Options::headless_renderer`] asked for it.
    /// Boxed: it holds the GL context and is far larger than the rest.
    renderer: Option<Box<HeadlessRenderer>>,
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

/// Create the first virtual output, and the renderer if `render` is set.
pub fn init(render: bool) -> Result<Backend, Error> {
    let renderer = render
        .then(HeadlessRenderer::new)
        .transpose()?
        .map(Box::new);
    info!(size = ?OUTPUT_SIZE, render, "headless backend ready");
    Ok(Backend::Headless(HeadlessData {
        outputs: vec![virtual_output(OUTPUT_NAME, OUTPUT_SIZE)],
        sequence: 0,
        renderer,
    }))
}

/// An output's pixels, read back by [`State::headless_render`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputPixels {
    /// Width and height in physical pixels.
    pub size: (i32, i32),
    /// ARGB8888 as in `wl_shm` (bytes B, G, R, A in memory), top row first, rows packed
    /// (`4 * width` bytes each).
    pub data: Vec<u8>,
    /// What changed since this output was last rendered, in physical pixels. The whole
    /// output the first time and after a mode change; empty if nothing changed.
    pub damage: Vec<Rectangle<i32, Physical>>,
}

impl OutputPixels {
    /// The pixel at (`x`, `y`) as `0xAARRGGBB`. Panics if it is outside the output.
    pub fn pixel(&self, x: i32, y: i32) -> u32 {
        assert!(
            (0..self.size.0).contains(&x) && (0..self.size.1).contains(&y),
            "({x}, {y}) is outside {:?}",
            self.size
        );
        // In range by the assert above, so the casts are lossless.
        let at = (y as usize * self.size.0 as usize + x as usize) * 4;
        let [b, g, r, a] = [
            self.data[at],
            self.data[at + 1],
            self.data[at + 2],
            self.data[at + 3],
        ];
        u32::from_be_bytes([a, r, g, b])
    }
}

/// A GLES renderer on Mesa's software EGL device (llvmpipe), with one offscreen buffer
/// per output. Software only, so pixels are the same with or without a GPU (ADR 0021).
struct HeadlessRenderer {
    renderer: GlesRenderer,
    /// Keyed by output name; made on first render, dropped with the output.
    targets: HashMap<String, Target>,
}

impl fmt::Debug for HeadlessRenderer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HeadlessRenderer")
            .field("targets", &self.targets.keys())
            .finish_non_exhaustive()
    }
}

/// Where one output is drawn.
struct Target {
    buffer: GlesRenderbuffer,
    size: Size<i32, Physical>,
    damage_tracker: OutputDamageTracker,
    /// The buffer holds the previous frame (buffer age 1). False until the first render.
    drawn: bool,
}

/// Offscreen buffer format. RGBA8 renderbuffers are core GLES 3.
const TARGET_FORMAT: Fourcc = Fourcc::Abgr8888;
/// Format of [`OutputPixels::data`]: what `wl_shm` clients (and screencopy) expect.
const READ_FORMAT: Fourcc = Fourcc::Argb8888;

impl HeadlessRenderer {
    fn new() -> Result<Self, Error> {
        let device = EGLDevice::enumerate()
            .map_err(Error::init("listing EGL devices"))?
            .find(EGLDevice::is_software)
            .ok_or_else(|| {
                Error::init("finding the software EGL device")(
                    "no EGL_MESA_device_software device (is Mesa installed?)",
                )
            })?;
        // The workspace denies `unsafe_code`; both constructors are unavoidable for a GL
        // renderer (the tty backend makes the same calls).
        // SAFETY: this is the only EGLDisplay for this device, created and owned by
        // Smithay; nothing else in the process calls eglGetPlatformDisplay/eglTerminate on
        // it.
        #[allow(unsafe_code)] // see SAFETY
        let display =
            unsafe { EGLDisplay::new(device) }.map_err(Error::init("creating the EGL display"))?;
        let context = EGLContext::new(&display).map_err(Error::init("creating the EGL context"))?;
        // SAFETY: the context was just created and is only ever used from this thread (the
        // compositor is single-threaded, AGENTS.md).
        #[allow(unsafe_code)] // see SAFETY
        let renderer = unsafe { GlesRenderer::new(context) }
            .map_err(Error::init("creating the GLES renderer"))?;
        Ok(Self {
            renderer,
            targets: HashMap::new(),
        })
    }

    /// Draw `output` into its offscreen buffer and read the pixels back.
    fn render(
        &mut self,
        output: &Output,
        space: &Space<Window>,
        chrome: &mut Chrome,
    ) -> Result<OutputPixels, String> {
        let mode = output.current_mode().ok_or("the output has no mode")?;
        let size = output.current_transform().transform_size(mode.size);
        let renderer = &mut self.renderer;
        let target = match self.targets.remove(&output.name()) {
            Some(target) if target.size == size => target,
            _ => Target {
                buffer: renderer
                    .create_buffer(TARGET_FORMAT, (size.w, size.h).into())
                    .map_err(|err| format!("creating the offscreen buffer: {err:?}"))?,
                size,
                damage_tracker: OutputDamageTracker::from_output(output),
                drawn: false,
            },
        };
        let target = self.targets.entry(output.name()).or_insert(target);

        let elements = crate::render::output_elements(renderer, space, chrome, output);
        let mut framebuffer = renderer
            .bind(&mut target.buffer)
            .map_err(|err| format!("binding the offscreen buffer: {err:?}"))?;
        let age = usize::from(target.drawn);
        let result = target
            .damage_tracker
            .render_output(renderer, &mut framebuffer, age, &elements, CLEAR_COLOR)
            .map_err(|err| format!("rendering: {err:?}"))?;
        let damage = result.damage.cloned().unwrap_or_default();
        target.drawn = true;

        let region = Rectangle::from_size((size.w, size.h).into());
        let mapping = renderer
            .copy_framebuffer(&framebuffer, region, READ_FORMAT)
            .map_err(|err| format!("reading the pixels back: {err:?}"))?;
        let bytes = renderer
            .map_texture(&mapping)
            .map_err(|err| format!("mapping the pixels: {err:?}"))?;
        // Smithay's GLES frame flips y when drawing into an offscreen target, so the rows
        // read back already start at the top. (`TextureMapping::flipped` says true: it
        // describes GL's bottom-up convention, not what was drawn. The tests check this.)
        Ok(OutputPixels {
            size: (size.w, size.h),
            data: bytes.to_vec(),
            damage,
        })
    }
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
        if let Some(renderer) = &mut data.renderer {
            renderer.targets.remove(name);
        }
        self.remove_output(&output);
        Ok(())
    }

    /// Draw the virtual output called `name` offscreen and return its pixels: windows,
    /// title bars and layer surfaces, without the pointer or a drag icon. Fails without
    /// the renderer ([`crate::Options::headless_renderer`]).
    pub fn headless_render(&mut self, name: &str) -> Result<OutputPixels, String> {
        let Backend::Headless(data) = &mut self.backend else {
            return Err("not the headless backend".into());
        };
        let renderer = data
            .renderer
            .as_mut()
            .ok_or("the headless renderer is off (Options::headless_renderer)")?;
        let output = data
            .outputs
            .iter()
            .find(|o| o.name() == name)
            .ok_or_else(|| format!("no output {name}"))?;
        renderer.render(output, &self.space, &mut self.chrome)
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
    let sequence = match &mut state.backend {
        Backend::Headless(data) => {
            data.sequence += 1;
            data.sequence
        }
        #[allow(unreachable_patterns)] // only headless when winit and tty are off
        _ => return,
    };
    let now = state.clock.now();
    let refresh = crate::presentation::refresh_interval(REFRESH_MHZ);
    for output in state.outputs() {
        crate::presentation::take_feedback_unrendered(&state.space, output).presented(
            now,
            refresh,
            sequence,
            wp_presentation_feedback::Kind::empty(),
        );
        crate::outputs::send_frames(&state.space, output, time);
    }
}
