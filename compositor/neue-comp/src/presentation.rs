//! Presentation timing (task 1.14, ADR 0017): `wp_presentation` feedback and
//! `wp_tearing_control_v1` hints.
//!
//! Feedback follows frame callbacks: a surface gets it from the output that paces it (its
//! primary scan-out output when something rendered it, else the output its window overlaps
//! most). The tty backend takes feedback when a frame is queued and marks it presented at
//! that frame's vblank, with the kernel's timestamp; winit marks it when the frame is
//! submitted; the headless backend on each tick of its frame timer.
//!
//! Tearing hints are recorded per surface (double-buffered, like the protocol says) but
//! not acted on: Smithay's DRM compositor has no asynchronous page flips yet, so every
//! flip waits for vblank. [`State::wants_tearing`] is the hook for when it does.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use smithay::backend::renderer::element::{
    RenderElementStates, default_primary_scanout_output_compare,
};
use smithay::desktop::utils::{
    OutputPresentationFeedback, surface_presentation_feedback_flags_from_states,
    surface_primary_scanout_output, take_presentation_feedback_surface_tree,
    update_surface_primary_scanout_output, with_surfaces_surface_tree,
};
use smithay::desktop::{Space, Window, layer_map_for_output};
use smithay::input::pointer::CursorImageStatus;
use smithay::output::Output;
use smithay::reexports::wayland_protocols::wp::presentation_time::server::wp_presentation_feedback;
use smithay::reexports::wayland_protocols::wp::tearing_control::v1::server::{
    wp_tearing_control_manager_v1::{self, WpTearingControlManagerV1},
    wp_tearing_control_v1::{self, PresentationHint, WpTearingControlV1},
};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource, WEnum, Weak, backend::GlobalId,
};
use smithay::wayland::compositor::{Cacheable, with_states};
use smithay::wayland::presentation::Refresh;
use smithay::wayland::{Dispatch2, GlobalDispatch2};

use crate::state::{DndIcon, State};

/// Version of `wp_tearing_control_manager_v1` we implement.
const TEARING_CONTROL_VERSION: u32 = 1;

/// The refresh interval of a mode refreshing at `refresh_mhz` (millihertz), for
/// presentation feedback. Truncated to whole nanoseconds, like the protocol.
pub fn refresh_interval(refresh_mhz: i32) -> Refresh {
    match u64::try_from(refresh_mhz) {
        Ok(mhz) if mhz > 0 => Refresh::fixed(Duration::from_nanos(1_000_000_000_000 / mhz)),
        _ => Refresh::Unknown,
    }
}

/// Mark every surface `output` rendered (per `states`) as scanned out there, so feedback
/// and frame pacing know which output shows it.
// Only the rendering backends (winit, tty) call this.
#[cfg_attr(not(any(feature = "winit", feature = "tty")), allow(dead_code))]
pub fn update_primary_scanout_output(
    space: &Space<Window>,
    output: &Output,
    cursor: &CursorImageStatus,
    dnd_icon: Option<&DndIcon>,
    states: &RenderElementStates,
) {
    let mut update = |surface: &WlSurface, data: &_| {
        update_surface_primary_scanout_output(
            surface,
            output,
            data,
            None,
            states,
            default_primary_scanout_output_compare,
        );
    };
    for window in space.elements() {
        window.with_surfaces(&mut update);
    }
    for layer in layer_map_for_output(output).layers() {
        layer.with_surfaces(&mut update);
    }
    if let Some(lock) = crate::lock::with_output_lock(output, |lock| lock.surface.clone()) {
        with_surfaces_surface_tree(lock.wl_surface(), &mut update);
    }
    if let CursorImageStatus::Surface(surface) = cursor {
        with_surfaces_surface_tree(surface, &mut update);
    }
    if let Some(icon) = dnd_icon {
        with_surfaces_surface_tree(&icon.surface, &mut update);
    }
}

/// The feedback of every surface whose primary scan-out output is `output`, with the flags
/// rendering earned it (zero copy when a plane scanned the buffer out).
// Only the rendering backends (winit, tty) call this.
#[cfg_attr(not(any(feature = "winit", feature = "tty")), allow(dead_code))]
pub fn take_feedback(
    space: &Space<Window>,
    output: &Output,
    states: &RenderElementStates,
) -> OutputPresentationFeedback {
    let mut feedback = OutputPresentationFeedback::new(output);
    let flags = |surface: &WlSurface, _: &_| {
        surface_presentation_feedback_flags_from_states(surface, None, states)
    };
    for window in space.elements() {
        window.take_presentation_feedback(&mut feedback, surface_primary_scanout_output, flags);
    }
    for layer in layer_map_for_output(output).layers() {
        layer.take_presentation_feedback(&mut feedback, surface_primary_scanout_output, flags);
    }
    if let Some(lock) = crate::lock::with_output_lock(output, |lock| lock.surface.clone()) {
        take_presentation_feedback_surface_tree(
            lock.wl_surface(),
            &mut feedback,
            surface_primary_scanout_output,
            flags,
        );
    }
    feedback
}

