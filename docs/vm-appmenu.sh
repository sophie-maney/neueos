# Run on the VM by docs/dev-setup.md §18 (task 1.17). Starts a throwaway session bus with
# a bare `com.canonical.AppMenu.Registrar` (it only owns the name, which is all Qt checks
# for; the real registrar is the shell's, Phase 3), then Kate and GNOME Text Editor on it,
# and prints the window list with their menu addresses.
#   ssh neueos-vm '. ~/neueos/docs/vm-env.sh; sh ~/neueos/docs/vm-appmenu.sh'
set -u
exec dbus-run-session -- sh -c '
python3 -c "
from gi.repository import Gio, GLib
Gio.bus_own_name(Gio.BusType.SESSION, \"com.canonical.AppMenu.Registrar\", Gio.BusNameOwnerFlags.NONE, None, None, None)
GLib.MainLoop().run()
" &
sleep 1
kate >/tmp/kate.log 2>&1 &
gnome-text-editor >/tmp/gte.log 2>&1 &
sleep 8
neuectl windows
busctl --user list | grep -i -E "kate|texteditor|dbusmenu"
pkill -x kate; pkill -x gnome-text-edit; kill %1
'
