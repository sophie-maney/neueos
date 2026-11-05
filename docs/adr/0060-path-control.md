# 0060: Path control: one leaf widget, the popup style is a pop-up button

- **Status:** Accepted
- **Date:** 2026-11-05

## Context
Task 2.24a. Mountain Lion's path control shows a file path as a row of segments (standard
style) or as a pop-up button listing the path (popup style). The kit needs both, with the
keyboard and AccessKit, and the row must cope with a width narrower than the path.

## Decision
1. **The standard style is one leaf Masonry widget** (`PathWidget`), not a row of button
   widgets. The segments are text and a hover fill, so the widget lays them out and hit
   tests them itself, like `Segmented`. The look is `neue_paint::materials::path_control`
   (hover and pressed fills, chevrons, a focus ring around one segment), with tokens in
   `neue_theme::materials::path_control`. It sends the chosen segment's index in the path
   (0 is the root); the last segment, the current level, can be chosen too.
2. **Collapsing.** When the row is wider than its box, the segments after the first are
   replaced by an ellipsis, as few as needed, and the last segment is always kept. The
   ellipsis takes no input. The minimum width (`LenReq::MinContent`) is the first segment,
   the ellipsis and the last. If even that does not fit, the row is clipped.
3. **Keyboard.** One focus stop. Left and Right move a focus between the drawn segments
   (skipping the ellipsis), Return or Space choose it; a click moves the focus to the
   clicked segment. The focus starts on the last segment. A collapse that hides the
   focused segment moves it to the next drawn one.
4. **The popup style is `PathControl::popup(segments, make)`**, which returns a
   `PopUpButton` over the segments in reverse (current level first, root last) and maps
   its index back to the path index. No second widget, and its look and keyboard are the
   pop-up button's (ADR 0046).
5. **AccessKit.** The widget is a `Group` whose value is the path joined with " / ", with a
   `Button` child node per drawn segment (the last one selected). Masonry's `AccessEvent`
   does not say which child node an action was for, so the segments cannot be clicked
   individually: `Click` on the group chooses the focused segment.

## Consequences
- No icons in the segments yet, and no drag and drop onto a segment.
- Assistive technology can read every segment but activates only the focused one; fixing
  that needs Masonry to route actions to synthetic child nodes (the tab view has the same
  limit).
- A hidden segment is not reachable by the keyboard or the pointer. A menu on the
  ellipsis would fix that.
