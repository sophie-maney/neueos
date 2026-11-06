# 0089: The notifications server: `neue_dbus::notifications`, ids on the server, clocks in a portable store

- **Status:** Accepted
- **Date:** 2026-11-06

## Context
Task 3.15a, the first part of notifications (split from 3.15 into the server, the banner
and alert widgets, activation tokens for the shell, and the shell wiring). Apps send
notifications through `org.freedesktop.Notifications` (Desktop Notifications spec 1.2,
with the `ActivationToken` signal). The shell will own the name, as it owns the appmenu
registrar and the status-notifier watcher (ADR 0084). `neue-dbus` is Linux-only, but the
logic of how long a banner stays up should be tested on macOS like the rest of the shell's
portable modules (`neue_shell::levels`).

## Decision
1. **`neue_dbus::notifications::Notifications`** serves the interface at
   `/org/freedesktop/Notifications` (`serve` for peer-to-peer tests, `own` to also take the
   name with replace and allow-replacement, as the watcher does). Each request goes to a
   callback on zbus's executor thread: `Request::Notify(Notification)` or
   `Request::Close(id)`. The callback must not block; the shell forwards to its loop.
2. **The server owns ids.** `Notify` must return an id synchronously, so the server hands
   them out (counting up, wrapping past 0, skipping open ids) and keeps the set of open
   ids. A `replaces_id` that is open is replaced (`Notification::replaces`); one that is no
   longer open comes back as itself for a new notification (the spec returns
   `replaces_id` whenever it is non-zero). At most 1024 open: `Notify` then fails with
   `LimitsExceeded` (replacements still work). The shell keeps fewer and evicts.
3. **The handle reports back.** `close(id, reason)` sends `NotificationClosed` with
   `Expired` (1), `Dismissed` (2) or `Undefined` (4); `CloseNotification` from an app is
   answered by the server itself with reason 3 before the callback hears of it. `invoke(id,
   key, token)` sends `ActivationToken` (when there is one) and then `ActionInvoked`, the
   order the spec asks for, and leaves the notification open: the shell closes it as
   dismissed unless it is `resident`. Both do nothing for ids that are not open.
   `CloseNotification` on an id that is not open is ignored rather than answered with the
   spec's "empty error", which apps closing stale ids would only log.
4. **Parsing is lenient.** Actions pair up (an odd last key is dropped, an empty label takes
   its key). Hints read: `urgency` (any integer type), `category`, `desktop-entry`,
   `image-data`/`image_data`/`icon_data` (only 8-bit RGB or RGBA, at most 4096 px a side,
   converted to straight RGBA without row padding), `image-path`/`image_path`, `resident`,
   `transient`; values of the wrong type are skipped. `Notification::icon` picks the
   source in the spec's order (image data, image path, `app_icon`, `icon_data`) as pixels,
   a file (path or `file://` URI, percent-decoded) or an icon name. `expire_timeout` is a
   `Timeout` (`Default`, `Never`, `After`).
5. **Capabilities:** `actions`, `body`, `icon-static`. No `body-markup` until banners draw
   markup, no `persistence` until the Notification Center (Phase 6), no sounds.
   `GetServerInformation` is `("neue-shell", "neueOS", <version>, "1.2")`.
6. **`neue_shell::notifications::Store<T>`** (portable) holds the open notifications
   oldest first, each with a `Lifetime` (`Never` or `After`) and a clock: running, paused
   (the pointer is over it; the time left is kept) or stopped. `lifetime(requested,
   critical, default)` is the policy: critical never expires whatever the app asked, an
   app's own time is kept, otherwise the configured default. `Lifetime::Never` is what the
   shell shows as an alert, everything else as a banner. A replacement keeps its place and
   restarts its clock (still paused if hovered); a full store evicts the oldest. The
   caller passes the time in and wakes at `next_deadline` to `expire`.
7. **Config:** `shell.toml` `[notifications]` with `do-not-disturb` (false) and
   `banner-timeout` (milliseconds, 1000 to 60000, default 5000).

## Consequences
- The store is generic over the notification, so the shell stores `neue_dbus`'s type on
  Linux and tests use anything; no D-Bus types reach portable code.
- Two places know an id is open: the server's set and the shell's store. The shell must
  call `close` for everything it drops (expiry, dismissal, eviction) so they agree; an id
  the server still holds after the shell forgot it is harmless except against the limit.
- Do Not Disturb is the shell's choice of what to show (3.15d); the server accepts
  notifications either way, so apps see no difference.
