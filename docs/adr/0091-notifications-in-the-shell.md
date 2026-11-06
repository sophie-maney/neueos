# 0091: Notifications in the shell: a worker for the server, a notifier on the UI thread, one surface that comes and goes

- **Status:** Accepted
- **Date:** 2026-11-06

## Context
Task 3.15d3 joins the parts of 3.15: the server (ADR 0089), the banner and alert panels
and the host that stacks them (ADR 0090 and its addenda), and activation tokens for the
shell (ADR 0066/0067 addenda). The shell must own `org.freedesktop.Notifications`, show
what apps send, time it out, and answer clicks with the app's signals.

## Decision
1. **A worker thread owns the server** (`notify::run`, like the tray's worker): it takes
   the name, hands the `Arc<Notifications>` to the UI thread, and turns each request into
   an `Event::Notify` already prepared: the icon (theme name, file, `desktop-entry`'s icon
   or the pixels), the image, the plain-text body (tags and entities dropped), the action
   labels without `default`, and `shell.toml`'s `[notifications]` as it is at that moment.
   Decoding and file reads never run on the UI thread, and one worker keeps requests in
   the order they arrived.
2. **`notify::Notifier` on the UI thread** keeps a `neue_shell::notifications::Store`
   (64 at most; the oldest is closed with reason "undefined"), which owns lifetimes,
   replacement in place, pausing and expiry. One timer thread at a time sleeps until the
   store's next deadline and sends `Event::Timer`.
3. **Lifetimes.** `expire_timeout` −1 is `banner-timeout` (default 5 s), 0 is `Never`, and
   critical is `Never` whatever was asked (ADR 0089). A hovered panel pauses its clock;
   leaving it resumes with what was left.
4. **Do Not Disturb is read with each notification** instead of watched: the worker loads
   `shell.toml` per request, so a change applies to the next notification without a file
   watcher. Notifications that arrive during Do Not Disturb are kept in the store (and
   expire as usual), only not shown; one that was on screen goes off it. The bar extra
   (3.15d4) writes the file, and the shell may follow it then.
5. **One surface, on the active output.** An overlay layer surface anchored top right,
   `fit_content` (ADR 0090 addendum), no keyboard, with no output, so the compositor picks
   the active one as for the OSD. It is opened with an empty host and the first panels are
   added when the window exists (`Event::Opened`), so that they slide in. It closes once
   the last panel has slid out (`on_gone`). A surface on screen stays where it is until it
   closes; the next burst opens on whichever output is active then. The plan's
   "one surface per output" is not needed: only the active output shows banners.
6. **At most five banners are on screen** (`MAX_SHOWN`); older ones stay open and are
   taken off the screen, to be listed by the Notification Center (Phase 6).
7. **Clicks.** A click on a panel that has a `default` action, or an action button, asks
   for an activation token (`Cx::activation_token` for the surface's window, ADR 0066
   addendum) and then sends `ActivationToken` and `ActionInvoked` (`Notifications::invoke`)
   and closes the notification with reason "dismissed" unless it is `resident`. A click on
   one without a `default` action, and the close button, only dismiss it. An alert or
   banner whose app closed it meanwhile is not invoked.
8. **Banner or alert is decided by the panel:** a notification with action buttons shows as
   an alert, one without as a banner (`NotificationContent::actions`). A critical
   notification without actions is a banner that stays until dismissed.
9. `IconSource::parse` (was private `from_str`) and `neue_config::NotificationsConfig` are
   public, and `tray`'s icon theme names are shared.

## Consequences
- A notification's image or icon file is decoded in full on the worker (PNG and JPEG, the
  limits of `neue_shell::picture`); SVG files are not shown. A slow decode delays later
  notifications by its time, not the UI.
- When the compositor closes the surface (its output went away), the panels on it are gone
  and the notifications stay open; the notifier notices at the next one (`cx.windows()`)
  and opens a new surface on the active output. The older banners are not brought back.
- Not shown yet: `body-markup`, inline replies, sounds (ADR 0089).
