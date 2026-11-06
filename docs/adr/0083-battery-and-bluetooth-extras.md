# 0083: Battery, Bluetooth and network menu extras in the shell

- **Status:** Accepted
- **Date:** 2026-11-06

## Context
Task 3.11. The D-Bus clients (`neue_dbus::power`, `neue_dbus::bluetooth`, 3.11a) and the
bar's icon extras and menus (`neue_shell::bar`, 3.11b) exist; the shell process has to
connect them. No new dependency, protocol or interface: this records how the shell follows
the two system services.

## Decision
1. **One worker per service.** `neue-shell` starts a battery worker and a Bluetooth worker
   (`extras.rs`, Linux-only like `global_menu.rs`), each on a thread of its own with its own
   system-bus connection, so a slow BlueZ never holds up UPower. A worker reads its service,
   sends the UI thread a snapshot when it differs from the last one sent, then waits for a
   change signal (`PowerChanges`, `BluetoothChanges`) or five seconds, whichever comes
   first. Signals say nothing about a service that appears on or leaves the bus, so the
   poll covers that: with UPower or BlueZ stopped, or started later, the extra follows
   within five seconds.
2. **Absent means hidden.** A failed read (the service isn't on the bus) counts as "nothing
   to show", like no battery (`IsPresent` false) or no adapter: the snapshot is `None` and
   the extra is hidden (`Handle::set_hidden`), not removed from the bar. A worker that
   can't connect to the system bus at all ends, logs a warning and hides its extra.
3. **UI thread.** `BarContent::{battery, bluetooth}` are the truth for every bar;
   `sync_bars` applies them (icon, accessible name, menu, hidden) to each bar, as it does
   for the clock. The shell also keeps BlueZ's full `State`, so the menu's device indices
   (`BluetoothCommand::Connect(i)`) are looked up to object paths there.
4. **Actions.** Each BlueZ call (`set_powered`, `connect`, `disconnect`) runs through
   `cx.spawn` on a fresh connection, because `connect` can block for many seconds; the
   result is only logged. The new state is not applied optimistically: BlueZ's change
   signal brings it, so the menu never shows something BlueZ didn't do.
5. **Limits.** Only already paired devices are listed and no BlueZ agent runs, so pairing
   and PIN or passkey requests still need `bluetoothctl`. A failed connect shows nothing
   to the user beyond the log (a notification arrives with 3.15).

## Consequences
- The extras need no configuration; a desktop with no battery or adapter shows neither.
- Testing in the VM uses `neue-dbus`'s `mock_power_bluetooth` example on the system bus
  (`docs/dev-setup.md` §1.6b).
- The five-second poll costs one `GetAll` and one `GetManagedObjects` call per interval.

## Addendum: the network extra (task 3.12c)
`neue_dbus::network` (3.12a) and the bar's network extra (3.12b) are wired the same way:
- **Worker.** `extras::run_network` follows NetworkManager on a thread of its own.
  NetworkManager's signals come in bursts (every scan touches each access point), so the
  worker reads once the signals have been quiet for 300 ms, but never waits more than 2 s
  for a burst to end (`follow` takes the debounce; the other two workers pass none). The
  5 s poll covers NetworkManager appearing and leaving.
- **Absent means hidden.** NetworkManager off the bus, or on it with neither an Ethernet
  nor a Wi-Fi device, is `None` and hides the extra.
- **Actions** (`Shell::network_command`) look indices up in the last `State` and run each
  call through `cx.spawn` on a fresh connection: Wi-Fi switch, join, connect, disconnect,
  and a scan when the menu opens (skipped with Wi-Fi off; a scan NetworkManager refuses
  as too soon is logged at debug level). Results are only logged; the change signals bring
  the new state. A protected network that isn't saved can't be joined (the menu greys it
  out; `nmtui` until a secret agent exists).