/// Feedback without a renderer (headless): what `output` paces, by the same rule as
/// frame callbacks (`outputs::send_frames`), with no flags.
pub fn take_feedback_unrendered(
    space: &Space<Window>,
    output: &Output,
) -> OutputPresentationFeedback {
    let mut feedback = OutputPresentationFeedback::new(output);
    let first = space.outputs().next() == Some(output);
    let none = |_: &WlSurface, _: &_| wp_presentation_feedback::Kind::empty();
    for window in space.elements() {
        let primary = crate::outputs::primary_output(space, window);
        if primary.as_ref() == Some(output) || (primary.is_none() && first) {
            window.take_presentation_feedback(&mut feedback, |_, _| Some(output.clone()), none);
        }
    }
    for layer in layer_map_for_output(output).layers() {
        layer.take_presentation_feedback(&mut feedback, |_, _| Some(output.clone()), none);
    }
    feedback
}

/// The tearing hint of a surface, double-buffered: applied on `wl_surface.commit`.
#[derive(Debug, Clone, Copy)]
pub struct TearingCachedState {
    pub hint: PresentationHint,
}

impl Default for TearingCachedState {
    fn default() -> Self {
        Self {
            hint: PresentationHint::Vsync,
        }
    }
}

impl Cacheable for TearingCachedState {
    fn commit(&mut self, _dh: &DisplayHandle) -> Self {
        *self
    }

    fn merge_into(self, into: &mut Self, _dh: &DisplayHandle) {
        *into = self;
    }
}

/// Marks a surface that has a `wp_tearing_control_v1` (one is allowed per surface).
#[derive(Debug, Default)]
struct HasTearingControl(AtomicBool);

/// The `wp_tearing_control_manager_v1` global.
#[derive(Debug)]
pub struct TearingControlState {
    _global: GlobalId,
}

impl TearingControlState {
    pub fn new(dh: &DisplayHandle) -> Self {
        Self {
            _global: dh.create_global::<State, WpTearingControlManagerV1, _>(
                TEARING_CONTROL_VERSION,
                TearingGlobal,
            ),
        }
    }
}

/// User data of the global and of bound managers.
#[derive(Debug)]
pub struct TearingGlobal;

/// User data of a `wp_tearing_control_v1`: its surface (inert once the surface is gone).
#[derive(Debug)]
pub struct TearingControl(Mutex<Weak<WlSurface>>);

impl TearingControl {
    fn surface(&self) -> Option<WlSurface> {
        self.0.lock().ok()?.upgrade().ok()
    }
}

impl GlobalDispatch2<WpTearingControlManagerV1, State> for TearingGlobal {
    fn bind(
        &self,
        _state: &mut State,
        _dh: &DisplayHandle,
        _client: &Client,
        resource: New<WpTearingControlManagerV1>,
        data_init: &mut DataInit<'_, State>,
    ) {
        data_init.init(resource, TearingGlobal);
    }
}

impl Dispatch2<WpTearingControlManagerV1, State> for TearingGlobal {
    fn request(
        &self,
        _state: &mut State,
        _client: &Client,
        manager: &WpTearingControlManagerV1,
        request: wp_tearing_control_manager_v1::Request,
        _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, State>,
    ) {
        match request {
            wp_tearing_control_manager_v1::Request::GetTearingControl { id, surface } => {
                let taken = with_states(&surface, |states| {
                    states
                        .data_map
                        .get_or_insert_threadsafe(HasTearingControl::default)
                        .0
                        .swap(true, Ordering::AcqRel)
                });
                if taken {
                    manager.post_error(
                        wp_tearing_control_manager_v1::Error::TearingControlExists,
                        "the surface already has a wp_tearing_control_v1",
                    );
                } else {
                    data_init.init(id, TearingControl(Mutex::new(surface.downgrade())));
                }
            }
            wp_tearing_control_manager_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch2<WpTearingControlV1, State> for TearingControl {
    fn request(
        &self,
        _state: &mut State,
        _client: &Client,
        _resource: &WpTearingControlV1,
        request: wp_tearing_control_v1::Request,
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, State>,
    ) {
        let Some(surface) = self.surface() else {
            return;
        };
        match request {
            wp_tearing_control_v1::Request::SetPresentationHint { hint } => {
                // An unknown hint value is ignored (a newer client); vsync stays.
                let WEnum::Value(hint) = hint else { return };
                with_states(&surface, |states| {
                    states
                        .cached_state
                        .get::<TearingCachedState>()
                        .pending()
                        .hint = hint;
                });
            }
            // Destroying the object reverts to vsync on the next commit (spec).
            wp_tearing_control_v1::Request::Destroy => with_states(&surface, |states| {
                if let Some(marker) = states.data_map.get::<HasTearingControl>() {
                    marker.0.store(false, Ordering::Release);
                }
                states
                    .cached_state
                    .get::<TearingCachedState>()
                    .pending()
                    .hint = PresentationHint::Vsync;
            }),
            _ => {}
        }
    }
}

impl State {
    /// Whether `surface`'s current state asks for tearing (asynchronous) page flips.
    pub fn wants_tearing(surface: &WlSurface) -> bool {
        with_states(surface, |states| {
            states
                .cached_state
                .get::<TearingCachedState>()
                .current()
                .hint
                == PresentationHint::Async
        })
    }
}
