//! Clipboard, primary selection, data-control and drag-and-drop on the test client.
//!
//! Sources carry their payload as user data and write it when the compositor forwards a
//! `send`, so a paste is: `request_*` on the receiving client (returns the read end of a
//! pipe), a roundtrip on the owner (it writes), then read the pipe.

use std::io::{PipeReader, Read, Write};
use std::os::fd::{AsFd, OwnedFd};

use wayland_client::protocol::{
    wl_data_device::{self, WlDataDevice},
    wl_data_device_manager::{self, WlDataDeviceManager},
    wl_data_offer::WlDataOffer,
    wl_data_source::{self, WlDataSource},
    wl_surface::WlSurface,
};
use wayland_client::{Connection, Dispatch, QueueHandle, delegate_noop, event_created_child};
use wayland_protocols::ext::data_control::v1::client::{
    ext_data_control_device_v1::{self, ExtDataControlDeviceV1},
    ext_data_control_manager_v1::ExtDataControlManagerV1,
    ext_data_control_offer_v1::{self, ExtDataControlOfferV1},
    ext_data_control_source_v1::{self, ExtDataControlSourceV1},
};
use wayland_protocols::wp::primary_selection::zv1::client::{
    zwp_primary_selection_device_manager_v1::ZwpPrimarySelectionDeviceManagerV1,
    zwp_primary_selection_device_v1::{self, ZwpPrimarySelectionDeviceV1},
    zwp_primary_selection_offer_v1::{self, ZwpPrimarySelectionOfferV1},
    zwp_primary_selection_source_v1::{self, ZwpPrimarySelectionSourceV1},
};

use super::{ClientData, TestClient};

/// The protocol objects for selections, bound when the client connects.
pub(super) struct SelectionObjects {
    pub data_manager: WlDataDeviceManager,
    pub data_device: WlDataDevice,
    pub primary_manager: ZwpPrimarySelectionDeviceManagerV1,
    pub primary_device: ZwpPrimarySelectionDeviceV1,
    pub control_manager: ExtDataControlManagerV1,
    pub control_device: ExtDataControlDeviceV1,
}

/// The current selections the compositor has offered this client.
#[derive(Default)]
pub(super) struct Offers {
    pub clipboard: Option<WlDataOffer>,
    pub primary: Option<ZwpPrimarySelectionOfferV1>,
    pub control_clipboard: Option<ExtDataControlOfferV1>,
    pub control_primary: Option<ExtDataControlOfferV1>,
    /// MIME types of every offer seen so far, in announcement order.
    pub mime_types: Vec<String>,
    /// Set when the compositor cancelled one of our sources.
    pub cancelled_sources: usize,
    /// The offer of the drag currently over one of our surfaces. Accepted on entry.
    pub dnd_offer: Option<WlDataOffer>,
    pub dnd_enters: usize,
    pub dnd_drops: usize,
    /// Source side of a drag: `dnd_drop_performed` and `dnd_finished` events.
    pub dnd_performed: usize,
    pub dnd_finished: usize,
}

impl SelectionObjects {
    pub(super) fn new(
        globals: &wayland_client::globals::GlobalList,
        qh: &QueueHandle<ClientData>,
        seat: &wayland_client::protocol::wl_seat::WlSeat,
    ) -> Self {
        let data_manager: WlDataDeviceManager = globals
            .bind(qh, 1..=3, ())
            .expect("binding wl_data_device_manager");
        let primary_manager: ZwpPrimarySelectionDeviceManagerV1 = globals
            .bind(qh, 1..=1, ())
            .expect("binding zwp_primary_selection_device_manager_v1");
        let control_manager: ExtDataControlManagerV1 = globals
            .bind(qh, 1..=1, ())
            .expect("binding ext_data_control_manager_v1");
        Self {
            data_device: data_manager.get_data_device(seat, qh, ()),
            primary_device: primary_manager.get_device(seat, qh, ()),
            control_device: control_manager.get_data_device(seat, qh, ()),
            data_manager,
            primary_manager,
            control_manager,
        }
    }
}

/// A pipe: the write end goes to the compositor, the read end stays with the test.
fn pipe() -> (PipeReader, OwnedFd) {
    let (read, write) = std::io::pipe().expect("creating a pipe");
    (read, OwnedFd::from(write))
}

/// Read what the owner wrote, once it has answered the `send`.
pub fn read_all(mut pipe: PipeReader) -> Vec<u8> {
    let mut out = Vec::new();
    pipe.read_to_end(&mut out).expect("reading the pipe");
    out
}

fn write_payload(fd: OwnedFd, payload: &[u8]) {
    let mut file = std::fs::File::from(fd);
    file.write_all(payload).expect("writing the payload");
}

/// A drag in progress, as the dragging client sees it.
pub struct Drag {
    pub source: WlDataSource,
    pub icon: Option<WlSurface>,
}

