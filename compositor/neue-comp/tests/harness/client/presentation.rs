//! Presentation and sync protocols on the test client (task 1.14): `wp_presentation`
//! feedback, `wp_tearing_control_v1`, `wp_security_context_v1`, and a bare client that only
//! lists globals (for sandboxed connections, which cannot bind everything
//! [`TestClient`] binds).

use std::os::fd::AsFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::Duration;

use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_registry::WlRegistry, wl_surface::WlSurface};
use wayland_client::{Connection, Dispatch, DispatchError, QueueHandle, WEnum, delegate_noop};
use wayland_protocols::wp::presentation_time::client::{
    wp_presentation::{self, WpPresentation},
    wp_presentation_feedback::{self, WpPresentationFeedback},
};
use wayland_protocols::wp::security_context::v1::client::{
    wp_security_context_manager_v1::WpSecurityContextManagerV1,
    wp_security_context_v1::WpSecurityContextV1,
};
use wayland_protocols::wp::tearing_control::v1::client::{
    wp_tearing_control_manager_v1::WpTearingControlManagerV1,
    wp_tearing_control_v1::WpTearingControlV1,
};

use super::{ClientData, TestClient, Toplevel};

/// What became of a `wp_presentation_feedback`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feedback {
    Pending,
    Presented {
        /// Nanoseconds between refreshes (0: unknown).
        refresh_ns: u32,
        sequence: u64,
        flags: u32,
        /// Seconds part of the presentation time.
        tv_sec: u64,
    },
    Discarded,
}

#[derive(Default)]
pub(super) struct PresentationData {
    presentation: Option<WpPresentation>,
    clock_id: Option<u32>,
    feedback: Vec<Feedback>,
    tearing: Option<WpTearingControlManagerV1>,
}

impl TestClient {
    fn bind_presentation(&mut self) -> WpPresentation {
        if let Some(presentation) = &self.data.presentation.presentation {
            return presentation.clone();
        }
        let presentation: WpPresentation = self
            ._globals
            .bind(&self.qh, 1..=2, ())
            .expect("binding wp_presentation");
        self.data.presentation.presentation = Some(presentation.clone());
        self.roundtrip();
        presentation
    }

    /// The clock `wp_presentation` timestamps use (a `clockid_t`).
    pub fn presentation_clock(&mut self) -> u32 {
        self.bind_presentation();
        self.data.presentation.clock_id.expect("no clock_id event")
    }

    /// Ask for presentation feedback on `surface`'s next commit; returns its index.
    pub fn request_feedback(&mut self, surface: &WlSurface) -> usize {
        let presentation = self.bind_presentation();
        let index = self.data.presentation.feedback.len();
        self.data.presentation.feedback.push(Feedback::Pending);
        presentation.feedback(surface, &self.qh, index);
        index
    }

    /// What happened to feedback `index` so far.
    pub fn feedback(&self, index: usize) -> Feedback {
        self.data.presentation.feedback[index]
    }

