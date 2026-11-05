# 0078: The global menu's GMenuModel side: `org.gtk.Menus` and `org.gtk.Actions`

- **Status:** Accepted
- **Date:** 2026-11-05

## Context
Task 3.8. GTK 3 and 4 apps (`GtkApplication`) export their menu bar (and, in GTK 3, an app
menu) as a `GMenuModel` over `org.gtk.Menus`, and what the items do over
`org.gtk.Actions`. The compositor already passes the addresses to the shell (ADR 0025,
0076); the shell followed only DBusMenu ones, and `gtk_shell1.capabilities` was 0 so GTK
kept its in-window menu bar.

## Decision
1. **`neue_dbus::gmenu_client::GMenuClient`** follows one window's `GMenuAddress`
   (`bus_name` and up to four paths). `fetch()` reads everything into the same
   `MenuNode` trees the DBusMenu side gives, so the bar's rendering is shared:
   `titles` (the menu bar), `app_menu` (the items of the GTK 3 app menu) and
   `activations` (item id to action). Ids are sequential per fetch.
2. **Menu model.** `Start([0])`, then `Start` for every other group the items link to
   (GTK's exporter puts each submenu in a group of its own), up to 8 rounds of 64
   groups, 12 levels deep and 5000 items. `:section` links are flattened with a separator
   between non-empty sections; `:submenu` links become submenus. Mnemonic underscores are
   removed from labels; an `accel` attribute (`<Primary>q`) becomes the shortcut when the
   app exports one.
3. **Actions.** `DescribeAll` at the `application_path` (`app.*`) and `window_path`
   (`win.*`). An item is disabled if its action is missing or disabled, and
   `hidden-when` (`action-disabled`, `action-missing`) hides it. A boolean state shows
   as a check mark; an item with a `target` on a stateful action is a radio item, on
   when the state equals the target. Choosing an item calls `Activate(name,
   [target], {})`; parameterless actions get no parameter. Other prefixes (`unity.`…)
   have no actions and their items are disabled.
4. **Updates: re-read everything.** `GMenuSignals` is one match rule on the member
   `Changed` (a rule can't name two interfaces), filtered to the two interfaces in
   code; a signal from the followed window's paths means "fetch again", as with DBusMenu
   (ADR 0076 §3). The exporter only sends `Changed` for groups a client has started, so
   the client stays subscribed: a fetch starts the groups again and only then ends the
   previous subscription, so the count never reaches zero; `Drop` ends it.
5. **Shell.** `Address` is now `DbusMenu | GMenu`; the worker's source is one or the
   other, a second thread forwards `GMenuSignals`, and a click on a GMenu item calls
   `activate` (GMenuModel has no `AboutToShow`/`opened`/`closed`, so those events are
   ignored and trees are held back only while a *title's* menu is open as before).
   **The app menu is the menu under the app's name** (`Handle<MenuTitle>::set_menu` on
   the bar's fixed bold title, which was empty until now). `set_menu` takes effect the
   next time it opens, so it needs no hold-back. Apps with no app menu (GTK 4, Qt) leave
   it empty, as before.
6. **`gtk_shell1.capabilities` is 3** (`global_app_menu | global_menu_bar`), amending
   ADR 0025 §7. GTK 3 tests bit `value - 1` of the field, so the XML's values 1 and 2 are
   bits 0 and 1 and `0b11` is both; GTK 4 behaves the same here (checked in the VM: both
   `gtk3-demo-application` and `gtk4-demo-application` hide their in-window menu bars).
   `desktop_icons` (3) would be 4; we don't claim it.

## Consequences
- Tests: `neue-dbus/tests/gmenu.rs` runs the client against a stand-in exporter over a
  peer-to-peer connection (sections, groups of submenus, accelerators, check and radio
  state, `hidden-when`, activation with and without a target, the subscription handoff,
  a dead app). Shell tests cover the address conversion. The compositor test expects 3.
- VM: `gtk3-demo-application` shows the app menu under its name and Preferences and Help
  in the bar; `gtk4-demo-application` shows File, Edit, Preferences and Help; choosing
  Help ▸ About and Preferences ▸ Prefer Dark Theme act (the app reports the action). A
  GTK app with no exported menus (`gtk3-demo`'s main window) keeps working and the bar
  shows no titles for it.
- **GTK apps lose their menus when no shell is running** (they hide their own). The
  session always starts the shell (ADR 0066); a bare `neue-comp` without `--shell` leaves
  GTK apps with no menu. Accepted.
- Not done: icons (`icon`, `verb-icon`), section labels (headers), `target`-dependent
  `hidden-when: macos-menubar`, `SetState`, and menu items that open dialogs for
  parameters. Submenu items are read eagerly, not on first open.
