# 0045: Menus: the menu model, menu layers and context menus

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
Task 2.17a adds menus to neue-kit: a model that the menu bar's DBusMenu export (2.18) and
pop-up buttons (2.17b) can share, and context menus with submenus, keyboard navigation
and type-select. Masonry has layers (ADR 0002, ADR 0034: xdg-popups on Wayland, in-window
overlays under winit) but no menu widget of the kind we need: its `Selector` menu is a flat
list without keyboard handling. A layer can't take keyboard focus when it is created (it
is added by the driver after the event that opened it), its pointer events reach every
layer through `Layer::capture_pointer_event`, and only an owner's attached layers are
cleaned up with it.

## Decision
1. **Model: `Menu<M>` and `MenuItem<M>`** (`neue_kit::menu`). Items, separators and
   submenus; per item a label, an optional `Shortcut`, enabled, and a `MenuToggle`
   (`Check(CheckState)` or `Radio(bool)`), plus the message it sends (`on_choose`,
   `on_choose_with`). Read accessors (`items`, `label`, `child_menu`, `shortcut_keys`,
   `is_enabled`, `toggle`) are public for the DBusMenu exporter. When a menu goes into a
   widget it splits into a `MenuSpec` (plain data the widgets show, cheap to clone) and a
   handler table. Every item in the tree gets an id: its index in a depth-first walk
   (separators and submenus included), which can serve as the DBusMenu item id.
2. **Shortcuts are labels.** `Shortcut::command(key)` is Cmd on macOS hosts and Ctrl
   elsewhere (as ADR 0042); `.shift()`, `.alt()`, `.ctrl()`. Shown as glyphs in Mac order
   (⌃⌥⇧⌘, then the key; DejaVu Sans has them), read to assistive technologies as
   `Ctrl+Shift+S`. They don't bind keys; key equivalents arrive with the menu bar (Phase 3).
   If Super becomes ⌘ (PLAN.md Q1), only `Shortcut::modifiers` changes.
3. **One layer per open menu** (`widgets/menu.rs`): `MenuWidget` (the panel, AccessKit
   `Menu`) with one `MenuRowWidget` child per item (`MenuItem`, `MenuItemCheckBox`,
   `MenuItemRadio`; separators are hidden from AT). Submenus are layers attached to the
   menu they open from (`create_attached_layer`), so a new submenu replaces the old one and
   closing a menu closes its submenus. Submenus open at the parent's right edge, overlapping
   its rim by `SUBMENU_OVERLAP`, with their first item level with the parent item.
4. **Focus.** The menu takes keyboard focus when its rows report `Update::WidgetAdded`
   (an action they send bubbles to the menu, whose `ActionCtx` can request focus); the
   highlighted row is the focused widget, so screen readers follow the highlight. The
   owner records what had focus before (kept if it reopens while open), and closing gives
   focus back to it.
5. **Pointer, per session.** The menus of one session share a small state: whether the
   current press is inside any of them (each checks itself in layer order; the deepest
   decides "outside" and closes them all), and whether releasing the button chooses
   ("armed": after a press inside a menu, after dragging `DRAG_THRESHOLD` from where the
   opening press was, or always when opened from the keyboard). So press-drag-release and
   click-release-click both work, and a right-click released in place leaves the menu up.
   Hovering an item highlights it and opens its submenu at once (no delay); leaving the
   menu keeps the item whose submenu is open highlighted.
6. **Keyboard.** Up, Down, Home and End move over enabled items without wrapping; Right
   or Return opens a submenu with its first item highlighted, Left closes it; Return or
   Space chooses; Escape closes every menu; letters type-select (case-insensitive prefix
   over enabled items, reset after `TYPE_SELECT_RESET`). All keys stop at the menu, so Tab
   and the default button don't act while it is open.
7. **Choosing.** The menu asks its owner (by `mutate_later`) to remove the root menu and
   submit `Chosen(id)`, the owner's action type; the owner's element maps the id to the
   message. An owner that already replaced the menu doesn't remove it twice.
8. **Context menus: `ContextMenu<M>`** wraps a child. A secondary press inside it opens
   the menu at the pointer; the Menu key or Shift-F10 (keys bubbling from focus inside it)
   and AccessKit's `ShowContextMenu` open it at its middle with the first item
   highlighted. `Handle::set_menu` replaces the menu.
9. **Look** (`neue_theme::materials::menu`, `neue_paint::materials::menu`): a near-white
   (dark: charcoal) panel, `radii::MENU` corners, a hairline rim and a soft drop shadow
   drawn in the paint insets; 19 px items with a check column, right-aligned shortcuts and
   a triangle for submenus; the highlight is an accent gradient band (graphite with the
   graphite accent) with white text. Opaque for now; the translucent tint waits for the
   backdrop blur (Phase 3).
10. **Drivers tolerate removing a layer twice** (Wayland and winit): a dismissed popup's
    nested popups are removed by the driver and again through Masonry's attached-layer
    cleanup.

## Consequences
- 15 end-to-end tests on the fake host and 12 goldens (open, highlighted, submenu × light/
  dark × 1×/2×), passing on macOS and Linux aarch64. The golden specimen uses Ctrl
  shortcuts so it renders the same on every host.
- Not yet: on Wayland a submenu anchors to a point, so near the right screen edge it flips
  over its parent instead of to its left (needs a row anchor rect in `popup.rs`); in-window
  overlays (macOS) aren't kept inside the window; a compositor dismissal (outside click on
  Wayland) drops focus instead of returning it; no submenu open delay or diagonal-motion
  grace; no scrolling for menus taller than the screen; Ctrl-click doesn't open context
  menus on macOS hosts; no menu open/close animation.
- Wayland behaviour (nested popups, grabs, keyboard focus in a popup) is unverified in
  the VM: `docs/checks/phase-2.md` §8.

## Addendum: slider items (task 3.14c)
The volume extra needs a slider in a menu, which sets a value without closing it.
- **Model.** `MenuItem::slider(label, value)` with `on_change(|v| …)`: a value from 0 to
  1, the label read by assistive technologies only. `Menu::into_parts` keeps its shape
  (a slider has no choose handler); `into_parts_with_sliders` also returns the slider
  handlers by item id, for owners that pass changes on.
- **Delivery.** `Anchor::on_change` (like `on_closed`) is called on the owner with the
  item id and value while the menus stay up. Only menu-bar titles set it
  (`TitleAction::Changed`); in context menus and pop-up buttons the knob moves but nothing
  is sent, and the DBusMenu export leaves slider items out (DBusMenu has no slider).
  `Handle::set_menu` swaps the slider handlers too; an open menu keeps the value it opened
  with, so a change from elsewhere (the volume keys) shows the next time it opens.
- **Input.** A press on the row moves the knob there and drags it until the release
  (which chooses nothing); Left and Right step it by `SLIDER_ITEM_STEP` (1/16, the OSD's
  steps) while it is highlighted, instead of leaving for the bar's neighbouring menu;
  Return and Space do nothing. It draws the regular slider across the row, with no
  highlight bar; the knob shows a focus ring when the keyboard highlighted it.
- **Accessibility.** Role `Slider` with numeric value, range 0..1 and step; `Increment`,
  `Decrement` and `SetValue`.
