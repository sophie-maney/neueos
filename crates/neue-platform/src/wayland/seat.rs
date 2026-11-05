//! The seat: keyboard (with repeat), pointer (with cursor shapes and click counting) and
//! text-input-v3.

use masonry_core::core::keyboard::KeyState;
use masonry_core::core::{
    CursorIcon, Modifiers, PointerButtonEvent, PointerEvent as MasonryPointer, PointerScrollEvent,
    PointerState, PointerUpdate, TextEvent,
};
use masonry_core::dpi::PhysicalPosition;
use masonry_core::kurbo::{Point, Vec2};
use masonry_core::ui_events::pointer::PointerButtons;
use smithay_client_toolkit::seat::keyboard::{KeyEvent, KeyboardHandler, Keysym, RawModifiers};
use smithay_client_toolkit::seat::pointer::{
    PointerEvent, PointerEventKind, PointerHandler, ThemeSpec, ThemedPointer,
};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use wayland_client::protocol::{wl_keyboard, wl_pointer, wl_seat, wl_surface};
use wayland_client::{Connection, QueueHandle};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ZwpTextInputV3;

use super::clipboard::{self, Selection};
use super::{State, TextInputData, input};
use crate::WindowId;

/// Input state of the one seat the driver uses.
#[derive(Default)]
pub(super) struct Input {
    pub(super) seat: Option<wl_seat::WlSeat>,
    pointer: Option<ThemedPointer>,
    /// Window under the pointer.
    pub(super) pointer_focus: Option<WindowId>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pub(super) text_input: Option<TextInput>,
    /// Window whose surface has text-input focus.
    pub(super) ti_focus: Option<WindowId>,
    pub(super) kb_focus: Option<WindowId>,
    /// Window whose surface lost keyboard focus; reported to Masonry after the event batch
    /// unless one of its surfaces gets focus back.
    kb_pending_leave: Option<WindowId>,
    modifiers: Modifiers,
    buttons: PointerButtons,
    clicks: input::ClickCounter,
    /// Latest input serial, for popup grabs and window moves.
    pub(super) last_serial: Option<u32>,
}

impl Input {
    /// Shows `cursor` while the pointer is over one of our surfaces.
    pub(super) fn set_cursor(&self, conn: &Connection, cursor: CursorIcon) {
        if let Some(pointer) = &self.pointer
            && let Err(err) = pointer.set_cursor(conn, cursor)
        {
            tracing::debug!(%err, ?cursor, "can't set the cursor");
        }
    }
}

/// text-input-v3 object plus the state between `done` events.
pub(super) struct TextInput {
    pub(super) ti: ZwpTextInputV3,
    pub(super) enabled: bool,
    pub(super) pending_preedit: Option<(String, Option<(usize, usize)>)>,
    pub(super) pending_commit: Option<String>,
}

impl State {
    /// Uses the first seat: creates its text-input object. Pointer and keyboard come from
    /// `new_capability`.
    pub(super) fn adopt_seat(&mut self, seat: wl_seat::WlSeat) {
        if self.input.seat.is_some() {
            return;
        }
        if let Some(mgr) = &self.g.text_input {
            self.input.text_input = Some(TextInput {
                ti: mgr.get_text_input(&seat, &self.qh, TextInputData),
                enabled: false,
                pending_preedit: None,
                pending_commit: None,
            });
        }
        self.clipboard.adopt_seat(&self.qh, &seat);
        self.input.seat = Some(seat);
    }

    /// Reports a deferred keyboard leave to Masonry.
    pub(super) fn flush_focus_leave(&mut self) {
        if let Some(id) = self.input.kb_pending_leave.take()
            && let Some(w) = self.reg.windows.get_mut(&id)
        {
            w.root
                .handle_text_event(TextEvent::WindowFocusChange(false));
            w.accesskit.set_focused(false);
        }
    }

    fn key(&mut self, event: &KeyEvent, state: KeyState, repeat: bool) {
        let Some(id) = self.input.kb_focus else {
            return;
        };
        let ev = input::key_event(event, state, self.input.modifiers, repeat);
        if clipboard::is_paste(&ev) {
            // Answered with the clipboard's text, as Masonry's text widgets expect.
            self.paste(id, Selection::Clipboard);
        } else if let Some(w) = self.reg.windows.get_mut(&id) {
            w.root.handle_text_event(TextEvent::Keyboard(ev));
        }
    }

    fn pointer_state(&self, id: WindowId, pos: Point, time: u32) -> PointerState {
        let scale = self.reg.windows.get(&id).map_or(1.0, |w| w.scale);
        PointerState {
            time: u64::from(time) * 1_000_000,
            position: PhysicalPosition::new(pos.x * scale, pos.y * scale),
            buttons: self.input.buttons,
            modifiers: self.input.modifiers,
            count: self.input.clicks.count(),
            scale_factor: scale,
            ..PointerState::default()
        }
    }

    fn pointer_button(
        &mut self,
        id: WindowId,
        pos: Point,
        time: u32,
        button: u32,
        down: bool,
    ) -> MasonryPointer {
        let button = input::button(button);
        if let Some(b) = button {
            if down {
                self.input.buttons.insert(b);
                self.input.clicks.press(b, pos, time);
            } else {
                self.input.buttons.remove(b);
            }
        }
        let event = PointerButtonEvent {
            button,
            pointer: input::MOUSE,
            state: self.pointer_state(id, pos, time),
        };
        if down {
            MasonryPointer::Down(event)
        } else {
            MasonryPointer::Up(event)
        }
    }
}

