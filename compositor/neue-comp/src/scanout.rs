//! Direct scanout on the tty backend (task 1.15c, ADR 0020).
//!
//! A client buffer that covers a whole output (a full-screen window without a title bar)
//! can be put on the CRTC's primary plane instead of being copied into our swapchain:
//! Smithay's DRM compositor tries that on every frame when [`FRAME_FLAGS`] allow it, tests
//! the plane assignment with an atomic test commit, and falls back to compositing when the
//! test fails. Overlay planes stay off (they break on NVIDIA, and an underlay setup is
//! beyond Phase 1); the cursor keeps its own plane.
//!
//! Clients learn which buffers can be scanned out from per-surface dmabuf feedback: a
//! surface Smithay tried to scan out (it did, or the format or the commit was wrong) gets
//! [`OutputFeedback::scanout`], which prefers the primary plane's formats and modifiers;
//! every other surface gets the default feedback, as before.

use smithay::backend::allocator::format::FormatSet;
use smithay::backend::drm::DrmNode;
use smithay::backend::drm::compositor::FrameFlags;
use smithay::backend::renderer::element::RenderElementStates;
use smithay::backend::renderer::element::utils::select_dmabuf_feedback;
use smithay::desktop::utils::surface_primary_scanout_output;
use smithay::desktop::{Space, Window, layer_map_for_output};
use smithay::output::Output;
use smithay::reexports::wayland_protocols::wp::linux_dmabuf::zv1::server::zwp_linux_dmabuf_feedback_v1::TrancheFlags;
use smithay::wayland::dmabuf::{DmabufFeedback, DmabufFeedbackBuilder};
use tracing::warn;

/// Primary-plane scanout (any format: the test commit rejects what the plane cannot show)
/// and the cursor plane. No overlays.
pub const FRAME_FLAGS: FrameFlags = FrameFlags::ALLOW_PRIMARY_PLANE_SCANOUT
    .union(FrameFlags::ALLOW_PRIMARY_PLANE_SCANOUT_ANY)
    .union(FrameFlags::ALLOW_CURSOR_PLANE_SCANOUT);

/// The two feedbacks a surface on one output can get.
#[derive(Debug, Clone)]
pub struct OutputFeedback {
    /// What the renderer imports: the global default.
    pub render: DmabufFeedback,
    /// A scanout tranche for the primary plane first, then the renderer's formats.
    pub scanout: DmabufFeedback,
}

/// The default feedback: everything `render_formats` on `render_node`.
pub fn render_feedback(render_node: DrmNode, render_formats: &FormatSet) -> Option<DmabufFeedback> {
    DmabufFeedbackBuilder::new(render_node.dev_id(), render_formats.clone())
        .build()
        .map_err(|err| warn!(?err, "building dmabuf feedback failed"))
        .ok()
}

/// The feedbacks for an output whose primary plane on `scanout_node` (the GPU's primary
/// node) takes `plane_formats`. The scanout tranche only has formats the renderer can also
/// import, so a buffer that stops fitting the plane can still be composited.
pub fn output_feedback(
    render_node: DrmNode,
    scanout_node: DrmNode,
    render_formats: &FormatSet,
    plane_formats: &FormatSet,
) -> Option<OutputFeedback> {
    let render = render_feedback(render_node, render_formats)?;
    let scanout_formats: FormatSet = plane_formats
        .intersection(render_formats)
        .copied()
        .collect();
    let scanout = DmabufFeedbackBuilder::new(render_node.dev_id(), render_formats.clone())
        .add_preference_tranche(
            scanout_node.dev_id(),
            TrancheFlags::Scanout,
            scanout_formats,
            // Feedback objects exist from linux-dmabuf v4; 6 is the newest Smithay knows.
            4u32..=6,
        )
        .build()
        .map_err(|err| warn!(?err, "building scanout dmabuf feedback failed"))
        .ok()?;
    Some(OutputFeedback { render, scanout })
}

/// Give every window and layer surface paced by `output` the feedback that fits how the
/// last frame (`states`) showed it. Smithay only sends a feedback when it changes.
pub fn send_feedback(
    space: &Space<Window>,
    output: &Output,
    states: &RenderElementStates,
    feedback: &OutputFeedback,
) {
    let select = |surface: &_, _: &_| {
        select_dmabuf_feedback(surface, states, &feedback.render, &feedback.scanout)
    };
    for window in space.elements() {
        window.send_dmabuf_feedback(output, surface_primary_scanout_output, select);
    }
    for layer in layer_map_for_output(output).layers() {
        layer.send_dmabuf_feedback(output, surface_primary_scanout_output, select);
    }
}
