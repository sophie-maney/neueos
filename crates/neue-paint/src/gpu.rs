//! GPU backend: replays a [`Scene`] with `vello_gpu` on wgpu.
//!
//! `vello_gpu` needs no compute shaders, so it also runs on GLES 3.0 (virgl in the VM; see
//! `docs/research/vm-graphics.md`).

use std::collections::HashMap;
use std::fmt;

use vello_common::paint::{ImageId, ImageSource};
use vello_common::peniko::Color;
use vello_gpu::{
    ClearSettings, RenderSize, RenderTargetConfig, Renderer, Resources, TargetInit, TextureBindings,
};

use crate::scene::{Scene, TextureBrush};
use crate::{Error, device_size};

/// Renders [`Scene`]s into wgpu textures. Keeps uploaded procedural textures between
/// frames.
pub struct GpuRenderer {
    renderer: Renderer,
    resources: Resources,
    scene: vello_gpu::Scene,
    textures: HashMap<(u64, u64), (ImageId, bool)>,
    depth: Option<((u16, u16), wgpu::TextureView)>,
}

impl fmt::Debug for GpuRenderer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GpuRenderer")
            .field("textures", &self.textures.len())
            .finish_non_exhaustive()
    }
}

impl GpuRenderer {
    /// A renderer for targets of `format`. Use a non-sRGB format: vello blends in sRGB
    /// space and writes sRGB-encoded values.
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let (renderer, resources) = Renderer::new(
            device,
            &RenderTargetConfig {
                format,
                width: 1,
                height: 1,
            },
        );
        Self {
            renderer,
            resources,
            scene: vello_gpu::Scene::new(1, 1),
            textures: HashMap::new(),
            depth: None,
        }
    }

    /// Record commands that render `scene` at `scale` into `target`, cleared to
    /// `background` first. `target` must be `ceil(scene size * scale)` device pixels.
    /// Submit `encoder` afterwards.
    #[allow(
        clippy::too_many_arguments,
        reason = "mirrors vello_gpu::Renderer::render"
    )]
    pub fn render(
        &mut self,
        scene: &Scene,
        scale: f64,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        background: Color,
    ) -> Result<(), Error> {
        let (width, height) = device_size(scene, scale)?;
        let size = RenderSize { width, height };
        self.scene.reset_and_resize(width, height);

        let Self {
            renderer,
            resources,
            scene: gpu_scene,
            textures,
            depth,
        } = self;
        let mut resolve = |resources: &mut Resources, brush: &TextureBrush| {
            let key = (brush.texture.cache_key(), scale.to_bits());
            let (id, transparent) = *textures.entry(key).or_insert_with(|| {
                let pixmap = brush.texture.render(scale);
                let id = renderer.upload_image(resources, device, queue, encoder, &pixmap);
                (id, pixmap.may_have_transparency())
            });
            ImageSource::opaque_id_with_transparency_hint(id, transparent)
        };
        crate::replay::replay!(&mut *gpu_scene, scene, scale, &mut *resources, &mut resolve);

        let depth_view = match depth {
            Some((depth_size, view)) if *depth_size == (width, height) => view,
            _ => {
                let view = Renderer::create_depth_texture_view(device, &size);
                &depth.insert(((width, height), view)).1
            }
        };
        renderer
            .render(
                gpu_scene,
                resources,
                device,
                queue,
                encoder,
                &size,
                target,
                Some(depth_view),
                &TextureBindings::new(),
                TargetInit::Clear(ClearSettings::Viewport { color: background }),
            )
            .map_err(Error::Gpu)
    }
}
