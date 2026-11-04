//! Nested backend: the compositor runs in a window of another Wayland compositor.
//!
//! Adapted from Smithay's `smallvil/src/winit.rs` and `anvil/src/winit.rs` (MIT): GLES
//! rendering through `OutputDamageTracker` with buffer age, so only damaged regions are
//! redrawn and submitted, and linux-dmabuf v4 feedback from the EGL device's render node.

use std::fmt;

use smithay::backend::egl::EGLDevice;
use smithay::backend::renderer::ImportDma;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::surface::{
    WaylandSurfaceRenderElement, render_elements_from_surface_tree,
};
use smithay::backend::renderer::gles::GlesRenderer;

use crate::render::{CLEAR_COLOR, OutputElement};
use smithay::backend::winit::{self, WinitEvent, WinitEventLoop, WinitGraphicsBackend};
use smithay::input::pointer::{CursorIcon, CursorImageStatus};
use smithay::output::{Mode, Output, PhysicalProperties, Subpixel};
use smithay::reexports::wayland_protocols::wp::presentation_time::server::wp_presentation_feedback;
use smithay::reexports::wayland_server::DisplayHandle;
use smithay::reexports::winit::cursor::Cursor;
use smithay::utils::{IsAlive, Scale, Transform};
use smithay::wayland::dmabuf::{DmabufFeedbackBuilder, DmabufGlobal, DmabufState};
use tracing::{info, warn};

use crate::Error;
use crate::state::{Backend, State};

/// Refresh rate reported for the nested output, in mHz. The host paces our frames.
const REFRESH_MHZ: i32 = 60_000;

/// State owned by the winit backend.
pub struct WinitData {
    backend: WinitGraphicsBackend<GlesRenderer>,
    output: Output,
    damage_tracker: OutputDamageTracker,
    _dmabuf_global: DmabufGlobal,
    /// Frames left to redraw in full (after a resize, buffer contents are undefined).
    full_redraw: u8,
}

impl fmt::Debug for WinitData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WinitData")
            .field("output", &self.output.name())
            .field("full_redraw", &self.full_redraw)
            .finish_non_exhaustive()
    }
}

impl WinitData {
    /// Show `image` on the host window: named shapes use the host's own theme; a
    /// client-drawn surface is not rendered when nested, so it shows the default arrow.
    pub fn set_host_cursor(&self, image: &CursorImageStatus) {
        let window = self.backend.window();
        match image {
            CursorImageStatus::Hidden => window.set_cursor_visible(false),
            CursorImageStatus::Named(icon) => {
                window.set_cursor_visible(true);
                window.set_cursor(Cursor::Icon(*icon));
            }
            CursorImageStatus::Surface(_) => {
                window.set_cursor_visible(true);
                window.set_cursor(Cursor::Icon(CursorIcon::Default));
            }
        }
    }

    pub fn output(&self) -> &Output {
        &self.output
    }

    pub fn renderer(&mut self) -> &mut GlesRenderer {
        self.backend.renderer()
    }
}

/// Open the host window, create the output and the dmabuf global.
pub fn init(
    dh: &DisplayHandle,
    dmabuf_state: &mut DmabufState,
) -> Result<(Backend, WinitEventLoop), Error> {
    let (mut backend, events) = winit::init::<GlesRenderer>()
        .map_err(|err| err.to_string())
        .map_err(Error::init("opening the winit window"))?;

    let mode = Mode {
        size: backend.window_size(),
        refresh: REFRESH_MHZ,
    };
    let output = Output::new(
        "winit".to_owned(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "neueOS".into(),
            model: "Nested".into(),
            serial_number: "0".into(),
        },
    );
    // GL renders upside down relative to the window, hence the flip.
    output.change_current_state(Some(mode), Some(Transform::Flipped180), None, None);
    output.set_preferred(mode);

    // linux-dmabuf v4 with feedback needs the render node; fall back to v3 without it.
    let formats = backend.renderer().dmabuf_formats();
    let render_node = EGLDevice::device_for_display(backend.renderer().egl_context().display())
        .and_then(|device| device.try_get_render_node());
    let feedback = match render_node {
        Ok(Some(node)) => DmabufFeedbackBuilder::new(node.dev_id(), formats.clone())
            .build()
            .map_err(|err| warn!(?err, "building dmabuf feedback failed"))
            .ok(),
        Ok(None) => {
            warn!("no render node; linux-dmabuf falls back to v3");
            None
        }
        Err(err) => {
            warn!(?err, "no EGL device; linux-dmabuf falls back to v3");
            None
        }
    };
    let dmabuf_global = match feedback {
        Some(feedback) => dmabuf_state.create_global_with_default_feedback::<State>(dh, &feedback),
        None => dmabuf_state.create_global::<State>(dh, formats),
    };

    info!(size = ?mode.size, "winit backend ready");
    let damage_tracker = OutputDamageTracker::from_output(&output);
    let data = WinitData {
        backend,
        output,
        damage_tracker,
        _dmabuf_global: dmabuf_global,
        full_redraw: 0,
    };
    Ok((Backend::Winit(Box::new(data)), events))
}

