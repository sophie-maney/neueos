# 0054: Tab view

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
Task 2.21b adds the tab view: a strip of tabs over a bordered box with one tab's content
inside, as in `NSTabView`. Settings windows and inspectors use it.

## Decision
1. **One widget owns the strip, the box and the contents.** `TabViewWidget` (Masonry)
   draws the strip and the box, hit-tests the tabs and handles the keys; each tab's content
   is a child, wrapped in a small `TabPanelWidget` (role `TabPanel`, labelled with the
   tab's title) that fills the content area. The strip is the view's own, not a
   `Segmented` widget: tabs can be disabled one by one, the selected tab is neutral while
   the window is inactive, and the selection is not animated. It reuses the segmented
   control's gel (`neue_paint::materials::{tab_strip, tab_box}`).
2. **Contents are built up front, only the selected one is shown.** All panels are
   children and take part in `measure` (the view is as large as the largest tab), but
   layout stashes every panel except the selected one, so the others take no focus, no
   input and no accessibility node.
3. **Layout.** The strip is 22 px tall and centred; the box's top edge runs behind the
   middle of the strip; the content sits 12 px in from the box at the sides and bottom and
   8 px below the strip.
4. **Selection.** The user selects by click (press and release on the same enabled
   tab), Left and Right on the focused strip (over enabled tabs, no wrap), Home and End,
   and Ctrl-Tab and Ctrl-Shift-Tab (wrapping) from anywhere in the view, as the key
   bubbles up from the focused content. Selecting by the user moves the keyboard focus
   to the strip, because it may have been inside the tab that just got hidden. A disabled
   tab can't be selected by the user; the app may (`Handle::set_selected`), as with
   `NSTabView`. `on_select(usize)` sends the new index; `Handle` changes send nothing.
5. **Accessibility.** The view is a generic container that takes focus, with the selected
   title as its value and `Increment` and `Decrement` actions. Its children are a
   synthetic `TabList` node, built in `accessibility` with `AccessCtx::tree_update` as
   the text widgets do, holding one `Tab` node per tab (label, selected, disabled,
   position in set, controls the selected panel), and the selected `TabPanel`.
6. **Colours and metrics** are `neue_theme::materials::tabs`; disabled dims strip and box.

## Consequences
- `neue_kit::TabView` (`new`, `with_tab`, `with_disabled_tab`, `selected`, `on_select`,
  `width`, `height`, `disabled`, `bind`; `Handle::{set_selected, set_tab_enabled,
  set_enabled}`); `neue_paint::materials::{TabStrip, tab_strip, tab_box,
  tab_strip_outline}`; `neue_theme::materials::tabs`.
- Goldens: `tabs` (normal, focused, disabled, inactive) and `tabs-second`; gallery page
  "Panes" (a "Tab view" section).
- Not yet: closable or reorderable tabs, tabs on other edges, a tab that overflows the
  view (the strip spills out evenly), an animated switch. The tab nodes are synthetic, so
  assistive technology can read them but not click them (Masonry only routes actions to
  widgets); it selects with the view's `Increment` and `Decrement`.
