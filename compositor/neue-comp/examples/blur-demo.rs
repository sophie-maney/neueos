//! A see-through window that asks for backdrop blur over all of itself
//! (`ext-background-effect-v1`, task 3.4, ADR 0071). For checking the compositor's blur
//! before first-party clients ask for it (task 3.5):
//!
//! ```sh
//! cargo run -p neue-comp --example blur-demo [WIDTH HEIGHT]
//! ```
//!
//! The window is a faint white wash with a darker frame; drag it over something busy.

#[cfg(not(target_os = "linux"))]
fn main() {}

#[cfg(target_os = "linux")]
fn main() -> anyhow::Result<()> {
    demo::run()
}

#[cfg(target_os = "linux")]
mod demo {
    use std::os::fd::AsFd;

    use anyhow::{Context, bail};
    use wayland_client::globals::{GlobalListContents, registry_queue_init};
    use wayland_client::protocol::{
        wl_buffer::WlBuffer, wl_compositor::WlCompositor, wl_region::WlRegion,
        wl_registry::WlRegistry, wl_shm, wl_shm::WlShm, wl_shm_pool::WlShmPool,
        wl_surface::WlSurface,
    };
    use wayland_client::{Connection, Dispatch, QueueHandle, delegate_noop};
    use wayland_protocols::ext::background_effect::v1::client::{
        ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1,
        ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1,
    };
    use wayland_protocols::xdg::shell::client::{
        xdg_surface::{self, XdgSurface},
        xdg_toplevel::{self, XdgToplevel},
        xdg_wm_base::{self, XdgWmBase},
    };

    /// White at 20% (premultiplied ARGB8888).
    const WASH: u32 = 0x3333_3333;
    /// Black at 40%.
    const FRAME: u32 = 0x6600_0000;

    #[derive(Default)]
    struct Demo {
        /// A configure is waiting to be acked: its serial and size (0: ours).
        configure: Option<(u32, (i32, i32))>,
        /// The size the last `xdg_toplevel.configure` asked for.
        size: (i32, i32),
        closed: bool,
    }

    pub fn run() -> anyhow::Result<()> {
        let mut args = std::env::args().skip(1).map(|a| a.parse::<i32>());
        let size = match (args.next(), args.next()) {
            (Some(w), Some(h)) => (w.context("width")?, h.context("height")?),
            (None, None) => (360, 220),
            _ => bail!("usage: blur-demo [WIDTH HEIGHT]"),
        };

        let conn = Connection::connect_to_env().context("connecting to the compositor")?;
        let (globals, mut queue) = registry_queue_init::<Demo>(&conn)?;
        let qh = queue.handle();
        let compositor: WlCompositor = globals.bind(&qh, 4..=6, ())?;
        let shm: WlShm = globals.bind(&qh, 1..=1, ())?;
        let wm_base: XdgWmBase = globals.bind(&qh, 1..=6, ())?;
        let effects: ExtBackgroundEffectManagerV1 = globals
            .bind(&qh, 1..=1, ())
            .context("the compositor has no ext_background_effect_manager_v1")?;

        let surface = compositor.create_surface(&qh, ());
        let xdg_surface = wm_base.get_xdg_surface(&surface, &qh, ());
        let toplevel = xdg_surface.get_toplevel(&qh, ());
        toplevel.set_title("Blur demo".to_owned());
        toplevel.set_app_id("org.neueos.BlurDemo".to_owned());
        let effect = effects.get_background_effect(&surface, &qh, ());
        surface.commit();

        let mut demo = Demo::default();
        let mut buffer: Option<WlBuffer> = None;
        while !demo.closed {
            queue.blocking_dispatch(&mut demo)?;
            let Some((serial, asked)) = demo.configure.take() else {
                continue;
            };
            xdg_surface.ack_configure(serial);
            let (w, h) = match asked {
                (w, h) if w > 0 && h > 0 => (w, h),
                _ => size,
            };
            let region = compositor.create_region(&qh, ());
            region.add(0, 0, w, h);
            effect.set_blur_region(Some(&region));
            region.destroy();
            let new = shm_buffer(&shm, &qh, (w, h))?;
            surface.attach(Some(&new), 0, 0);
            surface.damage_buffer(0, 0, w, h);
            surface.commit();
            if let Some(old) = buffer.replace(new) {
                old.destroy();
            }
        }
        Ok(())
    }

    /// The wash with a 1-pixel frame, `size` pixels.
    fn shm_buffer(
        shm: &WlShm,
        qh: &QueueHandle<Demo>,
        (w, h): (i32, i32),
    ) -> anyhow::Result<WlBuffer> {
        let fd = rustix::fs::memfd_create("blur-demo", rustix::fs::MemfdFlags::CLOEXEC)?;
        let mut file = std::fs::File::from(fd);
        let pixels: Vec<u8> = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .flat_map(|(x, y)| {
                let edge = x == 0 || y == 0 || x == w - 1 || y == h - 1;
                // wl_shm formats are little-endian.
                (if edge { FRAME } else { WASH }).to_le_bytes()
            })
            .collect();
        std::io::Write::write_all(&mut file, &pixels)?;
        let pool = shm.create_pool(file.as_fd(), w * h * 4, qh, ());
        let buffer = pool.create_buffer(0, w, h, w * 4, wl_shm::Format::Argb8888, qh, ());
        pool.destroy();
        Ok(buffer)
    }

    impl Dispatch<XdgWmBase, ()> for Demo {
        fn event(
            _: &mut Self,
            wm_base: &XdgWmBase,
            event: xdg_wm_base::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            if let xdg_wm_base::Event::Ping { serial } = event {
                wm_base.pong(serial);
            }
        }
    }

    impl Dispatch<XdgSurface, ()> for Demo {
        fn event(
            demo: &mut Self,
            _: &XdgSurface,
            event: xdg_surface::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            if let xdg_surface::Event::Configure { serial } = event {
                demo.configure = Some((serial, demo.size));
            }
        }
    }

    impl Dispatch<XdgToplevel, ()> for Demo {
        fn event(
            demo: &mut Self,
            _: &XdgToplevel,
            event: xdg_toplevel::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            match event {
                xdg_toplevel::Event::Configure { width, height, .. } => {
                    demo.size = (width, height);
                }
                xdg_toplevel::Event::Close => demo.closed = true,
                _ => {}
            }
        }
    }

    impl Dispatch<WlRegistry, GlobalListContents> for Demo {
        fn event(
            _: &mut Self,
            _: &WlRegistry,
            _: wayland_client::protocol::wl_registry::Event,
            _: &GlobalListContents,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    delegate_noop!(Demo: ignore WlCompositor);
    delegate_noop!(Demo: ignore WlSurface);
    delegate_noop!(Demo: ignore WlRegion);
    delegate_noop!(Demo: ignore WlShm);
    delegate_noop!(Demo: ignore WlShmPool);
    delegate_noop!(Demo: ignore WlBuffer);
    delegate_noop!(Demo: ignore ExtBackgroundEffectManagerV1);
    delegate_noop!(Demo: ignore ExtBackgroundEffectSurfaceV1);
}
