//! What goes on an output, shared by the winit and tty backends.
//!
//! Like Smithay's `space_render_elements`, but each window is followed by its server-side
//! title bar, so a bar stacks with its window instead of floating above every window.
//! Layer-shell surfaces are drawn in Smithay's order: overlay and top above the windows,
//! bottom and background below.

use smithay::backend::renderer::element::AsRenderElements;
use smithay::backend::renderer::element::memory::MemoryRenderBufferRenderElement;
use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::desktop::{LayerMap, LayerSurface, Space, Window, layer_map_for_output};
use smithay::output::Output;
use smithay::utils::{Logical, Point, Scale};
use smithay::wayland::shell::wlr_layer::Layer;

use crate::decorations::{self, Chrome, frame_geometry};

smithay::backend::renderer::element::render_elements! {
    /// Windows, their title bars and layer surfaces on one output, topmost first.
    pub OutputElement<=GlesRenderer>;
    Surface=WaylandSurfaceRenderElement<GlesRenderer>,
    Chrome=MemoryRenderBufferRenderElement<GlesRenderer>,
}

/// Everything on `output`, topmost first. Empty if the output is not in `space`.
pub fn output_elements(
    renderer: &mut GlesRenderer,
    space: &Space<Window>,
    chrome: &mut Chrome,
    output: &Output,
) -> Vec<OutputElement> {
    let Some(output_geo) = space.output_geometry(output) else {
        return Vec::new();
    };
    let output_scale = output.current_scale().fractional_scale();
    let scale = Scale::from(output_scale);
    let mut elements = Vec::new();

    let layer_map = layer_map_for_output(output);
    let (lower, upper): (Vec<&LayerSurface>, Vec<&LayerSurface>) = layer_map
        .layers()
        .rev()
        .partition(|s| matches!(s.layer(), Layer::Background | Layer::Bottom));
    push_layers(&mut elements, renderer, &layer_map, upper, scale);

    for window in space.elements().rev() {
        let Some(geometry) = space.element_geometry(window) else {
            continue;
        };
        let decorated = decorations::is_server_decorated(window);
        let frame = frame_geometry(window, geometry);
        let bbox = space
            .element_bbox(window)
            .map_or(frame, |bbox| bbox.merge(frame));
        if !output_geo.overlaps(bbox) {
            continue;
        }
        let render_location = geometry.loc - window.geometry().loc - output_geo.loc;
        elements.extend(
            window
                .render_elements::<WaylandSurfaceRenderElement<GlesRenderer>>(
                    renderer,
                    render_location.to_physical_precise_round(output_scale),
                    scale,
                    1.0,
                )
                .into_iter()
                .map(OutputElement::Surface),
        );
        if decorated {
            let bar: Point<i32, Logical> = frame.loc - output_geo.loc;
            elements.extend(
                chrome
                    .elements(
                        renderer,
                        window,
                        bar.to_f64().to_physical(scale),
                        frame.size.w,
                        output_scale,
                    )
                    .into_iter()
                    .map(OutputElement::Chrome),
            );
        }
    }

    push_layers(&mut elements, renderer, &layer_map, lower, scale);
    elements
}

/// Append the render elements of layer `surfaces`, topmost first.
fn push_layers(
    elements: &mut Vec<OutputElement>,
    renderer: &mut GlesRenderer,
    layer_map: &LayerMap,
    surfaces: Vec<&LayerSurface>,
    scale: Scale<f64>,
) {
    for surface in surfaces {
        let Some(geo) = layer_map.layer_geometry(surface) else {
            continue;
        };
        elements.extend(
            AsRenderElements::<GlesRenderer>::render_elements::<
                WaylandSurfaceRenderElement<GlesRenderer>,
            >(
                surface,
                renderer,
                geo.loc.to_physical_precise_round(scale),
                scale,
                1.0,
            )
            .into_iter()
            .map(OutputElement::Surface),
        );
    }
}
