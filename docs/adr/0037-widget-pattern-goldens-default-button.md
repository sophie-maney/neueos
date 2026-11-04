# 0037: The widget pattern: gallery goldens, window root, default button

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
Task 2.9 builds the push button as the pattern every later widget task copies (AGENTS.md:
gallery page, goldens of states × light/dark × scale 1 and 2, AccessKit, keyboard). It
needs four things neue-kit didn't have:
- a way for app crates to render widgets for goldens without depending on `masonry_*`
  (ADR 0002 §4);
- Return pressing the window's default button from anywhere in the window. Masonry sends
  keys only to the focused widget and its ancestors, and when nothing has focus, to the
  window's *focus fallback*. The default button is neither;
- the default button's pulse, which runs only while its window is active. Masonry tells
  only the focused widget (or the fallback) that window focus changed;
- live theme switches and a zoom for the gallery's appearance, accent and scale
  switches.

## Decision
1. **Golden comparison moves to `neue_paint::golden`** (`Goldens::check` and `compare`,
   tolerances unchanged), so neue-paint, neue-kit and app crates share it. Mismatches are
   errors, not panics.
2. **`neue_kit::testing` (feature `testing`)** wraps `masonry_testing` behind our own
   API: a `Fixture` puts one element in a window (with the theme's window background and
   an 8 px margin), drives it into a state (`hover`, `press`, `focus`, `set_disabled`,
   `set_window_focused`, `advance`) and renders it with `vello_cpu` into a premultiplied
   pixmap. `check_states` renders every `State` × light/dark × scale 1 and 2 against
   `{name}-{state}-{light|dark}@{1|2}x.png`. App crates enable the feature in their
   dev-dependencies only.
3. **Goldens live with the gallery** (`apps/gallery/tests/golden/`), rendered from the
   same `specimens` the gallery's pages show. `neue-gallery` is in `GOLDEN_CRATES`.
4. **Every neue-kit window gets a root widget** (`RootWidget`, crate-private) that paints
   the theme's window background and is the window's focus fallback
   (`WindowSpec::focus_fallback`, new in neue-platform; both drivers call
   `RenderRoot::set_focus_fallback`). It turns unhandled Return (and keypad Enter) into
   `RootAction::DefaultKey`, and window focus changes into `RootAction::WindowFocus`.
   The runner handles both: it calls the default button's handler (if the button is
   enabled), and tells the button the window's focus changed.
5. **The default button is wiring, like message handlers.** `Button::as_default` marks
   it; `Element` carries the default button's id up through containers (`Wiring`), and
   the runner keeps one per window. A second default button in a window is ignored with a
   warning.
6. **Push button behaviour follows the desktop it imitates:** a click doesn't take
   keyboard focus (Tab does); Space presses the focused button and shows it pressed while
   held; Return belongs to the default button only. The focus ring and the default look
   show only in the active window. AccessKit gets role `Button`, the label, the `Click`
   action, and keyboard shortcut "Return" on the default button.
7. **The pulse is an ambient loop, not a spring:** a cosine over
   `motion::DEFAULT_BUTTON_PULSE` that brightens the gel by up to
   `materials::aqua_button::PULSE_LIGHTEN`. It runs on animation frames only while the
   button is the default, enabled, in the active window, and motion is not reduced;
   otherwise it rests at 0 and stops asking for frames. Window focus, enabling and theme
   changes restart it.
8. **`Cx::set_theme`** swaps the default properties of every open window
   (`RenderRoot::set_default_properties`). **`Zoom`** draws a child at a multiple of its
   size through its transform; the gallery's scale switch uses it, because overriding the
   device scale would need a platform API (input and surface sizes). Zoom multiplies the
   display's scale: on a 2× display, zoom 1 is the real 2× rendering.

## Consequences
- Later widget tasks add a page to `apps/gallery/src/pages.rs`, specimens to
  `specimens.rs`, and a `check_states` test; states beyond the built-in ones are
  `(name, fn(&mut Fixture))` pairs.
- The fixture drops message handlers: behaviour tests stay in neue-kit's own tests on
  the fake host.
- Escape for a cancel button would use the same root mechanism; it isn't wired yet.
- Return presses the default button without showing it pressed for a moment, as the
  original did. Cosmetic; revisit with dialogs (2.22).
- A pulsing default button repaints itself every frame while its window is active.
  It's one button's paint, but the performance check on real hardware should include it.