/// Handle one event from the host window.
pub fn handle_event(event: WinitEvent, state: &mut State) {
    match event {
        WinitEvent::Resized { size, .. } => {
            let Backend::Winit(data) = &mut state.backend else {
                return;
            };
            let mode = Mode {
                size,
                refresh: REFRESH_MHZ,
            };
            data.output
                .change_current_state(Some(mode), None, None, None);
            data.output.set_preferred(mode);
            data.full_redraw = 4;
            let output = data.output.clone();
            state.output_mode_changed(&output);
        }
        WinitEvent::Input(event) => state.process_input_event(event),
        WinitEvent::Redraw => render(state),
        WinitEvent::CloseRequested => state.loop_signal.stop(),
        WinitEvent::Focus(_) => {}
    }
}

fn render(state: &mut State) {
    let Backend::Winit(data) = &mut state.backend else {
        return;
    };
    data.full_redraw = data.full_redraw.saturating_sub(1);
    let age = if data.full_redraw > 0 {
        0
    } else {
        data.backend.buffer_age().unwrap_or(0)
    };

    let output = &data.output;
    let damage_tracker = &mut data.damage_tracker;
    let space = &state.space;
    let chrome = &mut state.chrome;
    // The drag icon, if a drag is running, is drawn at the pointer.
    let icon = state.dnd_icon.as_ref().filter(|i| i.surface.alive());
    let pointer_location = state.pointer.current_location();
    let output_loc = space.output_geometry(output).map(|geo| geo.loc);
    let scale = Scale::from(output.current_scale().fractional_scale());
    let result = data
        .backend
        .bind()
        .map_err(|err| format!("{err:?}"))
        .and_then(|(renderer, mut framebuffer)| {
            let mut elements: Vec<OutputElement> = match (icon, output_loc) {
                (Some(icon), Some(loc)) => {
                    let at = pointer_location - loc.to_f64() + icon.offset.to_f64();
                    render_elements_from_surface_tree::<_, WaylandSurfaceRenderElement<_>>(
                        renderer,
                        &icon.surface,
                        at.to_physical(scale).to_i32_round(),
                        scale,
                        1.0,
                        Kind::Unspecified,
                    )
                    .into_iter()
                    .map(OutputElement::Surface)
                    .collect()
                }
                _ => Vec::new(),
            };
            elements.extend(crate::render::output_elements(
                renderer, space, chrome, output,
            ));
            damage_tracker
                .render_output(renderer, &mut framebuffer, age, &elements, CLEAR_COLOR)
                .map(|result| (result.damage.cloned(), result.states))
                .map_err(|err| format!("{err:?}"))
        });

    match result {
        Ok((damage, states)) => {
            crate::presentation::update_primary_scanout_output(
                space,
                output,
                &state.cursor_status,
                icon,
                &states,
            );
            if let Some(damage) = damage {
                match data.backend.submit(Some(&damage)) {
                    // The host compositor does not tell us when the frame is on screen;
                    // submission is the closest we know.
                    Ok(()) => {
                        let refresh = crate::presentation::refresh_interval(REFRESH_MHZ);
                        crate::presentation::take_feedback(space, output, &states).presented(
                            state.clock.now(),
                            refresh,
                            0,
                            wp_presentation_feedback::Kind::Vsync,
                        );
                    }
                    Err(err) => warn!(?err, "submitting the frame failed"),
                }
            }
        }
        Err(err) => warn!(err, "rendering failed"),
    }

    // Frame callbacks for everything on this output, so clients draw their next frame.
    crate::outputs::send_frames(&state.space, output, state.start_time.elapsed());

    data.backend.window().request_redraw();
    // Screencopy frames waiting for damage may have some now.
    let output = output.clone();
    state.capture_waiting_frames(&output);
}
