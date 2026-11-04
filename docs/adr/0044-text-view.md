# 0044: Text view: multi-line text as scroll view content

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
Task 2.16 adds multi-line text: an editable and a read-only view that wrap, scroll and keep
the caret in sight. The text field (ADR 0042) already wraps parley's `PlainEditor` in our
own widget, and the scroll view (ADR 0043) already clips, scrolls, draws overlay scrollers
and reveals what takes focus.

## Decision
1. **`neue_kit::TextView`** is a `ScrollView` whose content is `TextViewWidget`, drawn in a
   text field's well. `TextView::new()` (`.text`, `.editable`, `.width`, `.height`,
   `.on_change`, `.disabled`, `.bind`); defaults 300 × 120 (`control::TEXT_VIEW_WIDTH`,
   `TEXT_VIEW_HEIGHT`), text inset `TEXT_VIEW_INSET`. `Handle::{set_text, text,
   set_enabled}`; `set_text` puts the caret at the start and sends nothing. The app owns
   the text (ADR 0040 §1): `on_change(String)` sends the whole text after each user change.
2. **The widget is as tall as its text.** It wraps at the scroll view's width (the content
   is as wide as the view along a non-scrolling axis) and measures its height for that
   width, as Masonry's `TextArea` does, putting the old wrap width back after a speculative
   measure. There is no sideways scrolling.
3. **The scroll view frames it.** `ScrollWidget` gains a private `framed` mode, used only
   by `TextView`: it paints the well (`neue_paint::materials::text_field`, square) under
   its content, clips the content inside the border, draws the focus ring for focus
   anywhere inside (`has_focus_target`), and is not a Tab stop itself, so Tab reaches the
   text view alone. The content paints no background.
4. **Keeping the caret in view.** After every event that can move it, the widget lays out
   again and calls `request_scroll_to` with the caret's rectangle, grown by the inset above
   and below so the first and last lines scroll flush. The request arrives before the
   content has grown (a new last line), so `ScrollWidget` remembers it and applies it again
   after its next layout; compose clears it. This also fixes the same case for any content
   that grows while taking focus.
5. **Keys** (the command key is Cmd on macOS hosts and Ctrl elsewhere; word moves use
   Option on macOS and Ctrl elsewhere): Return inserts a line break; Up and Down move
   between wrapped lines; Home and End go to the ends of the line, with the command key to
   the ends of the text; on macOS Cmd-Left/Right go to the ends of the line and Cmd-Up/Down
   to the ends of the text; Cmd-Backspace/Delete delete to the end of the line. Select all,
   copy, cut and paste as in a field; pasted text keeps line breaks (`\r\n` and `\r`
   become `\n`) and tabs. A click places the caret, a double click selects a word, a triple
   click a paragraph. **Page Up and Page Down, and Tab, are not handled by the widget**: the
   first two bubble to the scroll view, which scrolls without moving the caret, and Tab
   moves focus (no tab characters can be typed). Tab into a view doesn't select its text.
6. **Read-only views** (`editable(false)`) select and copy but never change: no typing,
   cut, paste, IME session or `SetValue`, no caret. Return isn't used, so it reaches the
   window's default button. The AccessKit node is `read_only`.
7. **AccessKit:** role `MultilineTextInput`, text runs and the selection from
   `PlainEditorAccessibility`, `SetTextSelection`, and `SetValue` (editable only), which
   replaces the text and sends `on_change`. The scroll view around it is the existing
   `ScrollView` node.
8. **Reuse.** The caret blink (`Blink`, ADR 0042 §7) is shared with the field. `Fixture::
   focus` focuses the first focusable widget at or below its target, and `Fixture::
   select_text` presses the command key and A, which works in fields and views.

## Consequences
- No undo, no context menu (2.17), no primary selection (2.28), no drag-and-drop of text,
  no find bar, no rich text, no line numbers or code editing (a tab-inserting mode and a
  monospace style belong to Write and to the code editor, Phase 7).
- The editor lays the whole text out, so very long documents (megabytes) are slow; a
  virtualised document view would be a separate widget.
- Page Up and Page Down don't move the caret.
- Goldens: 48 in `apps/gallery/tests/golden` (empty, focused, typed with a wrapped line,
  typed past the bottom, composing, inactive; filled at rest, scrolled, selected,
  disabled; read-only at rest and selected) × light/dark × 1x/2x.

## Alternatives considered
- **A text view that is its own scroll container** (wheel, scrollers and clipping inside
  the widget): duplicates ADR 0043 and the scrollers' code; the scroll view's AccessKit
  actions, scroll-into-view and overlay scrollers come for free as content.
- **Making `TextField` multi-line with a flag:** its one-line scrolling, search and secure
  paths and fixed height would have forked every method; the two share only the editor
  and the blink, which is little.
- **Drawing the well in the content widget:** it would scroll away with the text.
