# 0094: Dock editing and menus: a drag inside the surface, the app owning the order, tiles sliding from where they were shown, menus asked of the app and placed like popovers

- **Status:** Accepted
- **Date:** 2026-11-06

## Context
Task 3.17b: drag a dock tile to rearrange it, drag it off to remove it, Keep in Dock
(written to `[dock] pinned`), and a context menu with the app's windows, Keep in Dock and
Quit. The dock is one layer surface (ADRs 0092, 0093) on a keyboard-less layer; its tiles
come from the shell's model (`neue_shell::dock`), which owns the pinned list. The menu's
items depend on the app's windows, which only the shell knows. `neue-settingsd` is the
only writer of the TOML files (ADR 0004, 0074).

## Decision
1. **The drag stays inside the dock's surface.** A primary press on a tile that moves past
   `dock::DRAG_THRESHOLD` (4 px) becomes a drag; a shorter one is still a click. The
   pointer is captured, and the compositor's implicit grab keeps sending motion while the
   button is down, also beyond the surface. The dragged icon (at its size at rest, held
   where it was taken) is drawn under the pointer, kept inside the surface. The surface
   keeps at least `dock::DRAG_ROOM` (one icon, scaled with it) beyond the icons, so an
   icon lifted off the shelf stays visible; the room is pass-through like the bounce room.
2. **Where it lands.** Along the dock, the slot is the tile stretch under the dragged
   icon's middle (clamped to the ends); the others open a gap there. More than
   `dock::REMOVE_DISTANCE` (half an icon) beyond the band it is off the dock: the others
   close up around the surface's middle and the icon fades (`REMOVING_OPACITY`).
3. **The app owns the order.** On a drop the widget emits `Moved(key, to)` or
   `Removed(key)` (`Dock::on_move`, `on_remove`) and goes back to the tiles it was given;
   the app answers with new tiles (`set_tiles`), or doesn't (a refusal).
4. **Tiles slide from where they were shown.** Each tile keeps a sprung offset along and
   away from the dock (`dock::SLIDE` = `Spring::STANDARD`). Whenever the arrangement
   changes (the gap moving, a drop, `set_tiles`), every tile starts from where it was drawn,
   measured from the dock's middle (which stays put on the screen while the centred
   surface grows or shrinks), and slides to its new place. So a drop settles from under
   the pointer, a refusal slides back, and tiles coming and going move their neighbours
   smoothly. Reduced motion jumps. Minimise targets (`on_placed`) are the places, not the
   sliding positions.
5. **The model.** `Dock::move_tile` keeps the app where it was dropped (a running app
   dragged into the dock is kept; kept apps stay in front of the others, so a drop after a
   running one lands after the last kept one); `unpin` (dragged off, or Keep in Dock
   unchecked) lets it go, its tile staying while it has windows; `pin` adds it after the
   kept apps. Only apps with a desktop entry can be kept. Entries of `pinned` that aren't
   installed stay after the kept app before them, and kept entries keep their spelling;
   new ones are written as the tile key (the desktop file ID without `.desktop`).
6. **Saving.** Each change is saved on a thread through `neue-settingsd`
   (`Settings::set_strings("shell.dock.pinned", …)`, new in `neue-dbus`). The model is
   the truth while the shell runs; a failed save is logged.
7. **Menus are asked of the app.** A secondary click, the Menu key or Shift-F10 on a
   focused tile, or AccessKit's `ShowContextMenu` makes the widget remember the tile and
   its icon, and emit `Menu(key)` (`Dock::on_menu`). The app builds a `Menu` and calls
   `Handle::<Dock>::show_menu(cx, key, menu)`, which opens it if that tile asked. The
   tile's focus node owns the menu (its action type is now `Chosen`), so the item handlers
   are registered for it as for a `ContextMenu`.
8. **Placement like a popover.** `menu::Anchor` gains `beside: Option<(Rect, Side)>`; set,
   the menu root carries a `PopoverAnchor` and the platform places it flush against the
   icon grown by `dock::MENU_GAP`, centred on that side (above at the bottom, beside it on
   the left and right) and flipped if there is no room. The menu's deeper shadow at the
   bottom adds a few pixels to the gap.
9. **The shell's menu:** the app's windows (titles, the active one checked; choosing one
   activates it), Keep in Dock (checked when kept, disabled without a desktop entry), and
   Quit (asks every window of the app to close, `ShellRequest::Close`) while it runs, or
   Open while it doesn't.

## Alternatives considered
- **A Wayland drag with an icon surface** (`wl_data_device.start_drag`, the compositor
  already draws drag icons): the icon could follow the pointer anywhere, but it needs a
  drag source and an icon surface in `neue-platform` and the kit, a cross-crate interface
  for one widget. Kept for later (below).
- **Growing the surface only while dragging:** no permanent room, but a configure round
  trip at the start and end of each drag, with the pointer's coordinates shifting under
  it for a frame.
- **The widget reordering its own tiles:** the shell would have to mirror it and could
  disagree (an app that can't be kept); with the app owning the order, a refusal is just
  a slide back.
- **Menus pushed to the widget ahead of time** (one per tile, refreshed on every window
  title change): more work on every update for a menu that is rarely opened.
- **Placing the menu at a point above the icon:** a short menu fits below the point and
  covers the icon; the popover placement prefers a side.

## Consequences
- The bottom dock's surface is 24 px deeper at 48 px icons (unless magnification already
  needs more room), and side docks 24 px wider; all pass-through. Every dock golden grew.
- `DockTileWidget` emits `Chosen`; `menu::Anchor` has a new field; `testing::Fixture`
  gains `press_at_px`.
- Quit closes windows one by one; an app that asks about unsaved work does so per window.

## Not done
- The icon can't follow the pointer beyond the dock's surface: dragged far off, it waits
  at the surface's edge, faded. A Wayland drag icon would fix that.
- No "poof" when an icon is dropped off the dock; no press-and-hold to open the menu.
- Reordering from the keyboard or an assistive technology (the dock gets no keyboard
  focus yet, ADR 0092); the menu works from both.
- Show All Windows, Hide, Open at Login.
