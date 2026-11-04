# `gtk_shell1`

GTK's private Wayland extension. We implement the part GTK apps use to announce their
`org.gtk.Menus` and `org.gtk.Actions` addresses (ADR 0005, ADR 0025).

- **XML:** `crates/neue-protocols/protocols/gtk-shell.xml`, vendored unchanged from
  <https://gitlab.gnome.org/GNOME/gtk/-/raw/main/gdk/wayland/protocol/gtk-shell.xml>
  (fetched 2026-11-04, version 7 in the file). Licence: LGPL-2.1-or-later.
- **Offered:** `gtk_shell1` version 5.
- **Implemented:** `gtk_surface1.set_dbus_properties` (stored per toplevel, shown by
  `neuectl windows` as `menu`), and `gtk_shell1.capabilities` (always 0 for now).
- **Accepted and ignored:** every other request.
- **Bindings:** `neue_protocols::gtk_shell::{server, client}`, behind the `server` and
  `client` features.

The KDE half, `org_kde_kwin_appmenu_manager` v2, comes from `wayland-protocols-plasma`.

## Updating
Fetch the file again, diff it against ours (ignoring our header comment), and read the new
requests. A new version needs no code unless we want to offer it.
