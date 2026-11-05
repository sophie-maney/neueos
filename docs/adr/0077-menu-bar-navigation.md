# 0077: Moving between the menu bar's menus, and Ctrl+F2 through `neue_shell_v1` v2

- **Status:** Accepted
- **Date:** 2026-11-05

## Context
Task 3.7b. With 3.7a the bar shows the active app's menus, but each title only opens its
own menu: pointing at a neighbour while a menu is up does nothing, Left and Right stay
inside one menu, and the keyboard can't reach the bar at all (it is a layer surface with
`keyboard_interactivity: none`, so it never takes focus from the app it names). Getting
it working in the VM also showed two things in the way: the compositor never grabbed for
popups of layer surfaces (only of windows), so bar menus never got the keyboard; and
since task 3.5 kit apps on a compositor with blur got no keyboard or pointer at all.

## Decision
1. **The bar coordinates switching** (`MenuBarWidget`, neue-kit). It follows which title
   on the left has its menu up from the titles' `Opened`/`Closed` actions as they bubble
   past (without handling them, so apps still get their messages). Switching takes the
   old menu down directly (no focus hand-back) and opens the new one in the same
   mutation, passing on what focus should go back to (`Anchor::return_focus`). Menu
   extras on the right take no part.
   - **Pointer:** a move over another left title while a menu is up opens its menu, not
     armed (a click or a drag chooses, as for a menu opened by a press).
   - **Keyboard:** menus opened by a bar title carry `Anchor::on_step`. Left in the top
     menu and Right on an item with no submenu send `TitleAction::Step`, which the bar
     handles: the next or previous title that would open something, wrapping, first
     item highlighted. Left in a submenu still closes it. Between focused titles (no
     menu up) Left and Right move focus; Escape resigns it.
   - **Opening from the keyboard:** `Handle<MenuBar>::open_first_menu` opens the first
     title with items, unless a menu is up.
2. **`neue_shell_manager_v1` version 2** adds the event `focus_menu_bar(serial, output)`.
   The compositor action `focus-menu-bar` sends it to every manager of version 2 or
   later; `[keybindings]` binds it to Ctrl+F2 by default. `output` is the active
   window's output, else the pointer's (null if the shell has no `wl_output` for it).
   The serial comes from the compositor's counter, for the menu's `xdg_popup.grab`; the
   driver stores it as the last serial and reports `ShellEvent::FocusMenuBar`, and the
   shell calls `open_first_menu` on that output's bar right away. The action fails
   (`Internal`, with a message) without such a shell or while the session is locked.
3. **Popup grabs for layer surfaces** (neue-comp). `xdg_popup.grab` now accepts a root
   that is a layer surface, not only a window, so shell menus get the keyboard like app
   menus. When the last menu of a layer surface that takes no keyboard focus closes,
   Smithay would give the keyboard to that surface (the bar), which then swallows the
   next key. `popup_destroyed` schedules `release_layer_grab` on idle (once the client has
   destroyed it): if the grab's root is such a layer and none of its popups are alive, it
   ends the keyboard and pointer grabs and focuses the active window (else the topmost).
   The active window stays active for the shell while its menu is up (as before).
   Replacing a menu during a switch needs nothing more: Smithay accepts a new grab of the
   same root through the grab's previous serial (covered by a test).
4. **Seat adoption before the first roundtrip** (neue-platform). The blur roundtrip at
   start-up (task 3.5) ran before the seat was adopted, so the seat's `capabilities`
   arrived first and were ignored: no keyboard, no pointer for any kit app on a
   compositor with blur. Seats are now adopted before any roundtrip.

## Alternatives considered
- **Switching in each title** (a title noticing the pointer and asking a sibling to
  close): titles don't know their siblings; the bar already lays them out.
- **Giving the bar `on_demand` interactivity and focusing it for Ctrl+F2:** clicking the
  bar would then take focus from the app it names, and the bar would need its own focus
  handling. A grabbing popup already gets the keyboard and gives it back.
- **No serial in the event:** Smithay only checks the serial while another grab is held,
  but `xdg_popup.grab` must name an input serial; a serial the compositor issued keeps
  the shell protocol-correct.
- **A new IPC error code for "no shell":** an IPC change for one message; `Internal`
  with a message is enough until a second case appears.

## Consequences
- In the VM, Ctrl+F2 then the arrows walk the gallery's menus, pointing switches menus,
  and Escape or choosing gives the keyboard back to the app.
- While a bar menu has the keyboard, the app's window draws inactive (it got
  `wl_keyboard.leave`); on the reference system the app stays key. Left for a follow-up:
  the compositor would have to keep the window activated while a shell menu has the
  keyboard.
- The bold app-name title opens nothing until the fallback App menu (3.9), so
  navigation skips it.
- A shell bound at version 1 never gets `focus_menu_bar`; older shells keep working.
