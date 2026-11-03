//! A scripted Wayland client on `wayland-client`.
//!
//! Every call that talks to the compositor ends with a roundtrip, so tests read like a
//! script and need no event loop of their own.

use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;

use wayland_client::globals::{GlobalList, GlobalListContents, registry_queue_init};
use wayland_client::protocol::{
    wl_buffer::WlBuffer, wl_compositor::WlCompositor, wl_keyboard, wl_keyboard::WlKeyboard,
    wl_pointer, wl_pointer::WlPointer, wl_registry::WlRegistry, wl_seat::WlSeat, wl_shm,
    wl_shm::WlShm, wl_shm_pool::WlShmPool, wl_surface::WlSurface,
};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle, delegate_noop};
use wayland_protocols::xdg::activation::v1::client::{
    xdg_activation_token_v1::{self, XdgActivationTokenV1},
    xdg_activation_v1::XdgActivationV1,
};
use wayland_protocols::xdg::shell::client::{
    xdg_surface::{self, XdgSurface},
    xdg_toplevel::{self, XdgToplevel},
    xdg_wm_base::{self, XdgWmBase},
};

/// A connected client with the core globals bound.
pub struct TestClient {
    queue: EventQueue<ClientData>,
    qh: QueueHandle<ClientData>,
    data: ClientData,
    compositor: WlCompositor,
    shm: WlShm,
    wm_base: XdgWmBase,
    pub seat: WlSeat,
    activation: XdgActivationV1,
    _pointer: WlPointer,
    _keyboard: WlKeyboard,
    _globals: GlobalList,
}

/// A mapped toplevel. Index into [`ClientData::toplevels`] plus its protocol objects.
pub struct Toplevel {
    index: usize,
    pub surface: WlSurface,
    pub xdg_surface: XdgSurface,
    pub toplevel: XdgToplevel,
    buffer: Option<WlBuffer>,
    /// Size of the attached buffer.
    pub size: (i32, i32),
}

/// What the compositor has told us about one toplevel.
#[derive(Debug, Default, Clone)]
pub struct ToplevelEvents {
    /// Serial of the last `xdg_surface.configure`.
    pub configure_serial: Option<u32>,
    /// Size from the last `xdg_toplevel.configure` (0 means "client decides").
    pub size: (i32, i32),
    /// States from the last `xdg_toplevel.configure`.
    pub states: Vec<xdg_toplevel::State>,
    pub close_requested: bool,
}

#[derive(Default)]
struct ClientData {
    toplevels: Vec<ToplevelEvents>,
    /// Serial of the last `wl_pointer.button` press.
    button_serial: Option<u32>,
    /// Serial of the last `wl_keyboard.enter`.
    enter_serial: Option<u32>,
    /// The last activation token the compositor handed out.
    token: Option<String>,
}

impl TestClient {
    pub(super) fn new(stream: UnixStream) -> Self {
        let conn = Connection::from_socket(stream).expect("connecting to the compositor");
        let (globals, mut queue) =
            registry_queue_init::<ClientData>(&conn).expect("listing globals");
        let qh = queue.handle();
        let compositor = globals.bind(&qh, 4..=6, ()).expect("binding wl_compositor");
        let shm = globals.bind(&qh, 1..=1, ()).expect("binding wl_shm");
        let wm_base = globals.bind(&qh, 1..=6, ()).expect("binding xdg_wm_base");
        let seat: WlSeat = globals.bind(&qh, 1..=7, ()).expect("binding wl_seat");
        let activation = globals
            .bind(&qh, 1..=1, ())
            .expect("binding xdg_activation_v1");
        let pointer = seat.get_pointer(&qh, ());
        let keyboard = seat.get_keyboard(&qh, ());
        let mut data = ClientData::default();
        queue.roundtrip(&mut data).expect("initial roundtrip");
        Self {
            queue,
            qh,
            data,
            compositor,
            shm,
            wm_base,
            seat,
            activation,
            _pointer: pointer,
            _keyboard: keyboard,
            _globals: globals,
        }
    }