    /// Wait (up to two seconds) until feedback `index` is presented or discarded.
    pub fn wait_for_feedback(&mut self, index: usize) -> Feedback {
        for _ in 0..200 {
            self.roundtrip();
            if self.feedback(index) != Feedback::Pending {
                return self.feedback(index);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Feedback::Pending
    }

    /// Commit a fresh buffer of the toplevel's size (a new frame of content).
    pub fn commit_frame(&mut self, toplevel: &mut Toplevel) {
        self.attach_buffer(toplevel, toplevel.size);
        toplevel.surface.commit();
    }

    /// A bare `wl_surface` with no role and no buffer.
    pub fn create_bare_surface(&mut self) -> WlSurface {
        self.compositor.create_surface(&self.qh, ())
    }

    /// `wp_tearing_control_manager_v1.get_tearing_control` for `surface` (not flushed).
    pub fn tearing_control(&mut self, surface: &WlSurface) -> WpTearingControlV1 {
        let manager = match &self.data.presentation.tearing {
            Some(manager) => manager.clone(),
            None => {
                let manager: WpTearingControlManagerV1 = self
                    ._globals
                    .bind(&self.qh, 1..=1, ())
                    .expect("binding wp_tearing_control_manager_v1");
                self.data.presentation.tearing = Some(manager.clone());
                manager
            }
        };
        manager.get_tearing_control(surface, &self.qh, ())
    }

    /// Roundtrip, expecting the compositor to kill the connection with a protocol error;
    /// returns the interface of the offending object and the error code.
    pub fn roundtrip_protocol_error(&mut self) -> Option<(String, u32)> {
        match self.queue.roundtrip(&mut self.data) {
            Ok(_) => None,
            Err(DispatchError::Backend(wayland_client::backend::WaylandError::Protocol(err))) => {
                Some((err.object_interface, err.code))
            }
            Err(err) => panic!("roundtrip failed without a protocol error: {err}"),
        }
    }

    /// Create a security context whose clients connect through `listener`. It lasts until
    /// the write end of `close` is closed. The context is committed and the listener's
    /// file descriptor handed over (drop yours afterwards).
    pub fn create_security_context(
        &mut self,
        listener: &UnixListener,
        close: &impl AsFd,
        app_id: &str,
    ) -> WpSecurityContextV1 {
        let manager: WpSecurityContextManagerV1 = self
            ._globals
            .bind(&self.qh, 1..=1, ())
            .expect("binding wp_security_context_manager_v1");
        let context = manager.create_listener(listener.as_fd(), close.as_fd(), &self.qh, ());
        context.set_sandbox_engine("org.neueos.test".into());
        context.set_app_id(app_id.into());
        context.set_instance_id("1".into());
        context.commit();
        self.roundtrip();
        context
    }
}

/// The globals a fresh connection on `stream` sees, by interface name.
pub fn global_names(stream: UnixStream) -> Vec<String> {
    let conn = Connection::from_socket(stream).expect("connecting to the compositor");
    let (globals, _queue) = registry_queue_init::<Bare>(&conn).expect("listing globals");
    globals
        .contents()
        .with_list(|list| list.iter().map(|g| g.interface.clone()).collect())
}

/// State of the bare client of [`global_names`].
struct Bare;

impl Dispatch<WlRegistry, GlobalListContents> for Bare {
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

impl Dispatch<WpPresentation, ()> for ClientData {
    fn event(
        data: &mut Self,
        _: &WpPresentation,
        event: wp_presentation::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_presentation::Event::ClockId { clk_id } = event {
            data.presentation.clock_id = Some(clk_id);
        }
    }
}

impl Dispatch<WpPresentationFeedback, usize> for ClientData {
    fn event(
        data: &mut Self,
        _: &WpPresentationFeedback,
        event: wp_presentation_feedback::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let slot = &mut data.presentation.feedback[*index];
        match event {
            wp_presentation_feedback::Event::Presented {
                tv_sec_hi,
                tv_sec_lo,
                refresh,
                seq_hi,
                seq_lo,
                flags,
                ..
            } => {
                *slot = Feedback::Presented {
                    refresh_ns: refresh,
                    sequence: (u64::from(seq_hi) << 32) | u64::from(seq_lo),
                    flags: match flags {
                        WEnum::Value(kind) => kind.bits(),
                        WEnum::Unknown(bits) => bits,
                    },
                    tv_sec: (u64::from(tv_sec_hi) << 32) | u64::from(tv_sec_lo),
                };
            }
            wp_presentation_feedback::Event::Discarded => *slot = Feedback::Discarded,
            _ => {}
        }
    }
}

delegate_noop!(ClientData: ignore WpTearingControlManagerV1);
delegate_noop!(ClientData: ignore WpTearingControlV1);
delegate_noop!(ClientData: ignore WpSecurityContextManagerV1);
delegate_noop!(ClientData: ignore WpSecurityContextV1);
