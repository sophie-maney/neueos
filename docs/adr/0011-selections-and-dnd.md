# 0011: Clipboard, primary selection, data-control and DnD icons (task 1.8)

- **Status:** Accepted
- **Date:** 2026-11-03

## Context
The core data device has existed since 1.1 (foot needs it). Task 1.8 adds the rest of
what desktop apps and clipboard tools expect: primary selection, a way for tools such as
`wl-copy`, `wl-paste` and clipboard managers to read and write selections without
keyboard focus, and drag icons.

## Decision
1. **Protocols.** `zwp_primary_selection_device_manager_v1`, `ext_data_control_manager_v1`
   and `zwlr_data_control_manager_v1`, all from Smithay's `wayland::selection`. The plan
   names ext-data-control. wlr-data-control is added too because `wl-clipboard` and most
   clipboard managers in the wild still use only that one; both share the same selection
   store, so they cost nothing but a global.
2. **Focus.** Clipboard and primary selection follow *keyboard* focus, in
   `SeatHandler::focus_changed` (`set_data_device_focus` and `set_primary_focus`). Data
   control ignores focus by design.
3. **Data-control is open to every client.** There is no permission model for Wayland
   clients yet: any client can read and replace the clipboard, as on other compositors
   that ship these protocols. That is the point of the protocol, but it also lets any
   Flatpak app read copied passwords. Flatpak sandboxes will need to filter these globals
   once the portal story (Phase 7) exists; the filter closure passed to `DataControlState::new` is where
   that goes. Until then this is accepted.
4. **Drag icons.** `State::dnd_icon` holds the icon surface and its offset from the
   pointer. The offset starts at zero and moves with the buffer offsets the client
   commits (`wl_surface.attach` x/y and `wl_surface.offset`). It is cleared when the drop
   happens or the drag is cancelled. Both backends draw it just under the cursor and send
   it frame callbacks, so animated icons work. Pointer drags only; touch DnD waits for
   touch input.
   **Escape cancels a drag.** Smithay doesn't end the grab when a client destroys its data
   source, so a client-side cancel (GTK on Esc) would leave the icon on screen until the
   button is released. While a drag runs (`State::dnd_active`), the compositor takes Escape
   itself: the pointer grab is unset without a drop, the source gets `cancelled`, the icon
   goes, and clients never see the key.
5. **No clipboard persistence.** When the owning client exits the selection is gone. A
   clipboard manager built on data-control (a later shell task) can keep it alive.

## Consequences
- `neue-shell` can build a clipboard history on data-control without compositor changes.
- Sandboxed apps must not get the data-control globals when Flatpak support lands.
- XWayland selection bridging is `xwayland-satellite`'s job, not ours.
