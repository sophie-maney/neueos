# 0035: neue-kit's app model: messages, retained widgets, handles, background tasks

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
Task 2.7 gives apps their API. ADR 0002 §4 puts Masonry behind neue-kit's own API, and
says the Elm-style app model is ours to design (Xilem was rejected as an extra layer).
neue-platform (ADRs 0032–0034) offers `App`/`Ctx` callbacks that hand out Masonry's
`RenderRoot`, `WidgetId` and `ErasedAction`, and has no way for another thread to wake
its loop. Apps must not see Masonry types, must run blocking work off the UI thread, and
must be checkable for the "no Masonry" rule.

## Decision
1. **Elm-style update loop over retained widgets.** An app implements `Application`: its
   struct is the model, `type Message` lists what can happen, `start` opens windows and
   `update(cx, message)` is the only place state changes, one message at a time on the
   UI thread. There is no `view` function and no tree diffing: widgets are built once and
   `update` changes them through typed **handles** (`Handle<Label>::set_text`,
   `Handle<Button<M>>::set_enabled`, `Handle<Stack<M>>::push`/`clear`). A handle starts
   unbound and a builder's `bind(&mut handle)` points it at the widget, so handles can
   live in the model from `Default`. Handles to widgets not in an open window do nothing.
2. **The widget facade.** Builders (`Label`, `Button<M>`, `Stack<M>`) wrap neue-kit's own
   Masonry widgets, built on `masonry_core` only (never the `masonry` stock widgets).
   Each converts into an `Element<M>`: the erased Masonry widget plus handlers mapping
   its actions (and its children's) to messages. `Cx::open_window` registers the
   handlers per window; closing a window, or clearing a stack, drops them. The theme
   reaches widgets as a Masonry default property (`ThemeProp`), so a later theme change
   can use `RenderRoot::set_default_properties` and repaint everything (task 2.9).
3. **Background work reports with messages.** `Cx::spawn(|sender| … -> M)` runs blocking
   work on its own thread; the closure may send progress through its `Sender` and check
   `is_cancelled`. `Cx::spawn_future` polls a future to completion on its own thread with
   `pollster`; futures that need a runtime (tokio) stay on it and send through
   `Cx::sender()`, which any thread can use. All of them feed one channel to the UI
   thread. `Task::cancel` is enforced on the UI thread at delivery, so after it returns
   no message of that task reaches `update`.
4. **neue-platform gains a waker** (a cross-crate interface change): `Ctx::waker()`
   returns a `Waker` (`Clone + Send + Sync`); `wake()` makes the loop call the new
   `App::on_wake`. Wakes coalesce through an atomic flag, cleared before `on_wake` so a
   wake during the callback queues another. The winit driver sends a user event through
   its `EventLoopProxy`; the Wayland driver pings a calloop `ping` source.
   `WindowId::from_raw` exists for fakes of the app API in tests.
5. **The "no Masonry" rule is checked.** `cargo xtask check` reads `cargo metadata` and
   fails if a crate under `apps/` or `shell/` depends directly on a `masonry*` crate.
6. **Interim widgets.** The button is the Aqua gel from neue-paint with a label, pointer,
   Space/Return and AccessKit click; task 2.9 makes it the real push button (default
   pulse, focus ring, goldens) and sets the gallery and golden pattern every later widget
   follows. `Label` is a single unwrapped line in the theme's type scale.

## Consequences
- Apps read like Elm apps (model, messages, `update`), but widget state lives in the
  widgets: `update` must keep the model and widgets in step through handles. A
  declarative `view` with diffing can be added on top later without changing messages
  or tasks.
- One OS thread per task is fine for occasional work (file I/O, thumbnails, D-Bus
  calls); a pool can replace it behind the same API if apps spawn many tasks.
- A panicking task never sends its result; the app only sees the missing message.
- Widget changes through handles from `start` don't apply (the window isn't open yet);
  initial state goes into the builders.

## Alternatives considered
- **Declarative `view` with diffing (Xilem, iced):** the most Elm-like, but it is the
  extra layer ADR 0002 turned down, and a reconciler is far beyond one task.
- **Exposing `RenderRoot`/`WidgetMut` with tags:** leaks Masonry into apps (ADR 0002 §4).
- **An async runtime in neue-kit (tokio):** heavier than apps need; services already run
  tokio themselves and can talk to the UI through `Sender`.
- **Polling a channel on a timer instead of a waker:** adds latency and idle wake-ups.