    /// Serial of the last pointer button press this client saw.
    pub fn button_serial(&self) -> u32 {
        self.data
            .button_serial
            .expect("no pointer button press seen")
    }

    /// Serial of the last keyboard enter this client saw.
    pub fn enter_serial(&self) -> Option<u32> {
        self.data.enter_serial
    }

    /// Ask for an xdg-activation token, optionally tied to an input `serial` and the
    /// requesting `surface`.
    pub fn activation_token(&mut self, serial: Option<u32>, surface: Option<&WlSurface>) -> String {
        self.data.token = None;
        let token = self.activation.get_activation_token(&self.qh, ());
        if let Some(serial) = serial {
            token.set_serial(serial, &self.seat);
        }
        if let Some(surface) = surface {
            token.set_surface(surface);
        }
        token.commit();
        self.roundtrip();
        token.destroy();
        self.data.token.take().expect("no activation token")
    }

    /// Ask the compositor to activate `toplevel` with `token`.
    pub fn activate(&mut self, toplevel: &Toplevel, token: &str) {
        self.activation
            .activate(token.to_owned(), &toplevel.surface);
        self.roundtrip();
    }

    /// Ack the last configure and commit a buffer of the configured size (or the current
    /// size when the compositor leaves it to us).
    pub fn ack_and_commit(&mut self, toplevel: &mut Toplevel) {
        self.roundtrip();
        let events = self.data.toplevels[toplevel.index].clone();
        let serial = events.configure_serial.expect("no configure to ack");
        toplevel.xdg_surface.ack_configure(serial);
        let size = match events.size {
            (w, h) if w > 0 && h > 0 => (w, h),
            _ => toplevel.size,
        };
        self.attach_buffer(toplevel, size);
        toplevel.surface.commit();
        self.roundtrip();
    }

    /// Send pending requests and process every event the compositor sends in reply.
    pub fn roundtrip(&mut self) {
        self.queue
            .roundtrip(&mut self.data)
            .expect("roundtrip with the compositor");
    }

    /// Events received for `toplevel`.
    pub fn events(&self, toplevel: &Toplevel) -> &ToplevelEvents {
        &self.data.toplevels[toplevel.index]
    }

    /// Create an `xdg_toplevel`, do the initial configure dance and commit a `size` shm
    /// buffer, so the compositor maps it. Uses the configured size if the compositor
    /// sets one.
    pub fn create_toplevel(&mut self, title: &str, app_id: &str, size: (i32, i32)) -> Toplevel {
        self.create_toplevel_with(title, app_id, size, |_| {})
    }

    /// [`TestClient::create_toplevel`], calling `setup` on the `xdg_toplevel` before the
    /// first commit (e.g. to set a parent or ask for the maximized state).
    pub fn create_toplevel_with(
        &mut self,
        title: &str,
        app_id: &str,
        size: (i32, i32),
        setup: impl FnOnce(&XdgToplevel),
    ) -> Toplevel {
        let index = self.data.toplevels.len();
        self.data.toplevels.push(ToplevelEvents::default());

        let surface = self.compositor.create_surface(&self.qh, ());
        let xdg_surface = self.wm_base.get_xdg_surface(&surface, &self.qh, index);
        let toplevel = xdg_surface.get_toplevel(&self.qh, index);
        toplevel.set_title(title.to_owned());
        toplevel.set_app_id(app_id.to_owned());
        setup(&toplevel);
        surface.commit();
        self.roundtrip();

        let events = &self.data.toplevels[index];
        let serial = events
            .configure_serial
            .expect("no initial configure after the first commit");
        let size = match events.size {
            (w, h) if w > 0 && h > 0 => (w, h),
            _ => size,
        };
        xdg_surface.ack_configure(serial);

        let mut toplevel = Toplevel {
            index,
            surface,
            xdg_surface,
            toplevel,
            buffer: None,
            size,
        };
        self.attach_buffer(&mut toplevel, size);
        toplevel.surface.commit();
        self.roundtrip();
        toplevel
    }

