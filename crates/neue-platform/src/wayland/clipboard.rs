//! Text clipboard (`wl_data_device`) and primary selection (`zwp_primary_selection_v1`)
//! for the Wayland driver (task 2.6, ADR 0034 §2).
//!
//! - **Copy:** Masonry's text widgets handle Ctrl-C and Ctrl-X and send
//!   `ClipboardStore`; the driver offers the text under the usual text MIME types and
//!   answers `send` requests itself until another client takes the selection.
//! - **Paste:** Ctrl-V and Shift-Insert never reach Masonry. The driver reads the
//!   selection through a pipe on the event loop (never blocking: the source may be slow,
//!   or be us) and hands the text over as `TextEvent::ClipboardPaste`. Our own selection
//!   is pasted straight from memory.
//! - **Primary selection:** a middle click pastes it the same way. Masonry doesn't report
//!   text selections, so the driver doesn't own the primary selection yet (task 2.28).

use std::io::{ErrorKind, Read as _, Write as _};

use masonry_core::core::TextEvent;
use smithay_client_toolkit::data_device_manager::data_device::{DataDevice, DataDeviceHandler};
use smithay_client_toolkit::data_device_manager::data_offer::{DataOfferHandler, DragOffer};
use smithay_client_toolkit::data_device_manager::data_source::{
    CopyPasteSource, DataSourceHandler,
};
use smithay_client_toolkit::data_device_manager::{DataDeviceManagerState, ReadPipe, WritePipe};
use smithay_client_toolkit::primary_selection::PrimarySelectionManagerState;
use smithay_client_toolkit::primary_selection::device::{
    PrimarySelectionDevice, PrimarySelectionDeviceHandler,
};
use smithay_client_toolkit::primary_selection::selection::PrimarySelectionSourceHandler;
use smithay_client_toolkit::reexports::calloop::PostAction;
use wayland_client::globals::GlobalList;
use wayland_client::protocol::wl_data_device::WlDataDevice;
use wayland_client::protocol::wl_data_device_manager::DndAction;
use wayland_client::protocol::wl_data_source::WlDataSource;
use wayland_client::protocol::{wl_seat, wl_surface};
use wayland_client::{Connection, QueueHandle};
use wayland_protocols::wp::primary_selection::zv1::client::{
    zwp_primary_selection_device_v1::ZwpPrimarySelectionDeviceV1,
    zwp_primary_selection_source_v1::ZwpPrimarySelectionSourceV1,
};

use super::State;
use crate::WindowId;

/// Text MIME types, most preferred first: what we offer, and what we ask for.
const TEXT_MIMES: [&str; 5] = [
    "text/plain;charset=utf-8",
    "UTF8_STRING",
    "text/plain",
    "TEXT",
    "STRING",
];

/// Pastes bigger than this are dropped.
const MAX_PASTE: usize = 64 << 20;

/// Writes of at most `PIPE_BUF` bytes to a pipe that polls writable never block, so the
/// driver needs no `O_NONBLOCK` (and no `fcntl` binding).
const PIPE_BUF: usize = 4096;

/// Which selection to paste.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Selection {
    Clipboard,
    Primary,
}

/// The seat's data devices and the selection we own.
#[derive(Default)]
pub(super) struct Clipboard {
    manager: Option<DataDeviceManagerState>,
    primary_manager: Option<PrimarySelectionManagerState>,
    device: Option<DataDevice>,
    primary_device: Option<PrimarySelectionDevice>,
    /// Our clipboard selection and its text, until another client takes it.
    owned: Option<(CopyPasteSource, String)>,
}

impl Clipboard {
    pub(super) fn bind(globals: &GlobalList, qh: &QueueHandle<State>) -> Self {
        let manager = DataDeviceManagerState::bind(globals, qh).ok();
        let primary_manager = PrimarySelectionManagerState::bind(globals, qh).ok();
        if manager.is_none() {
            tracing::warn!("compositor lacks wl_data_device_manager: no clipboard");
        }
        if primary_manager.is_none() {
            tracing::warn!("compositor lacks zwp_primary_selection_device_manager_v1");
        }
        Self {
            manager,
            primary_manager,
            ..Self::default()
        }
    }

    /// Creates the data devices for the seat the driver uses.
    pub(super) fn adopt_seat(&mut self, qh: &QueueHandle<State>, seat: &wl_seat::WlSeat) {
        self.device = self.manager.as_ref().map(|m| m.get_data_device(qh, seat));
        self.primary_device = self
            .primary_manager
            .as_ref()
            .map(|m| m.get_selection_device(qh, seat));
    }
}

/// The first of our text MIME types that `offered` contains.
fn pick_text_mime(offered: &[String]) -> Option<&'static str> {
    TEXT_MIMES
        .into_iter()
        .find(|mime| offered.iter().any(|o| o == mime))
}