impl TestClient {
    /// MIME types the compositor has announced to this client, in order.
    pub fn offered_mime_types(&self) -> &[String] {
        &self.data.offers.mime_types
    }

    /// Drag events seen as a target: `(enters, drops)`.
    pub fn dnd_target_events(&self) -> (usize, usize) {
        (self.data.offers.dnd_enters, self.data.offers.dnd_drops)
    }

    /// Drag events seen as the source: `(drop_performed, finished)`.
    pub fn dnd_source_events(&self) -> (usize, usize) {
        (
            self.data.offers.dnd_performed,
            self.data.offers.dnd_finished,
        )
    }

    /// How many of our sources the compositor has told to stop (replaced selections,
    /// rejected drops).
    pub fn cancelled_sources(&self) -> usize {
        self.data.offers.cancelled_sources
    }

    /// Become the clipboard owner with `payload` under `mime`. Needs keyboard focus and
    /// the serial of its `enter`.
    pub fn set_clipboard(&mut self, serial: u32, mime: &str, payload: &[u8]) {
        let source = self
            .selection
            .data_manager
            .create_data_source(&self.qh, payload.to_vec());
        source.offer(mime.to_owned());
        self.selection
            .data_device
            .set_selection(Some(&source), serial);
        self.roundtrip();
    }

    /// Ask for the clipboard in `mime`. Returns the read end; let the owner roundtrip
    /// before reading it.
    pub fn request_clipboard(&mut self, mime: &str) -> Option<PipeReader> {
        self.roundtrip();
        let offer = self.data.offers.clipboard.as_ref()?;
        let (read, write) = pipe();
        offer.receive(mime.to_owned(), write.as_fd());
        self.roundtrip();
        Some(read)
    }

    /// Whether this client has been offered a clipboard.
    pub fn has_clipboard(&mut self) -> bool {
        self.roundtrip();
        self.data.offers.clipboard.is_some()
    }

    pub fn set_primary(&mut self, serial: u32, mime: &str, payload: &[u8]) {
        let source = self
            .selection
            .primary_manager
            .create_source(&self.qh, payload.to_vec());
        source.offer(mime.to_owned());
        self.selection
            .primary_device
            .set_selection(Some(&source), serial);
        self.roundtrip();
    }

    pub fn request_primary(&mut self, mime: &str) -> Option<PipeReader> {
        self.roundtrip();
        let offer = self.data.offers.primary.as_ref()?;
        let (read, write) = pipe();
        offer.receive(mime.to_owned(), write.as_fd());
        self.roundtrip();
        Some(read)
    }

    /// `wl-copy`: set the clipboard (or the primary selection) through ext-data-control,
    /// without focus.
    pub fn control_copy(&mut self, primary: bool, mime: &str, payload: &[u8]) {
        let source = self
            .selection
            .control_manager
            .create_data_source(&self.qh, payload.to_vec());
        source.offer(mime.to_owned());
        if primary {
            self.selection
                .control_device
                .set_primary_selection(Some(&source));
        } else {
            self.selection.control_device.set_selection(Some(&source));
        }
        self.roundtrip();
    }

    /// `wl-paste`: read the clipboard (or primary selection) through ext-data-control,
    /// without focus. Returns the read end, like [`TestClient::request_clipboard`].
    pub fn control_paste(&mut self, primary: bool, mime: &str) -> Option<PipeReader> {
        self.roundtrip();
        let offer = if primary {
            self.data.offers.control_primary.as_ref()
        } else {
            self.data.offers.control_clipboard.as_ref()
        }?;
        let (read, write) = pipe();
        offer.receive(mime.to_owned(), write.as_fd());
        self.roundtrip();
        Some(read)
    }

    /// Start a drag from `origin` with `serial` (the button press), optionally with an
    /// icon surface carrying a small buffer.
    pub fn start_drag(&mut self, origin: &WlSurface, serial: u32, with_icon: bool) -> Drag {
        let source = self
            .selection
            .data_manager
            .create_data_source(&self.qh, b"dragged".to_vec());
        source.offer("text/uri-list".to_owned());
        source.set_actions(wl_data_device_manager::DndAction::Copy);
        let icon = with_icon.then(|| {
            let icon = self.compositor.create_surface(&self.qh, ());
            let buffer = self.shm_buffer((16, 16));
            icon.attach(Some(&buffer), 0, 0);
            icon.damage_buffer(0, 0, 16, 16);
            icon.commit();
            icon
        });
        self.selection
            .data_device
            .start_drag(Some(&source), origin, icon.as_ref(), serial);
        self.roundtrip();
        Drag { source, icon }
    }
}