    /// Destroy `toplevel` and its surface (unmaps the window).
    pub fn destroy_toplevel(&mut self, toplevel: Toplevel) {
        toplevel.toplevel.destroy();
        toplevel.xdg_surface.destroy();
        toplevel.surface.destroy();
        if let Some(buffer) = toplevel.buffer {
            buffer.destroy();
        }
        self.roundtrip();
    }

    /// Attach a new transparent ARGB8888 shm buffer of `size` to the toplevel's surface.
    fn attach_buffer(&mut self, toplevel: &mut Toplevel, (w, h): (i32, i32)) {
        let stride = w * 4;
        let len = stride * h;
        let fd = rustix::fs::memfd_create("neue-comp-test", rustix::fs::MemfdFlags::CLOEXEC)
            .expect("memfd_create");
        let file = std::fs::File::from(fd);
        file.set_len(len as u64).expect("sizing the shm file");
        let pool = self.shm.create_pool(file.as_fd(), len, &self.qh, ());
        let buffer = pool.create_buffer(0, w, h, stride, wl_shm::Format::Argb8888, &self.qh, ());
        pool.destroy();
        toplevel.surface.attach(Some(&buffer), 0, 0);
        toplevel.surface.damage_buffer(0, 0, w, h);
        if let Some(old) = toplevel.buffer.replace(buffer) {
            old.destroy();
        }
        toplevel.size = (w, h);
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for ClientData {
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

impl Dispatch<XdgWmBase, ()> for ClientData {
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

impl Dispatch<XdgSurface, usize> for ClientData {
    fn event(
        data: &mut Self,
        _: &XdgSurface,
        event: xdg_surface::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            data.toplevels[*index].configure_serial = Some(serial);
        }
    }
}

impl Dispatch<XdgToplevel, usize> for ClientData {
    fn event(
        data: &mut Self,
        _: &XdgToplevel,
        event: xdg_toplevel::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let events = &mut data.toplevels[*index];
        match event {
            xdg_toplevel::Event::Configure {
                width,
                height,
                states,
            } => {
                events.size = (width, height);
                let (chunks, _) = states.as_chunks::<4>();
                events.states = chunks
                    .iter()
                    .filter_map(|c| xdg_toplevel::State::try_from(u32::from_ne_bytes(*c)).ok())
                    .collect();
            }
            xdg_toplevel::Event::Close => events.close_requested = true,
            _ => {}
        }
    }
}

impl Dispatch<WlPointer, ()> for ClientData {
    fn event(
        data: &mut Self,
        _: &WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_pointer::Event::Button {
            serial,
            state: wayland_client::WEnum::Value(wl_pointer::ButtonState::Pressed),
            ..
        } = event
        {
            data.button_serial = Some(serial);
        }
    }
}

impl Dispatch<WlKeyboard, ()> for ClientData {
    fn event(
        data: &mut Self,
        _: &WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_keyboard::Event::Enter { serial, .. } = event {
            data.enter_serial = Some(serial);
        }
    }
}

impl Dispatch<XdgActivationTokenV1, ()> for ClientData {
    fn event(
        data: &mut Self,
        _: &XdgActivationTokenV1,
        event: xdg_activation_token_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_activation_token_v1::Event::Done { token } = event {
            data.token = Some(token);
        }
    }
}

delegate_noop!(ClientData: ignore WlSeat);
delegate_noop!(ClientData: ignore XdgActivationV1);
delegate_noop!(ClientData: ignore WlCompositor);
delegate_noop!(ClientData: ignore WlSurface);
delegate_noop!(ClientData: ignore WlShm);
delegate_noop!(ClientData: ignore WlShmPool);
delegate_noop!(ClientData: ignore WlBuffer);
