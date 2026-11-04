//! CPU and GPU renders of the translucent materials agree within tolerance.
//!
//! Renders each material tile with `vello_cpu` (the golden reference) and with `vello_gpu`
//! on whatever wgpu adapter the machine has, and compares them. The two backends rasterise
//! and blend slightly differently, so the comparison is a little looser than the goldens' (measured on Apple GPUs:
//! no channel differs by more than 2): at most
//! [`PIXEL_TOLERANCE`] of the channel values may differ by more than [`CHANNEL_TOLERANCE`].
//! Without an adapter the test prints a note and passes (CI machines and the headless VM
//! may have none).

#![allow(
    clippy::unwrap_used,
    reason = "test helpers; a panic is a test failure"
)]

use neue_paint::cpu::{CpuRenderer, Quality};
use neue_paint::demo::{self, MaterialTile};
use neue_paint::gpu::GpuRenderer;
use neue_paint::peniko::Color;
use neue_paint::{Appearance, Backdrop, Pixmap, Scene};

/// Largest per-channel difference that still counts as equal, in 0..=255.
const CHANNEL_TOLERANCE: u8 = 3;
/// Share of channel values allowed to exceed [`CHANNEL_TOLERANCE`].
const PIXEL_TOLERANCE: f64 = 0.001;
/// No channel value may differ by more than this.
const WORST_TOLERANCE: u8 = 8;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: GpuRenderer,
}

impl Gpu {
    fn new() -> Option<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .ok()?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()?;
        let renderer = GpuRenderer::new(&device, FORMAT);
        Some(Self {
            device,
            queue,
            renderer,
        })
    }

    /// Render `scene` at `scale` and read the pixels back as straight RGBA8 rows.
    fn render(&mut self, scene: &Scene, scale: f64) -> (u32, u32, Vec<u8>) {
        let width = (scene.width() * scale).ceil() as u32;
        let height = (scene.height() * scale).ceil() as u32;
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("parity target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        self.renderer
            .render(
                scene,
                scale,
                &self.device,
                &self.queue,
                &mut encoder,
                &view,
                Color::TRANSPARENT,
            )
            .unwrap();

        // Rows in a texture-to-buffer copy are padded to 256 bytes.
        let row = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("parity readback"),
            size: u64::from(row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        buffer.map_async(wgpu::MapMode::Read, .., |result| result.unwrap());
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        let mapped = buffer.get_mapped_range(..).unwrap();
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for rows in mapped.chunks(row as usize).take(height as usize) {
            pixels.extend_from_slice(&rows[..(width * 4) as usize]);
        }
        (width, height, pixels)
    }
}

/// The CPU render as straight-alpha RGBA8 (the tiles are opaque, so premultiplied and
/// straight agree).
fn cpu_pixels(pixmap: &Pixmap) -> Vec<u8> {
    pixmap.data_as_u8_slice().to_vec()
}

#[test]
fn materials_render_alike_on_cpu_and_gpu() {
    let Some(mut gpu) = Gpu::new() else {
        eprintln!("no wgpu adapter: skipping the CPU/GPU parity test");
        return;
    };
    let mut cpu = CpuRenderer::new(Quality::Reference);
    let mut report = Vec::new();
    let mut failed = false;
    for tile in MaterialTile::ALL {
        for backdrop in [Backdrop::Opaque, Backdrop::Translucent] {
            for appearance in [Appearance::Light, Appearance::Dark] {
                for scale in [1.0, 2.0] {
                    let scene = demo::material_tile(tile, appearance, backdrop);
                    let expected = cpu_pixels(&cpu.render(&scene, scale).unwrap());
                    let (_, _, actual) = gpu.render(&scene, scale);
                    assert_eq!(expected.len(), actual.len());
                    let diffs = expected.iter().zip(&actual).map(|(e, a)| e.abs_diff(*a));
                    let worst = diffs.clone().max().unwrap_or(0);
                    let bad = diffs.filter(|d| *d > CHANNEL_TOLERANCE).count();
                    let share = bad as f64 / expected.len() as f64;
                    let name = format!("{}-{backdrop:?}-{appearance:?}@{scale}x", tile.name());
                    report.push(format!("{name}: worst {worst}, {:.3}% over", share * 100.0));
                    failed |= share > PIXEL_TOLERANCE || worst > WORST_TOLERANCE;
                }
            }
        }
    }
    eprintln!("{}", report.join("\n"));
    assert!(
        !failed,
        "CPU and GPU renders differ:\n{}",
        report.join("\n")
    );
}
