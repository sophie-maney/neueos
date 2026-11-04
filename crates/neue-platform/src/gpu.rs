//! wgpu device and per-surface render targets, shared by both drivers. Masonry content is
//! rendered by vello_gpu (`masonry_imaging`'s hybrid renderer) into an offscreen texture,
//! then blitted to the surface, as `masonry_winit` does.

use masonry_imaging::PreparedFrame;
use masonry_imaging::texture_render::{RenderTarget, Renderer};
use wgpu::util::{TextureBlitter, TextureBlitterBuilder};

/// A wgpu failure: no adapter, no device, or no usable surface format.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub(crate) struct GpuError(pub(crate) String);

/// Adapter, device and the Masonry renderer, shared by every surface.
pub(crate) struct Gpu {
    pub(crate) instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: Renderer,
}

impl std::fmt::Debug for Gpu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gpu")
            .field("adapter", &self.adapter.get_info().name)
            .finish_non_exhaustive()
    }
}

impl Gpu {
    /// Picks an adapter that can present to `surface`. `WGPU_BACKEND=gl` selects the VM's
    /// virgl GLES instead of llvmpipe Vulkan (docs/research/vm-graphics.md).
    pub(crate) fn new(
        instance: wgpu::Instance,
        surface: &wgpu::Surface<'_>,
    ) -> Result<Self, GpuError> {
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(surface),
            ..Default::default()
        }))
        .map_err(|e| GpuError(e.to_string()))?;
        let info = adapter.get_info();
        tracing::info!(adapter = %info.name, backend = ?info.backend, "wgpu adapter");
        // Request the adapter's own limits: the defaults exceed GLES 3.0 (ADR 0002 §2).
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("neue-platform"),
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .map_err(|e| GpuError(e.to_string()))?;
        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            renderer: Renderer::new(),
        })
    }

    /// Renders `frame` into `target` and presents it. `pre_present` runs right before the
    /// present (winit wants `Window::pre_present_notify` there). Returns whether the frame
    /// was presented (Wayland: whether the surface was committed).
    pub(crate) fn render(
        &mut self,
        target: &SurfaceTarget,
        frame: PreparedFrame<'_>,
        pre_present: impl FnOnce(),
    ) -> bool {
        let texture = match target.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                // The window changed under us (e.g. mid-resize): reconfigure and retry once.
                target.surface.configure(&self.device, &target.config);
                match target.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(t)
                    | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
                    other => {
                        tracing::warn!(?other, "no surface texture after reconfigure");
                        return false;
                    }
                }
            }
            // Hidden or minimised window, or a slow compositor: the next frame will do.
            wgpu::CurrentSurfaceTexture::Occluded | wgpu::CurrentSurfaceTexture::Timeout => {
                tracing::debug!("surface occluded or timed out; skipping frame");
                return false;
            }
            other => {
                tracing::warn!(?other, "no surface texture; skipping frame");
                return false;
            }
        };
        let render_target = RenderTarget {
            adapter: &self.adapter,
            device: &self.device,
            queue: &self.queue,
            texture: &target.texture,
            view: &target.view,
        };
        if let Err(err) = self.renderer.render_to_texture(render_target, frame) {
            tracing::error!(%err, "rendering Masonry content failed");
            return false;
        }
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("blit"),
            });
        let view = texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        target
            .blitter
            .copy(&self.device, &mut encoder, &target.view, &view);
        self.queue.submit([encoder.finish()]);
        pre_present();
        self.queue.present(texture);
        true
    }
}

/// A configured wgpu surface plus its offscreen target. Recreated on every resize.
pub(crate) struct SurfaceTarget {
    surface: wgpu::Surface<'static>,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    blitter: TextureBlitter,
    config: wgpu::SurfaceConfiguration,
}

impl std::fmt::Debug for SurfaceTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SurfaceTarget")
            .field("width", &self.config.width)
            .field("height", &self.config.height)
            .finish_non_exhaustive()
    }
}

impl SurfaceTarget {
    /// Configures `surface` at `width`×`height` physical pixels.
    pub(crate) fn new(
        gpu: &Gpu,
        surface: wgpu::Surface<'static>,
        width: u32,
        height: u32,
    ) -> Result<Self, GpuError> {
        let caps = surface.get_capabilities(&gpu.adapter);
        // A non-sRGB format: vello blends in sRGB space already (ADR 0002 §2).
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| {
                matches!(
                    f,
                    wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm
                )
            })
            .ok_or_else(|| GpuError("surface offers no non-sRGB 8-bit format".into()))?;
        // Same alpha handling as masonry_winit: the target holds premultiplied colour.
        const PREMUL: wgpu::BlendState = wgpu::BlendState {
            alpha: wgpu::BlendComponent::REPLACE,
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::Zero,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let (alpha_mode, blitter) = if caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PostMultiplied)
        {
            (
                wgpu::CompositeAlphaMode::PostMultiplied,
                TextureBlitter::new(&gpu.device, format),
            )
        } else if caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
        {
            (
                wgpu::CompositeAlphaMode::PreMultiplied,
                TextureBlitterBuilder::new(&gpu.device, format)
                    .blend_state(PREMUL)
                    .build(),
            )
        } else {
            (
                wgpu::CompositeAlphaMode::Auto,
                TextureBlitter::new(&gpu.device, format),
            )
        };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            // Fifo is the only mode every platform guarantees (ADR 0002 §2).
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            color_space: wgpu::SurfaceColorSpace::Srgb,
            view_formats: vec![],
        };
        surface.configure(&gpu.device, &config);
        let (texture, view) = offscreen(&gpu.device, width, height);
        Ok(Self {
            surface,
            texture,
            view,
            blitter,
            config,
        })
    }

    /// Reconfigures for a new physical size.
    pub(crate) fn resize(&mut self, gpu: &Gpu, width: u32, height: u32) {
        if self.config.width == width && self.config.height == height {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&gpu.device, &self.config);
        (self.texture, self.view) = offscreen(&gpu.device, width, height);
    }
}

/// The texture vello_gpu renders into. No storage binding: GLES 3.0 can't do it, and the
/// hybrid renderer only needs a colour attachment.
fn offscreen(device: &wgpu::Device, width: u32, height: u32) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("masonry target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Rgba8Unorm,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}