impl Dispatch<WlDataDevice, ()> for ClientData {
    event_created_child!(ClientData, WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (WlDataOffer, ()),
    ]);

    fn event(
        data: &mut Self,
        _: &WlDataDevice,
        event: wl_data_device::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_device::Event::Selection { id } => data.offers.clipboard = id,
            // Take every drag, as a drop target that accepts anything would.
            wl_data_device::Event::Enter { serial, id, .. } => {
                data.offers.dnd_enters += 1;
                if let Some(offer) = &id {
                    offer.accept(serial, Some("text/uri-list".to_owned()));
                    offer.set_actions(
                        wl_data_device_manager::DndAction::Copy,
                        wl_data_device_manager::DndAction::Copy,
                    );
                }
                data.offers.dnd_offer = id;
            }
            wl_data_device::Event::Drop => {
                data.offers.dnd_drops += 1;
                if let Some(offer) = data.offers.dnd_offer.take() {
                    offer.finish();
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<WlDataOffer, ()> for ClientData {
    fn event(
        data: &mut Self,
        _: &WlDataOffer,
        event: wayland_client::protocol::wl_data_offer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wayland_client::protocol::wl_data_offer::Event::Offer { mime_type } = event {
            data.offers.mime_types.push(mime_type);
        }
    }
}

impl Dispatch<WlDataSource, Vec<u8>> for ClientData {
    fn event(
        data: &mut Self,
        _: &WlDataSource,
        event: wl_data_source::Event,
        payload: &Vec<u8>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_source::Event::Send { fd, .. } => write_payload(fd, payload),
            wl_data_source::Event::Cancelled => data.offers.cancelled_sources += 1,
            wl_data_source::Event::DndDropPerformed => data.offers.dnd_performed += 1,
            wl_data_source::Event::DndFinished => data.offers.dnd_finished += 1,
            _ => {}
        }
    }
}

impl Dispatch<ZwpPrimarySelectionDeviceV1, ()> for ClientData {
    event_created_child!(ClientData, ZwpPrimarySelectionDeviceV1, [
        zwp_primary_selection_device_v1::EVT_DATA_OFFER_OPCODE => (ZwpPrimarySelectionOfferV1, ()),
    ]);

    fn event(
        data: &mut Self,
        _: &ZwpPrimarySelectionDeviceV1,
        event: zwp_primary_selection_device_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwp_primary_selection_device_v1::Event::Selection { id } = event {
            data.offers.primary = id;
        }
    }
}

impl Dispatch<ZwpPrimarySelectionOfferV1, ()> for ClientData {
    fn event(
        data: &mut Self,
        _: &ZwpPrimarySelectionOfferV1,
        event: zwp_primary_selection_offer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwp_primary_selection_offer_v1::Event::Offer { mime_type } = event {
            data.offers.mime_types.push(mime_type);
        }
    }
}

impl Dispatch<ZwpPrimarySelectionSourceV1, Vec<u8>> for ClientData {
    fn event(
        data: &mut Self,
        _: &ZwpPrimarySelectionSourceV1,
        event: zwp_primary_selection_source_v1::Event,
        payload: &Vec<u8>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_primary_selection_source_v1::Event::Send { fd, .. } => write_payload(fd, payload),
            zwp_primary_selection_source_v1::Event::Cancelled => data.offers.cancelled_sources += 1,
            _ => {}
        }
    }
}

impl Dispatch<ExtDataControlDeviceV1, ()> for ClientData {
    event_created_child!(ClientData, ExtDataControlDeviceV1, [
        ext_data_control_device_v1::EVT_DATA_OFFER_OPCODE => (ExtDataControlOfferV1, ()),
    ]);

    fn event(
        data: &mut Self,
        _: &ExtDataControlDeviceV1,
        event: ext_data_control_device_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_data_control_device_v1::Event::Selection { id } => {
                data.offers.control_clipboard = id;
            }
            ext_data_control_device_v1::Event::PrimarySelection { id } => {
                data.offers.control_primary = id;
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtDataControlOfferV1, ()> for ClientData {
    fn event(
        data: &mut Self,
        _: &ExtDataControlOfferV1,
        event: ext_data_control_offer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_data_control_offer_v1::Event::Offer { mime_type } = event {
            data.offers.mime_types.push(mime_type);
        }
    }
}

impl Dispatch<ExtDataControlSourceV1, Vec<u8>> for ClientData {
    fn event(
        data: &mut Self,
        _: &ExtDataControlSourceV1,
        event: ext_data_control_source_v1::Event,
        payload: &Vec<u8>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_data_control_source_v1::Event::Send { fd, .. } => write_payload(fd, payload),
            ext_data_control_source_v1::Event::Cancelled => data.offers.cancelled_sources += 1,
            _ => {}
        }
    }
}

delegate_noop!(ClientData: ignore WlDataDeviceManager);
delegate_noop!(ClientData: ignore ZwpPrimarySelectionDeviceManagerV1);
delegate_noop!(ClientData: ignore ExtDataControlManagerV1);
