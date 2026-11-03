//! Shows the neue-paint proof of concept (linen and the Aqua button, light and dark) in a
//! winit window, rendered on the GPU with vello_gpu.
//!
//! Run with `cargo run -p neue-paint --example poc --features gpu`.

use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use neue_paint::demo::{self, SHEET_HEIGHT, SHEET_WIDTH};
use neue_paint::gpu::GpuRenderer;
use neue_paint::kurbo::Affine;
use neue_paint::peniko::Color;
use neue_paint::{Appearance, Scene};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

/// Light sheet on the left, dark sheet on the right.
fn scene() -> Scene {
    let mut scene = Scene::new(SHEET_WIDTH * 2.0, SHEET_HEIGHT);
    demo::draw_sheet(&mut scene, Appearance::Light);
    scene.with_transform(Affine::translate((SHEET_WIDTH, 0.0)), |s| {
        demo::draw_sheet(s, Appearance::Dark);
    });
    scene
}

struct Gfx {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: GpuRenderer,
    scene: Scene,
}

impl Gfx {
    fn new(event_loop: &ActiveEventLoop) -> Result<Self> {
        let attrs = Window::default_attributes()
            .with_title("neue-paint proof of concept")
            .with_inner_size(LogicalSize::new(SHEET_WIDTH * 2.0, SHEET_HEIGHT))
            .with_resizable(false);
        let window = Arc::new(event_loop.create_window(attrs)?);

        // Passing the display handle is required for wgpu's GL backend on Wayland
        // (docs/research/vm-graphics.md).
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle_from_env(
                Box::new(event_loop.owned_display_handle()),
            ));
        let surface = instance.create_surface(Arc::clone(&window))?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))?;

        let caps = surface.get_capabilities(&adapter);
        // vello writes sRGB-encoded values, so the surface must not encode again.
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .context("surface offers no non-sRGB format")?;
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            // Fifo is the only mode every platform guarantees.
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::Srgb,
        };
        surface.configure(&device, &config);
        let renderer = GpuRenderer::new(&device, format);
        Ok(Self {
            window,
            surface,
            device,
            queue,
            config,
            renderer,
            scene: scene(),
        })
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        self.config.width = size.width.max(1);
        self.config.height = size.height.max(1);
        self.surface.configure(&self.device, &self.config);
        self.window.request_redraw();
    }

    fn draw(&mut self) -> Result<()> {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                self.window.request_redraw();
                return Ok(());
            }
            _ => return Ok(()),
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        // The scene must cover the surface exactly. Derive the scale from the surface so
        // rounding in the window size can't make them disagree.
        let scale = f64::from(self.config.width) / self.scene.width();
        self.renderer
            .render(
                &self.scene,
                scale,
                &self.device,
                &self.queue,
                &mut encoder,
                &view,
                Color::BLACK,
            )
            .map_err(|e| anyhow!("{e}"))?;
        self.queue.submit([encoder.finish()]);
        self.window.pre_present_notify();
        self.queue.present(frame);
        Ok(())
    }
}

#[derive(Default)]
struct App {
    gfx: Option<Gfx>,
    error: Option<anyhow::Error>,
}

impl App {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: anyhow::Error) {
        self.error = Some(error);
        event_loop.exit();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gfx.is_none() {
            match Gfx::new(event_loop) {
                Ok(gfx) => self.gfx = Some(gfx),
                Err(e) => self.fail(event_loop, e),
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let Some(gfx) = self.gfx.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => gfx.resize(size),
            WindowEvent::RedrawRequested => {
                if let Err(e) = gfx.draw() {
                    self.fail(event_loop, e);
                }
            }
            _ => {}
        }
    }
}

fn main() -> Result<()> {
    let event_loop = EventLoop::new()?;
    let mut app = App::default();
    event_loop.run_app(&mut app)?;
    app.error.map_or(Ok(()), Err)
}