/// Ctrl-V (Ctrl-Shift-V too, as in terminals) or Shift-Insert.
pub(super) fn is_paste(key: &masonry_core::core::KeyboardEvent) -> bool {
    use masonry_core::core::keyboard::{Key, KeyState, NamedKey};
    let m = key.modifiers;
    key.state == KeyState::Down
        && !m.alt()
        && !m.meta()
        && match &key.key {
            Key::Character(c) => m.ctrl() && c.eq_ignore_ascii_case("v"),
            Key::Named(NamedKey::Insert) => m.shift() && !m.ctrl(),
            _ => false,
        }
}

impl State {
    /// Takes the clipboard selection with `text` (Masonry's `ClipboardStore`), using the
    /// serial of the input event that asked for it.
    pub(super) fn store_clipboard(&mut self, text: String) {
        let cb = &mut self.clipboard;
        let (Some(manager), Some(device), Some(serial)) =
            (&cb.manager, &cb.device, self.input.last_serial)
        else {
            tracing::debug!("no data device or input serial: not copying");
            return;
        };
        let source = manager.create_copy_paste_source(&self.qh, TEXT_MIMES);
        source.set_selection(device, serial);
        tracing::debug!(bytes = text.len(), "took the clipboard selection");
        // Dropping the previous source destroys it; the compositor already replaced it.
        cb.owned = Some((source, text));
    }

    /// Reads `which` selection and pastes its text into `window` once it has arrived.
    pub(super) fn paste(&mut self, window: WindowId, which: Selection) {
        let pipe = match which {
            Selection::Clipboard => {
                if let Some((_, text)) = &self.clipboard.owned {
                    let text = text.clone();
                    self.deliver_paste(window, text);
                    return;
                }
                let Some(offer) = self
                    .clipboard
                    .device
                    .as_ref()
                    .and_then(|d| d.data().selection_offer())
                else {
                    return;
                };
                let Some(mime) = offer.with_mime_types(pick_text_mime) else {
                    tracing::debug!("the clipboard holds no text");
                    return;
                };
                offer.receive(mime.to_owned()).map_err(|e| e.to_string())
            }
            Selection::Primary => {
                let Some(offer) = self
                    .clipboard
                    .primary_device
                    .as_ref()
                    .and_then(|d| d.data().selection_offer())
                else {
                    return;
                };
                let Some(mime) = offer.with_mime_types(pick_text_mime) else {
                    tracing::debug!("the primary selection holds no text");
                    return;
                };
                offer.receive(mime.to_owned()).map_err(|e| e.to_string())
            }
        };
        match pipe {
            Ok(pipe) => self.read_paste(window, pipe),
            Err(err) => tracing::warn!(%err, ?which, "can't read the selection"),
        }
    }

    /// Reads `pipe` to the end on the event loop, then pastes what it got.
    fn read_paste(&mut self, window: WindowId, pipe: ReadPipe) {
        // The source only learns about the request once it reaches the compositor.
        if let Err(err) = self.conn.flush() {
            tracing::warn!(%err, "flushing the Wayland connection failed");
        }
        let mut data = Vec::new();
        let inserted = self
            .loop_handle
            .insert_source(pipe, move |(), file, state| {
                let mut chunk = [0u8; 16 * 1024];
                // `&File` reads without the unsafe `NoIoDrop::get_mut`.
                match (&**file).read(&mut chunk) {
                    Ok(0) => {
                        let text = String::from_utf8_lossy(&data).into_owned();
                        state.deliver_paste(window, text);
                        PostAction::Remove
                    }
                    Ok(n) if data.len() + n > MAX_PASTE => {
                        tracing::warn!("paste over {MAX_PASTE} bytes dropped");
                        PostAction::Remove
                    }
                    Ok(n) => {
                        data.extend_from_slice(&chunk[..n]);
                        PostAction::Continue
                    }
                    Err(err)
                        if matches!(err.kind(), ErrorKind::Interrupted | ErrorKind::WouldBlock) =>
                    {
                        PostAction::Continue
                    }
                    Err(err) => {
                        tracing::warn!(%err, "reading the selection failed");
                        PostAction::Remove
                    }
                }
            });
        if let Err(err) = inserted {
            tracing::warn!(%err, "can't watch the selection pipe");
        }
    }

    fn deliver_paste(&mut self, window: WindowId, text: String) {
        if text.is_empty() {
            return;
        }
        if let Some(w) = self.reg.windows.get_mut(&window) {
            tracing::debug!(%window, bytes = text.len(), "paste");
            w.root.handle_text_event(TextEvent::ClipboardPaste(text));
        }
    }

