# 0082: The fallback menu and menu search

- **Status:** Accepted
- **Date:** 2026-11-06

## Context
Task 3.9, ADR 0005 §4. Apps that export no menu (libadwaita, Electron, Firefox, foot, X11
apps) get a menu bar of the shell's own, and Help's search finds items in whatever menus
the bar shows. `neue_shell_v1` already has what the Edit and Window items need
(`send_shortcut`, `run_action`, the toplevel's `activate`/`minimize`/`close`), so no
protocol change.

## Decision
1. **When.** The active window gets the fallback menu if it exports no address, or its
   menus were read and have no items (a GTK app with no exported menu; the address alone
   doesn't say). While an address's menus are still being read the bar shows nothing
   rather than flashing the fallback. No active window: no menus.
2. **What.** `neue_shell::fallback` describes the menus once, as data (`Section`s of
   `Row`s). The bar's titles, the entries menu search finds and the requests the
   compositor gets all come from it:
   - **App** (under the app's name): About (disabled: there is nothing generic to show),
     Hide (minimises the app's windows), Quit (asks each of the app's windows to close).
   - **File**: New Window (the desktop entry's `new-window` action, launched by the shell
     with `neue-desktop`; disabled without one or without an entry), Close Window.
   - **Edit**: Undo, Cut, Copy, Paste, Select All, sent as Ctrl chords with
     `send_shortcut` (the app gets Control in either case; the remap only rewrites Super).
     Their shortcuts show as ⌘ where Super reaches the app as Control (ADR 0081), else
     as the chord. **Terminals** (a desktop entry with the `TerminalEmulator` category)
     get Ctrl+Shift+C and Ctrl+Shift+V for Copy and Paste, and the other items are
     disabled, since Ctrl+C there interrupts. `[super-as-command]` can't say this to the
     shell (ADR 0081 §"alternatives"), and the entry's category is what the app is.
   - **Window**: Minimise, Zoom (`zoom-window`), Bring All to Front, then the app's
     windows (same app id) with the active one checked. "The app's windows" are the
     windows sharing the active window's app id, or the window alone without one.
3. **Help is the shell's.** Help ▸ "Search Menus…" opens a popover under the Help title
   with a search field and up to 8 hits as buttons, `Edit ▸ Paste  ⌃⇧V`-style, ranked by
   how the item's label matches (starts with the word, a word starts with it, contains
   it, only the menu above does) and then by menu order; every word of the query must
   match. Return runs the first hit. An app that exports a Help menu of its own gets the
   item at the top of it (we can't put a field inside another app's menu), so the bar
   never has two Helps. What is searched: the enabled items of the menus shown, as of
   opening the panel. Submenus the app hasn't populated (DBusMenu fills them on
   `AboutToShow`) are not searched.
4. **The kit.** `Popover::focus(&handle)` puts keyboard focus on a widget of the
   content when the popover opens (the search field), where it used to stay on the
   popover until Tab.
5. **The compositor.** `send_shortcut` ends a keyboard grab before focusing the window.
   The driver destroys a menu's popup after the update that sent the request, so the
   shell's popup grab still has the keyboard when the chord arrives, and the chord went
   to the popup. (ADR 0077 said "close the menu first", which the shell can't make
   happen before the request.) A headless test covers it.
6. **The bars' menus while the panel is open** wait as they do while a menu is up (the
   panel hangs off a title that rebuilding would replace) and are rebuilt when it closes.

## Consequences
- libadwaita, Electron, Firefox, foot and X11 apps get working Edit and Window menus, and
  a way to find any menu item.
- The Edit chords are what an app binds by default; an app that rebinds them is wrong
  in the menu and nowhere else.
- Multi-output: the panel opens under the Help of the first bar.

## Alternatives considered
- **A search field inside the menu.** The kit's menus are lists of rows; a field in one
  needs keyboard and focus handling that menus don't have. The popover has them.
- **Asking the compositor whether the app is a terminal.** Needs a protocol change
  for a fact the desktop entry already has.
- **Fuzzy (subsequence) matching.** Substring words are predictable and enough for menus.
