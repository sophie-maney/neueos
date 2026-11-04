# 0046: Pop-up button, pull-down button, combo box and the edit menu

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
Task 2.17b builds three controls on the menus of ADR 0045 and gives text fields and text
views their default context menu. The menu layer needs a few things it did not have: a
minimum width, a highlighted item on opening, a way for an owner to hear that its menus
closed, and opening from a deferred mutation. Text fields also have a context menu of their
own now, which must not fight the app's `ContextMenu` around them.

## Decision
1. **One widget for pop-up and pull-down buttons** (`widgets/popup.rs`, `PopUpWidget`),
   differing in kind. Both draw neutral gel with an accent cap on the right
   (`neue_paint::materials::popup_button`: up and down arrows for a pop-up, a down arrow for
   a pull-down; the cap darkens while the menu is open). The pop-up shows its chosen title
   (the widest title sets the width) and sends the chosen index (`PopUpButton::on_select`).
   The pull-down shows a fixed title and its `Menu<M>` sends the item messages.
2. **Placement.** A pop-up button's menu opens with the chosen item level with the button's
   title, the chosen item checked and highlighted, as wide as the button at least. A
   pull-down's menu opens right below it, nothing highlighted. Pressing the button opens
   the menu; press-drag-release chooses (the session logic of ADR 0045). Space, Return or
   Down open it from the keyboard; typing the start of a title chooses it while a pop-up is
   closed (the same prefix rule as in menus). Pressing a pull-down while its menu is up
   closes it: the menu sees the press first, as an outside one, and the button doesn't reopen
   while that menu is still its attached layer. The open look follows a flag the button
   keeps; it is reconciled with the layer on hover changes, because a compositor can take
   a popup away without telling its owner.
3. **Combo box = a text field with an arrow** (`widgets/field/combo.rs`). The field widget
   gets an optional `Combo` (menu, titles, open flag): an arrow button inside the well's
   right end (`combo_arrow`), the text area shortened to make room, and Down (or a press on
   the arrow) opens a menu of suggestions as wide as the field, below it. Choosing puts the
   title in the field and sends `ComboBox::on_select(index, title)`; typing sends the
   field's `on_change` as before. Focus returning to the field after a menu keeps the text as
   it was instead of selecting all, as Tab would.
4. **Menu layer changes** (`widgets/menu.rs`): `Anchor` (position, where the pointer went
   down, minimum width, initial highlight, `on_closed`, refocus target) replaces the bare
   point; `open` is generic over the owner's context (`Opener`: an event or a deferred
   mutation). `on_closed(owner, chosen)` runs on the owner after its menus are gone, to keep
   its own state (open flag, chosen item), and when given reports the choice itself:
   Masonry checks that an action's type is the widget's, so the combo box (a field) reports
   `FieldAction::Chosen` where context menus and pop-ups report `Chosen`.
5. **Edit menu** (`widgets/edit_menu.rs`): Cut, Copy and Select All (with their shortcuts),
   on text fields (not secure ones) and text views; Cut needs a selection in something
   editable, Copy a selection, Select All some text. Opened by a secondary press (the
   caret goes to the pointer if nothing is selected), the Menu key or Shift-F10, or
   AccessKit's `ShowContextMenu`. The widget is the owner and acts on itself when the menu
   closes (`on_closed` runs a mutation; changes send the usual change message).
6. **An app's `ContextMenu` around a field wins.** The field opens its menu after the event
   (a deferred mutation), and only if no other menu opened meanwhile: `menu::opened()`
   counts the menus a thread has opened, compared with its value at the event. The field
   leaves the secondary press and the Menu key unhandled so they bubble up to the wrapper,
   which opens its menu during the same event. (The counter is per thread: menus open on
   the UI thread only; a process-wide one made parallel tests interfere.)
7. **Shortcuts in goldens.** `Fixture::new` makes the command key Ctrl on its thread, so
   the edit menu's golden reads the same on macOS and Linux.

## Consequences
- 20 end-to-end tests (14 for the three controls, 6 for the edit menu) on the fake host,
  passing on macOS and Linux aarch64 (x86_64 not run);
  68 goldens (pop-up and pull-down: normal, open, open by key, focused, disabled; combo box:
  normal, focused, open, typed, filled, disabled; edit menu) × light/dark × 1×/2×. Gallery page
  "Pop-ups".
- **No Paste in the edit menu.** The drivers hand clipboard text to widgets only when the
  paste key is pressed (`TextEvent::ClipboardPaste`); a widget cannot ask for it. A menu
  Paste needs a request from the widget to the driver (a new Masonry signal, so a fork bump,
  and both drivers): its own task.
- Not yet: a combo box with its own text selection kept while the menu is up (the
  well's selection hides while focus is in the menu); disabled or separator items in a
  pop-up button; changing a pop-up's items after it is built (`Handle::set_items`);
  suggestion filtering as the user types; menus taller than the screen; menu animation.
- Wayland behaviour (placement of a popup above its parent, grabs) is unverified in the VM:
  `docs/checks/phase-2.md` §9.
