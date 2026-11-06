# 0085: The tray area in the menu bar: placement, overflow and image extras

- **Status:** Accepted
- **Date:** 2026-11-06

## Context
Task 3.13b, the bar's side of the status-icon tray (ADR 0084 is the D-Bus side, 3.13c the
shell wiring). The bar needs a group of icons that apps add and remove at any time, an icon
kind for pictures it doesn't draw itself, and a rule for when there isn't room. No new
dependency: icons are `neue_paint::Icon`s, which already carry raster pictures (3.3).

## Decision
1. **Placement.** The tray is the group of extras furthest left of the right edge: right to
   left, clock, battery, network, Bluetooth, tray. Inside it the first item is rightmost,
   like the other extras, so items an app registers later appear further left and the
   older ones don't move.
2. **A dynamic group.** `MenuBar::with_tray` adds icons at build time and
   `Handle<MenuBar>::set_tray` replaces them all, as `set_menus` does for the app's menus:
   no rebuild of the bar, and the same caveat (don't replace an icon while its menu is up).
   Titles bound to handles change in place (`set_icon`, `set_hidden`, `set_description`),
   which is what the shell does for a changed icon, tooltip or status; `set_tray` is for
   items that come and go.
3. **`ExtraIcon::Image(Icon)`.** One fixed size (`TRAY_ICON_SIZE`, 16 px) whatever the
   picture's, fitted and centred by `neue_paint::icon`; `ExtraIcon::pixmap` builds one from
   straight-alpha RGBA, which is what `neue_dbus` gives for pixmaps. A named icon is
   resolved by the shell through `neue-desktop`'s icon theme into a picture (3.13c), so the
   kit never looks names up and stays free of `neue-desktop`. `ExtraIcon` stops being
   `Copy`.
4. **Light and dark treatment.** A picture keeps its own colours (apps draw for either
   appearance), softened to `TRAY_DARK_OPACITY` on the dark bar so bright glyphs don't
   glare; the light bar draws it as is. A glyph (the placeholder) follows the label colour
   with the embossed shadow, as the other extras; over an open menu's highlight a picture
   is drawn as is and a glyph in the highlighted label colour.
5. **Tooltip and name.** A title has an optional accessible *description* (`MenuTitle::
   description`, `Handle::set_description`) read after its name; the tray gives the item's
   title as the name and its tooltip as the description.
6. **Status.** `neue_shell::bar::TrayItemView` carries `TrayStatus`: `Passive` items are
   hidden (taking no room, as a hidden extra), `NeedsAttention` shows the attention icon
   when there is one, and an item whose icon couldn't be found shows a placeholder glyph
   rather than nothing. `tray_titles` makes the titles and one handle each from a list of
   views; `menu_bar` uses it for `BarContent::tray`.
7. **Overflow.** The tray gives way. The bar's width asked for doesn't count it, and the
   tray fills the room between the last title on the left and the extras: icons are placed
   in order while each fits with `TRAY_OVERFLOW_GAP` (24 px) left before the titles; the
   first one that doesn't, and all after it, are stashed (not painted, not in the
   accessibility tree, laid out again whenever room is made). So the older items are kept
   and the newest are dropped. Passive items count as zero width. There is no "more" menu
   in this task: a notch for the left-out ones would be a follow-up if a real bar runs
   out of room (a 1920 px bar holds dozens of icons).
8. **Not here.** A press without a menu (3.13c needs the primary press to send
   `Activate` and the secondary one to open the menu: a kit addition, listed in the task)
   and the keyboard: like the other extras, the tray takes no part in Left and Right
   between menus.

## Consequences
- 3.13c has to add the press messages to the kit's titles before the shell wiring; the
  rest of its work is mapping `HostEvent`s to `TrayItemView`s.
- `ExtraIcon` cloning is cheap (a picture is shared), but it is no longer `Copy`.
- `MenuBar::min_width` exists so a bar outside a layer surface (gallery, goldens) can
  be wider than its titles; the shell's surface sets the width itself.