    /// Writes `text` to `pipe` on the event loop, `PIPE_BUF` bytes at a time.
    fn write_selection(&mut self, pipe: WritePipe, text: String) {
        let mut written = 0;
        let inserted = self.loop_handle.insert_source(pipe, move |(), file, _| {
            let end = (written + PIPE_BUF).min(text.len());
            match (&**file).write(&text.as_bytes()[written..end]) {
                Ok(n) => {
                    written += n;
                    if written >= text.len() {
                        PostAction::Remove
                    } else {
                        PostAction::Continue
                    }
                }
                Err(err)
                    if matches!(err.kind(), ErrorKind::Interrupted | ErrorKind::WouldBlock) =>
                {
                    PostAction::Continue
                }
                Err(err) => {
                    // Usually the reader gave up (EPIPE).
                    tracing::debug!(%err, "writing the selection failed");
                    PostAction::Remove
                }
            }
        });
        if let Err(err) = inserted {
            tracing::warn!(%err, "can't watch the selection pipe");
        }
    }
}

impl DataDeviceHandler for State {
    // Selections are read when pasting; drag and drop isn't supported yet.
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlDataDevice,
        _: f64,
        _: f64,
        _: &wl_surface::WlSurface,
    ) {
    }
    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}
    fn motion(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice, _: f64, _: f64) {}
    fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}
    fn drop_performed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}
}

impl DataOfferHandler for State {
    fn source_actions(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        _: DndAction,
    ) {
    }
    fn selected_action(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        _: DndAction,
    ) {
    }
}

impl DataSourceHandler for State {
    fn accept_mime(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlDataSource,
        _: Option<String>,
    ) {
    }

    fn send_request(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        source: &WlDataSource,
        mime: String,
        pipe: WritePipe,
    ) {
        let text = match &self.clipboard.owned {
            Some((owned, text)) if owned.inner() == source && TEXT_MIMES.contains(&&*mime) => {
                text.clone()
            }
            // Dropping the pipe closes it: the reader gets nothing.
            _ => return,
        };
        self.write_selection(pipe, text);
    }

    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
        if matches!(&self.clipboard.owned, Some((owned, _)) if owned.inner() == source) {
            tracing::debug!("another client took the clipboard");
            self.clipboard.owned = None;
        }
    }

    fn dnd_dropped(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}
    fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}
    fn action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: DndAction) {}
}

impl PrimarySelectionDeviceHandler for State {
    fn selection(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &ZwpPrimarySelectionDeviceV1,
    ) {
    }
}

impl PrimarySelectionSourceHandler for State {
    // We never own the primary selection (see the module docs).
    fn send_request(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &ZwpPrimarySelectionSourceV1,
        _: String,
        _: WritePipe,
    ) {
    }
    fn cancelled(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &ZwpPrimarySelectionSourceV1,
    ) {
    }
}

#[cfg(test)]
mod tests {
    use masonry_core::core::KeyboardEvent;
    use masonry_core::core::keyboard::{Code, Key, KeyState, Location, Modifiers, NamedKey};

    use super::*;

    fn key(key: Key, modifiers: Modifiers) -> KeyboardEvent {
        KeyboardEvent {
            state: KeyState::Down,
            key,
            code: Code::Unidentified,
            location: Location::Standard,
            modifiers,
            repeat: false,
            is_composing: false,
        }
    }

    #[test]
    fn paste_keys() {
        let v = || Key::Character("v".into());
        assert!(is_paste(&key(v(), Modifiers::CONTROL)));
        assert!(is_paste(&key(
            Key::Character("V".into()),
            Modifiers::CONTROL | Modifiers::SHIFT
        )));
        assert!(is_paste(&key(
            Key::Named(NamedKey::Insert),
            Modifiers::SHIFT
        )));
        assert!(!is_paste(&key(v(), Modifiers::empty())));
        assert!(!is_paste(&key(v(), Modifiers::CONTROL | Modifiers::ALT)));
        assert!(!is_paste(&key(
            Key::Named(NamedKey::Insert),
            Modifiers::empty()
        )));
        let mut up = key(v(), Modifiers::CONTROL);
        up.state = KeyState::Up;
        assert!(!is_paste(&up));
    }

    #[test]
    fn text_mime_preference() {
        let offered = |m: &[&str]| m.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            pick_text_mime(&offered(&[
                "STRING",
                "text/plain;charset=utf-8",
                "image/png"
            ])),
            Some("text/plain;charset=utf-8")
        );
        assert_eq!(
            pick_text_mime(&offered(&["TEXT", "UTF8_STRING"])),
            Some("UTF8_STRING")
        );
        assert_eq!(pick_text_mime(&offered(&["image/png"])), None);
    }
}
