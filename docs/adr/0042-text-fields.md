# 0042: Text fields: our own editor widget, secure input, search field

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
Task 2.14 adds the text, search and secure fields, with IME composition working on macOS
and in the VM. Masonry has a text editor widget (`TextArea`/`TextInput` in the `masonry`
crate), but app and library crates may only use `masonry_core` (ADR 0002 §4), and its
looks, keys and actions are not ours. parley's `PlainEditor` does the editing itself
(selection, word and cluster movement, IME preedit with underline, AccessKit text runs
through `masonry_core::core::PlainEditorAccessibility`). Both drivers already deliver
IME events and paste (ADR 0032, 0034).

## Decision
1. **One widget, three kinds.** `neue_kit::TextField` with `new()`, `search()` and
   `secure()`, like `BevelButton::recessed` (ADR 0040). The widget wraps a `PlainEditor`;
   its event code follows Masonry's `TextArea` (MIT OR Apache-2.0), cut down to one line.
   Line breaks and tabs that are typed or pasted become spaces.
2. **The app owns the text** (ADR 0040 §1). `on_change(String)` sends the whole text after
   every user change; IME preedit doesn't count until committed. `on_submit(String)` is
   sent on Return. **Without a submit handler, Return is not handled**, so it reaches the
   window's default button (ADR 0037 §4), as in a dialog's fields. `Handle::set_text` sends
   nothing.
3. **One line that scrolls.** The editor lays out without wrapping; after each layout the
   text scrolls sideways just enough to keep the caret inside the text area, and back as
   far as the text allows. Text is clipped to the well. Up and Home go to the start, Down
   and End to the end.
4. **Keys.** The command key is Super (⌘) on every host (select all, copy, cut; paste is
   the drivers'; amended by ADR 0080, it was Ctrl off macOS); words move with Option on
   macOS and Ctrl elsewhere; Cmd-Left/Right and Cmd-Backspace/Delete reach the ends. Tab into a field
   selects all its text; a click places the caret, a double click selects a word, a
   triple click everything. Escape clears a search field that has text. Undo is not here.
5. **Secure fields** keep the real text in the widget and give the editor one `•` per
   character. Insertions are mirrored by character index; deletions are worked out from
   the editor's length before and after (every bullet is one cluster of the same size,
   so what was deleted ends at the new caret). Hence: only bullets are laid out, drawn,
   and exposed to assistive technology (role `PasswordInput`); copy and cut do nothing;
   word moves go to the ends; and the field doesn't accept text input in Masonry's
   sense, so **no IME session** starts (secure input bypasses input methods). A platform
   that commits text anyway still types.
6. **Search fields** are rounded, with a magnifier on the left and, while they hold
   text, a cancel button on the right that clears on click (sending the empty text). The
   placeholder is "Search" unless set.
7. **Caret blink:** half of `motion::CARET_BLINK` (1 s) on, half off, restarting at every
   keystroke and stopping (shown) after `CARET_BLINK_TIMEOUT` (10 s) idle, so an idle
   focused field stops asking for frames. Under reduced motion the caret doesn't blink.
   The caret, the focus ring and the selection colour show only in the active window; in
   an inactive window the selection is grey (`unfocused_selection`).
8. **Paint and tokens.** The well (`neue_paint::materials::text_field`: square or round,
   darker top border, inner shadow, focus ring) and its palette
   (`neue_theme::materials::text_field`) follow the material pattern; text, selection
   and caret are drawn by the widget on the Masonry painter. New tokens: the semantic
   colour `text_selection` (a light accent tint: selected text keeps its colour, unlike
   row selection), `control::TEXT_FIELD_WIDTH` (200), `TEXT_FIELD_INSET`,
   `SEARCH_FIELD_INSET`, and the blink durations.
9. **AccessKit:** roles `TextInput`, `SearchInput`, `PasswordInput`; the placeholder; text
   runs and the selection from `PlainEditorAccessibility`, with `SetTextSelection`; and
   `SetValue`, which replaces the text and sends `on_change`.

## Consequences
- The Wayland driver still doesn't own the primary selection: the widget would have to
  report its selected text, and the platform would need an API to set the primary
  selection (a cross-crate change). Task 2.28.
- No undo/redo, no context menu (2.17 brings menus), no drag-and-drop of text, no
  password-reveal or Caps Lock indicator. Multi-line text is the text view (2.16).
- `neue_kit::testing::Fixture` gains `type_text`, `select_text` and `compose`, so goldens
  can show typed, selected and composing states. 68 goldens (17 states × light/dark ×
  1x/2x).
- A focused field animates for at most 10 s after the last key, then stops.

## Alternatives considered
- Depending on `masonry`'s `TextInput` and styling it: crosses ADR 0002 §4, and its
  Return/Escape actions, blink and colours don't match; we would wrap more than we'd
  save.
- Secure fields by drawing bullets over a hidden editor layout: the caret, selection and
  hit testing would follow the real glyphs' widths, and the real text would still reach
  the accessibility tree.