impl SeatHandler for State {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        self.adopt_seat(seat);
    }

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if self.input.seat.as_ref() != Some(&seat) {
            return;
        }
        match capability {
            Capability::Keyboard if self.input.keyboard.is_none() => {
                // sctk times the repeat from the compositor's rate and delay.
                let keyboard = self.seats.get_keyboard_with_repeat(
                    qh,
                    &seat,
                    None,
                    self.loop_handle.clone(),
                    Box::new(|state, _, event| state.key(&event, KeyState::Down, true)),
                );
                match keyboard {
                    Ok(k) => self.input.keyboard = Some(k),
                    Err(err) => tracing::error!(%err, "no keyboard"),
                }
            }
            Capability::Pointer if self.input.pointer.is_none() => {
                let surface = self.g.compositor.create_surface(qh);
                let pointer = self.seats.get_pointer_with_theme::<_, ()>(
                    qh,
                    &seat,
                    self.shm.wl_shm(),
                    surface,
                    ThemeSpec::default(),
                );
                match pointer {
                    Ok(p) => self.input.pointer = Some(p),
                    Err(err) => tracing::error!(%err, "no pointer"),
                }
            }
            _ => {}
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        match capability {
            Capability::Keyboard => {
                if let Some(k) = self.input.keyboard.take() {
                    k.release();
                }
                self.input.kb_focus = None;
            }
            Capability::Pointer => {
                if let Some(p) = self.input.pointer.take() {
                    p.pointer().release();
                }
                self.input.pointer_focus = None;
            }
            _ => {}
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        if self.input.seat.as_ref() == Some(&seat) {
            tracing::warn!("the seat went away");
            self.input.seat = None;
        }
    }
}

impl KeyboardHandler for State {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        serial: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
        self.input.last_serial = Some(serial);
        let Some((id, _)) = self.locate(surface) else {
            return;
        };
        self.input.kb_focus = Some(id);
        // Focus moving between a window and its own popups isn't a window focus change:
        // Masonry's menus close when their window loses focus.
        if self.input.kb_pending_leave == Some(id) {
            self.input.kb_pending_leave = None;
        } else {
            // Another window's leave is reported now: it isn't focused any more. (Taking it
            // out first would drop it, and a window the keyboard moved from, such as the
            // parent of a HUD window, would stay focused for good.)
            self.flush_focus_leave();
            if let Some(w) = self.reg.windows.get_mut(&id) {
                w.root.handle_text_event(TextEvent::WindowFocusChange(true));
                w.accesskit.set_focused(true);
            }
        }
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        // Deferred: a matching `enter` on the same window may follow in this batch.
        if let Some((id, _)) = self.locate(surface) {
            self.flush_focus_leave();
            self.input.kb_pending_leave = Some(id);
            if self.input.kb_focus == Some(id) {
                self.input.kb_focus = None;
            }
        }
    }

    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        serial: u32,
        event: KeyEvent,
    ) {
        self.input.last_serial = Some(serial);
        self.key(&event, KeyState::Down, false);
    }

    /// Compositor-side repeat (`wl_keyboard` v10).
    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.key(&event, KeyState::Down, true);
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.key(&event, KeyState::Up, false);
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: smithay_client_toolkit::seat::keyboard::Modifiers,
        _: RawModifiers,
        _: u32,
    ) {
        self.input.modifiers = input::modifiers(&modifiers);
    }
}

impl PointerHandler for State {
    fn pointer_frame(
        &mut self,
        conn: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            let Some((id, popup)) = self.locate(&event.surface) else {
                continue;
            };
            let Some(w) = self.reg.windows.get(&id) else {
                continue;
            };
            let offset = popup.map_or(Vec2::ZERO, |pi| w.popups[pi].origin.to_vec2());
            let (scale, cursor) = (w.scale, w.cursor);
            let pos = Point::new(event.position.0, event.position.1) + offset;
            let masonry = match event.kind {
                PointerEventKind::Enter { serial } => {
                    self.input.last_serial = Some(serial);
                    self.input.pointer_focus = Some(id);
                    // Cursors must be set again on every enter.
                    self.input.set_cursor(conn, cursor);
                    MasonryPointer::Enter(input::MOUSE)
                }
                PointerEventKind::Leave { .. } => {
                    if self.input.pointer_focus == Some(id) {
                        self.input.pointer_focus = None;
                    }
                    MasonryPointer::Leave(input::MOUSE)
                }
                PointerEventKind::Motion { time } => MasonryPointer::Move(PointerUpdate {
                    pointer: input::MOUSE,
                    current: self.pointer_state(id, pos, time),
                    coalesced: Vec::new(),
                    predicted: Vec::new(),
                }),
                PointerEventKind::Press {
                    time,
                    button,
                    serial,
                } => {
                    self.input.last_serial = Some(serial);
                    self.pointer_button(id, pos, time, button, true)
                }
                PointerEventKind::Release {
                    time,
                    button,
                    serial,
                } => {
                    self.input.last_serial = Some(serial);
                    self.pointer_button(id, pos, time, button, false)
                }
                PointerEventKind::Axis {
                    time,
                    horizontal,
                    vertical,
                    ..
                } => MasonryPointer::Scroll(PointerScrollEvent {
                    pointer: input::MOUSE,
                    delta: input::scroll_delta(&horizontal, &vertical, scale),
                    state: self.pointer_state(id, pos, time),
                }),
            };
            if let Some(w) = self.reg.windows.get_mut(&id) {
                w.root.handle_pointer_event(masonry);
            }
            if let PointerEventKind::Press {
                button: input::BTN_MIDDLE,
                ..
            } = event.kind
            {
                // After Masonry saw the press, which may have moved the caret.
                self.paste(id, Selection::Primary);
            }
        }
    }
}
