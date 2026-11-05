# Phase 2 owner runbook

Every Phase 2 check that still needs you, in one sitting: tasks 2.16 to 2.24 and 2.28 in
the VM, then all the macOS and VoiceOver checks in one go. It replaces the "Not run yet"
sections of `docs/checks/phase-2.md`; nothing here points to another page.

Work from top to bottom. A step is one of these:
- **Run** a command: paste it into the Mac terminal, in the repo root (it works from
  fish and zsh).
- **Do** something by hand in the Parallels window, with the mouse and keyboard.

Then compare with **Expected**. Allow about 2 hours in the VM and 45 minutes on the Mac.
You can stop after any part: Part 0 says how to pick up again.

## When to report back
- **STOP AND REPORT** (a few steps in Part 0): don't go on, send the agent the terminal
  output.
- **Note** (a few steps): an observation the agent needs even when nothing fails. Write
  down the step number and what you saw.
- Anything else that doesn't match **Expected**: write down the step number (for example
  `5.3`) and one line on what you saw, take a Mac screenshot of the VM window
  (Cmd-Shift-4, Space, click the window), and **carry on**.
- At the end (Part 16), send everything in one message.

## Keys in the Parallels window
- Ctrl means the Ctrl key (not Cmd). Option is Alt. Shift is Shift.
- On a Mac keyboard: Home/End are Fn-Left/Fn-Right, Page Up/Down are Fn-Up/Fn-Down, and
  F10 may need Fn.
- If the VM has captured the pointer, Ctrl-Option releases it.

## Gallery basics
- The page list is the column on the left: click a page name to open it.
- The **status line** is the grey line at the top of the page area (it starts as "Press a
  specimen…"). Most checks read it.
- The **toolbar** at the top of the window has the switches: Light/Dark, Blue/Graphite,
  Standard/Increased, motion, and "Zoom: 1×" (each click steps 1× → 1.5× → 2×).
- "Dark and 2×" means: click the Light switch (it reads "Dark"), click Zoom twice (it
  reads "Zoom: 2×"). Check the item still looks right, then set both back (Dark once, Zoom
  once).

---

## Part 0. Set up the VM session
**0.1 Run** (asks for the VM password: stops the login screen, starts the input daemon):
```sh
ssh -t neueos-vm 'sudo systemctl stop greetd; pkill -x neue-comp; pkill -x weston; pgrep -x ydotoold >/dev/null || sudo sh -c "setsid nohup ydotoold --socket-path=/tmp/.ydotool_socket --socket-own=1000:1000 >/dev/null 2>&1 </dev/null &"; true'
```
Expected: no output, or a `[sudo] password` prompt and then nothing.

**0.2 Run** (sync and build, a few minutes):
```sh
rsync -az --delete --exclude target --exclude .git ./ neueos-vm:neueos/
ssh neueos-vm 'cd neueos && cargo build --release -p neue-comp -p neuectl --no-default-features --features tty && cargo build --release -p neue-gallery -p neue-platform --examples --bins && echo BUILD OK'
```
Expected: the last line is `BUILD OK`. **STOP AND REPORT** if it isn't.

**0.3 Run** (start the compositor and the gallery on the Fields page):
```sh
ssh neueos-vm 'export XDG_RUNTIME_DIR=/run/user/1000 RUST_LOG=info; unset WAYLAND_DISPLAY
  setsid nohup ~/neueos/target/release/neue-comp --tty >/tmp/comp.log 2>&1 </dev/null &
  sleep 3; . ~/neueos/docs/vm-env.sh; gallery 3'
```
Expected: one line with `Widget Gallery`, then `gallery log: no WARN or ERROR`. The
Parallels window shows the gallery with its own title bar (traffic lights, "Widget
Gallery") and the toolbar under it. **STOP AND REPORT** if the window list is empty or
there are `WARN`/`ERROR` lines.

**Picking up later:** if the gallery is still on screen, just carry on. If you closed it,
run `ssh neueos-vm '. ~/neueos/docs/vm-env.sh; gallery'`. If the VM rebooted, run 0.1 and
0.3 again (0.2 only if the code changed).

---

## Part 1. Primary selection, Fields page (task 2.28)
The gallery is on "Fields". The primary selection is what a middle click pastes.

- [ ] **1.1 Do:** click the **Name** field, type `neuepick`, press Shift-Home (selects it).
- [ ] **1.2 Run** (opens a foot window that records what is pasted into it):
  ```sh
  ssh neueos-vm '. ~/neueos/docs/vm-env.sh; rm -f /tmp/pasted; term sh -c "cat > /tmp/pasted"'
  ```
  Expected: a foot window opens. If it covers the gallery, drag it aside by its title bar.
- [ ] **1.3 Run**, then within 5 seconds put the pointer over the foot window (a middle
  click at the pointer, then Return):
  ```sh
  ssh neueos-vm 'export YDOTOOL_SOCKET=/tmp/.ydotool_socket; sleep 5; ydotool click 0xC2; sleep 0.5; ydotool key 28:1 28:0; sleep 0.5; echo "foot got: $(cat /tmp/pasted)"'
  ```
  Expected: `foot got: neuepick`.
- [ ] **1.4 Do:** close foot (click it, type Ctrl-D). In the gallery, clear the Name field,
  type `dragged`, and select it by dragging across it with the mouse. Then repeat 1.2 and
  1.3. Expected: `foot got: dragged`.
- [ ] **1.5 Do:** close foot. Click the **Password** field, type `secret`, press
  Shift-Home. Repeat 1.2 and 1.3. Expected: `foot got: dragged` (the password is never
  offered).
- [ ] **1.6 Do:** close foot. In the gallery select the text in the Name field, click
  into the **Note** field, put the pointer over it and run:
  ```sh
  ssh neueos-vm 'export YDOTOOL_SOCKET=/tmp/.ydotool_socket; sleep 5; ydotool click 0xC2'
  ```
  (pointer over the Note field within 5 s). Expected: the selected text is inserted in
  the Note field.

## Part 2. Text view (task 2.16)
- [ ] **2.1 Do:** click "Text View" in the page list. Expected: three text views side by
  side (editable, read-only, disabled), text wrapped; a thin scroller appears on the
  editable one only while the pointer is over it.
- [ ] **2.2 Do:** click into the first view, press Ctrl-End, type `typed`, Return, `more`.
  Expected: the view scrolls to its end, the caret after "more" on a new line, and the
  status line starts with `Notes:` and ends with `typed` and `more`.
- [ ] **2.3 Do:** click into the second (read-only) view and type: nothing changes. Press
  Ctrl-A: everything in it is selected.
- [ ] **2.4 Do:** a triple click in the first view selects a paragraph; Up and Down move
  across wrapped lines; Tab leaves the view.

## Part 3. Virtual grid, Scrolling page (task 2.20b)
- [ ] **3.1 Do:** click "Scrolling". Expected: a section "Virtual grid (1,000,000 items)"
  with 4 columns of numbers, 1 to 4 in the top row.
- [ ] **3.2 Do:** scroll the grid with the wheel or two fingers, fast and for a while.
  Expected: the numbers climb by 4 per row, no lag, and the scroller's knob is small (its
  minimum length).

## Part 4. Menus (task 2.17a)
- [ ] **4.1 Do:** click "Menus". Expected: an area "Right-click here" and a text field.
- [ ] **4.2 Do:** click into the text field, press Shift-F10. Expected: a menu at the
  field's middle with Cut highlighted; the field's focus ring is gone.
- [ ] **4.3 Do:** press T, then Right. Expected: the Text Size submenu opens to the right,
  Small highlighted.
- [ ] **4.4 Do:** press Down, then Return. Expected: both menus close, the status line
  reads `Pressed “Regular”`, the caret blinks in the field again.
- [ ] **4.5 Do:** right-click "Right-click here". Expected: the menu opens at the pointer,
  nothing highlighted. Hover "Text Size": the submenu opens beside it. Click "Large":
  both close, `Pressed “Large”`.
- [ ] **4.6 Do:** right-click again, then click outside the menu. Expected: the menu
  closes and the status line doesn't change.
- [ ] **4.7 Do:** drag the gallery so the "Right-click here" area is near the right edge
  of the screen, right-click there. Expected: the menu moves or flips to stay on screen.
  (The submenu flipping over its parent is a known gap.) Drag the gallery back.

## Part 5. Pop-ups page (tasks 2.17b, 2.24a, 2.24b, 2.24c)
- [ ] **5.1 Do:** click "Pop-ups". Expected: a pop-up button "Medium", a pull-down
  "Actions", dimmed copies of both, two combo boxes (the second dimmed), a text field and
  a text view; further down "Path control", "Colour well" and "Date picker" sections.

Pop-up button, pull-down, combo box:
- [ ] **5.2 Do:** click "Medium". Expected: a menu over the button with "Medium" ticked
  and highlighted, level with the button; the button's arrow cap dark. Click "Large": the
  button reads "Large", status `Size: “Large”`.
- [ ] **5.3 Do:** open it again and press Escape: nothing changes. (Near the top of the
  screen the menu may be pushed down so it no longer lines up: a known gap.)
- [ ] **5.4 Do:** click "Actions". Expected: a menu right below it, nothing highlighted.
  Click "Rename": `Pressed “Rename”`.
- [ ] **5.5 Do:** click the first combo box's arrow. Expected: a menu below the field, as
  wide as the field, the arrow button dark. Click "Large": the field reads "Large".
- [ ] **5.6 Do:** type into the combo field: the status line follows. Press Down: the menu
  opens with its first item highlighted. Escape.
- [ ] **5.7 Do:** right-click the text field. Expected: Cut, Copy, Select All (no Paste,
  on purpose). Select All selects everything and the field keeps focus. Do the same on the
  text view. Shift-F10 in the field opens the same menu.

Path control:
- [ ] **5.8 Look:** under "Path control", a row "Macintosh HD › Users › sophie" with grey
  chevrons and no frame, and next to it a pop-up button showing "sophie".
- [ ] **5.9 Do:** hover "Users", then hold the mouse button down on it. Expected: a soft
  grey fill behind "Users", darker while held. Release on it: `Path: Users`. Press on
  "Users", move off, release: the status line doesn't change.
- [ ] **5.10 Do:** Tab until the path row has focus. Expected: a focus ring around
  "sophie". Press Left twice, then Return: the ring is on "Macintosh HD" and the status
  line reads `Path: Macintosh HD`.
- [ ] **5.11 Do:** click the "sophie" pop-up button. Expected: a menu with "sophie" over
  the button, then "Users", then "Macintosh HD". Choose "Users": `Path: Users`.

Colour well:
- [ ] **5.12 Look:** under "Colour well", a grey bevelled button with a see-through
  orange swatch over a faint checkerboard, and a dimmed one beside it.
- [ ] **5.13 Do:** click the first well. Expected: a popover under it, its arrow pointing
  at the well: a rainbow area (fading to white at the bottom) with a round knob near the
  top left, "Brightness" and "Opacity" sliders, a "Hex" field reading `#FF8C1A99`. Click
  the well again: the panel closes.
- [ ] **5.14 Do:** open it, drag across the rainbow area. Expected: the knob follows, the
  swatch and the hex field change while you drag, the status line shows `Colour: “#…”`.
  Drag the Brightness slider fully left: the swatch goes black; back right: the same hue
  returns.
- [ ] **5.15 Do:** click the Hex field, Ctrl-A, type `#336699`. Expected: the swatch turns
  blue as soon as the value is valid, the status line shows `#33669999`, the brightness
  slider moves. Type `zz` and press Return: the field goes back to the colour's hex.
- [ ] **5.16 Do:** press Escape to close the panel. Tab to the first well, press Space.
  Expected: the panel opens with the focus inside; Tab reaches the rainbow area, and the
  arrow keys move the knob. Escape closes the panel and the focus ring is back on the well.

Date picker:
- [ ] **5.17 Look:** under "Date picker", a field `05/11/2026` with a small stepper to its
  right, a dimmed copy under it, and to the right a calendar "November 2026": the 5th
  filled blue, the 20th ringed, October and December days grey at the ends.
- [ ] **5.18 Do:** click the day part of the field, press Up. Expected: the day is
  highlighted blue and becomes 06, status `Date: 2026-11-06`. Press Right, then Down: the
  month becomes 10. Press Right and type `2031`: the year changes only on the fourth
  digit.
- [ ] **5.19 Do:** click the upper half of the stepper: the highlighted part goes up by
  one; the lower half: down by one.
- [ ] **5.20 Do:** click the 17th in the calendar: it fills blue, `Date: 2026-11-17`.
  Click the right arrow in the calendar's header: December shows, the status line doesn't
  change. Click the circle between the arrows: back to November, the 20th selected.
- [ ] **5.21 Do:** Tab to the calendar. Left/Right move a day, Up/Down a week, Page
  Up/Down a month, Home goes to 20 November; the page follows and the status line shows
  each date.

## Part 6. Outline view, Tables page (task 2.19b)
- [ ] **6.1 Do:** click "Tables", scroll down to "Outline (folders)". Expected: Documents
  open (Work and Letters under it), then Pictures, Music, Field Notes.txt, Recipes.md;
  triangles on folders only; file names lined up with the folder names.
- [ ] **6.2 Do:** click the triangle of Pictures. Expected: it opens (Beach Photo.jpg,
  Screenshot.png), nothing gets selected, the status line doesn't change. Click it again:
  it closes.
- [ ] **6.3 Do:** Option-click the triangle of Documents: it closes with everything
  inside. Click it (no Option): only Work and Letters show, both closed. Option-click it
  again: every level is open.
- [ ] **6.4 Do:** click "Budget 2026.ods". Expected: `1 row selected`, the row blue with
  white text. Close Documents by its triangle: Documents turns blue instead, still
  `1 row selected`.
- [ ] **6.5 Do:** click a closed folder's row. Right: it opens; Right: the selection moves
  to its first child; Left: back to the folder; Left: it closes. Option-Right opens every
  level, Option-Left closes them. Type `mu`: Music is selected. Expected throughout: a
  focus ring round the table; the status line counts selected rows (it doesn't change
  when a row only moves because a folder above it opened).

## Part 7. Source list, Sidebar page (task 2.19c)
- [ ] **7.1 Do:** click "Sidebar". Expected: two sidebars, light blue-grey with a line down
  the right edge. Left one: FAVORITES (Documents selected in grey, Pictures, Music with a
  triangle, Starred with badge 12) and DEVICES (Disk, Recent with badge 3). Right one:
  Music open (Albums, Artists), Devices closed. Icons the same grey as the text; no
  stripes, header row or border.
- [ ] **7.2 Do:** click Pictures. Expected: blue gradient, white text, `Selected
  “Pictures”`. Click the FAVORITES title: nothing changes.
- [ ] **7.3 Do:** hover the FAVORITES row: "Hide" appears at its right end in blue. Click
  it: Documents to Starred disappear, `Nothing selected`. Click "Show": they're back.
  Move to an item row: "Hide" goes away.
- [ ] **7.4 Do:** click Music's triangle: Albums and Artists show, nothing selected.
  Double-click Disk: `Opened “Disk”`. Click Starred: its badge turns white with blue
  digits.
- [ ] **7.5 Do:** with an item selected: Down/Up step through items and skip the DEVICES
  title; Home/End go to the first/last item; on Music, Right opens it, Right moves to
  Albums, Left moves back, Left closes it; Left on Pictures does nothing. Type `st`:
  Starred. Expected: no focus ring anywhere.
- [ ] **7.6 Run** (opens foot, which takes the focus):
  ```sh
  ssh neueos-vm '. ~/neueos/docs/vm-env.sh; term'
  ```
  Expected: the sidebar's selection turns grey with dark text. Click the gallery: it's
  blue again. Close foot (Ctrl-D in it).

## Part 8. Icons page: icons and collection view (tasks 2.20a, 2.20c)
- [ ] **8.1 Do:** click "Icons". Expected: six dark-grey glyphs (folder, page, picture,
  note, star, clock); under them a blue disc with a light tick at three sizes; under that
  two collections side by side: small icons (1,000 items, labels like "Documents",
  "Pictures 7") and large icons (11 items), labels under the icons, long ones cut with
  "…".
- [ ] **8.2 Do:** Dark and 2×. Expected: glyphs and ticks turn light, the disc a lighter
  blue; everything sharp, no blurry edges. Set both back.
- [ ] **8.3 Do:** in the small-icon collection, click an item. Expected: its icon gets a
  rounded grey backdrop, its label a blue pill with white text. Press Right, Down, End,
  Home: the selection moves over the grid and scrolls into view.
- [ ] **8.4 Do:** Shift-click another item: a range is selected. Ctrl-click one: it
  toggles. Expected: the status line reads "N items selected".
- [ ] **8.5 Do:** type `pic`: a "Pictures…" item is selected. Press Return:
  `Opened “…”`. Double-click an item: the same for it.
- [ ] **8.6 Do:** in the large-icon collection, click the empty space right of the last
  row's items: the selection clears.
- [ ] **8.7 Do:** select an item, then press Tab to move the focus away. Expected: the blue
  pill turns grey with dark text.
- [ ] **8.8 Do:** make the gallery window narrower and wider by dragging its bottom-right
  corner. Expected: the number of columns changes, items stay centred in their cells.

## Part 9. Panes page: split view and tab view (tasks 2.21a, 2.21b)
- [ ] **9.1 Do:** click "Panes". Expected: three small split views (thin, thick, stacked),
  each with "Sidebar" and "Content"; below them a wider split of three panes with thick
  dividers; below that two tab views: "General", "Advanced" and a dimmed "Hidden"; and
  "Fields", "About".
- [ ] **9.2 Do:** hover a divider: the pointer becomes a resize cursor. Drag it right and
  left, far past both ends. Expected: the sidebar stops at its limits; dragged well past
  its small end it collapses (its label disappears, the divider sits at the edge); the
  status line reads like `Panes: 70 · 129`.
- [ ] **9.3 Do:** double-click a divider twice, slowly. Expected: the pane collapses and
  comes back at its old size (a double-click toggles).
- [ ] **9.4 Do:** click the page background, then press Tab until a blue ring sits round a
  divider. Expected: Right/Left move it 10 px; Home collapses a collapsible pane; Return restores
  it. In the three-pane split, Down/Up pick the other divider and the ring follows.
- [ ] **9.5 Do:** in the first tab view click "Advanced", "Hidden", "General". Expected:
  the content follows; the selected tab is blue with a white label; "Hidden" does
  nothing; status `Tab 2`, then `Tab 1`.
- [ ] **9.6 Do:** press Tab until a blue ring sits round the first tab strip, then Right,
  Left, Home, End. Expected: the selected tab follows; the arrows skip "Hidden".
- [ ] **9.7 Do:** in the second tab view click into the "Name" field, press Ctrl-Tab, then
  Ctrl-Shift-Tab. Expected: the tab switches from inside the field and the focus moves to
  the strip.
- [ ] **9.8 Run** `ssh neueos-vm '. ~/neueos/docs/vm-env.sh; term'`. Expected: the selected
  tabs turn grey. Close foot.
- [ ] **9.9 Do:** Dark and 2×: the dividers, tabs and rings stay sharp. Set both back.

## Part 10. Toolbar page (task 2.22c)
The toolbars on this page sit on the page background, not on the title bar's gradient.
- [ ] **10.1 Do:** click "Toolbar". Expected: a "Display mode" control, then three
  toolbars. The first: Open, Favourite, a dimmed Share (icon over label), a separator, a
  List/Icons control, a search field at the far right. The second shows icons only, the
  third labels only.
- [ ] **10.2 Do:** hold the mouse button down on "Favourite" in the first toolbar: a dark
  pill behind it while held. Click the dimmed "Share": nothing.
- [ ] **10.3 Do:** click "Open", press Tab. Expected: the focus ring is on "Favourite"
  (Share and the separator are skipped). Tab again: the ring is on List/Icons.
- [ ] **10.4 Do:** in "Display mode" click "Icon only", then "Text only". Expected: only
  the first toolbar changes (icons alone, shorter; then labels alone, shorter still), and
  the page below moves up with it.

## Part 11. Popovers page: popover, sheet, sheet placement, HUD (task 2.23)
- [ ] **11.1 Do:** click "Popovers". Expected: four buttons Below, Above, Left, Right; under
  "Show a sheet" a "Save…" button; under "HUD panel" an "Inspector" button.

Popover:
- [ ] **11.2 Do:** click "Below". Expected: a light panel ("Popover", a line of text, Done)
  under the button, its arrow touching the button's middle, its shadow around it. Press
  Escape: it closes, status `Popover closed`. Repeat with Above, Left and Right: each on
  its side.
- [ ] **11.3 Run:**
  ```sh
  ssh neueos-vm 'grep -c "popup configured" /tmp/gallery.log; grep -c "painting the layer inline" /tmp/gallery.log'
  ```
  Expected: the first number is at least 4, the second `0`.
- [ ] **11.4 Do:** click "Below", then the page background: it closes (`Popover closed`).
  Click "Below" twice: it closes and doesn't reopen. Click "Below", then Done: `Popover
  done`. Click "Below", press Tab: the focus ring is on Done. Escape.
- [ ] **11.5 Do:** drag the gallery up by its title bar until the "Above" button is near
  the top of the screen, click "Above". Expected: the popover opens below the button,
  arrow on its top side. Drag the gallery to the left edge and click "Left": it opens on
  the right. Drag the gallery back to the middle.
- [ ] **11.6 Do:** Dark, open a popover: the panel is dark. Escape, set Light back.

Sheet:
- [ ] **11.7 Do:** click "Save…". Expected: a panel with no title bar hangs from the top of
  the gallery, centred, under its toolbar: "Save changes to “Notes”?", a line of text,
  Cancel and a pulsing blue Save at the bottom right, a darker band fading down its top.
- [ ] **11.8 Run:**
  ```sh
  ssh neueos-vm '. ~/neueos/docs/vm-env.sh; wins; child_check; grep -c "no xdg-dialog" /tmp/gallery.log'
  ```
  Expected: a window titled `Save` whose `parent` is the gallery's `id`;
  `Save: below-top=N centred=true`, where N is the height of the gallery's title bar and
  toolbar (more than 22; task 2.35), the same N in every check below; the last line `0`.
- [ ] **11.9 Do:** press Return: the sheet closes, `Sheet: saved`. Open it, press Escape:
  `Sheet: cancelled`. Open it, press Tab three times: the ring goes Cancel → Save →
  Cancel and never into the gallery. Click a page in the gallery's page list: nothing
  happens. Escape: the sheet closes and the click works again.
- [ ] **11.10 Do:** open the sheet again and leave it open. **Run** (moves the gallery):
  ```sh
  ssh neueos-vm '. ~/neueos/docs/vm-env.sh; neuectl action move-window --window $(win_id) --x 100 --y 200; sleep 0.5; child_check'
  ```
  Expected: the sheet moved with the gallery; `Save: below-top=N centred=true`.
- [ ] **11.11 Do:** drag the gallery by its title bar: the sheet follows while you drag.
  **Run:**
  ```sh
  ssh neueos-vm '. ~/neueos/docs/vm-env.sh; neuectl action resize-window --window $(win_id) --width 1000 --height 700; sleep 0.5; child_check'
  ```
  Expected: `Save: below-top=N centred=true`, and on screen it's centred on the
  wider window.
- [ ] **11.12 Run** (tries to move the sheet itself):
  ```sh
  ssh neueos-vm '. ~/neueos/docs/vm-env.sh; neuectl action move-window --window $(win_id Save) --x 0 --y 0; sleep 0.5; child_check'
  ```
  Expected: the sheet doesn't move; still `below-top=N centred=true`. Hover the
  sheet's right edge: no resize cursor. Press Escape.
- [ ] **11.13 Do:** Dark, open the sheet: dark panel, light text. Escape, set Light back.

HUD panel:
- [ ] **11.14 Do:** click "Inspector". Expected: a dark rounded panel with a small close
  disc at the top left, the title "Inspector" centred, three lines of light text, no
  compositor title bar. The gallery's title bar still looks focused (coloured traffic
  lights).
- [ ] **11.15 Run:**
  ```sh
  ssh neueos-vm '. ~/neueos/docs/vm-env.sh; wins'
  ```
  Expected: a window `Inspector`, 240 × 120, whose `parent` is the gallery's `id`.
- [ ] **11.16 Do:** click the gallery's page area, then the panel. Expected: the gallery's
  bar looks focused both times, the panel looks the same both times. **Run**
  `ssh neueos-vm '. ~/neueos/docs/vm-env.sh; term'`: both go inactive. Close foot.
- [ ] **11.17 Do:** drag the panel by its top bar: it moves. Double-click its bar: nothing
  happens. Hover the disc: a cross appears. Click it: the panel closes, the gallery stays.
- [ ] **11.18 Do:** click "Inspector" again, then the gallery's red traffic light.
  Expected: both windows close.
- [ ] **11.19 Run** (gallery back for the next part):
  ```sh
  ssh neueos-vm '. ~/neueos/docs/vm-env.sh; gallery'
  ```

## Part 12. Window chrome (task 2.22b)
- [ ] **12.1 Look:** a single title bar (no second bar from the compositor above it), about
  60 px tall with rounded top corners: traffic lights and "Widget Gallery" on top, the
  switches under them on the same gradient, a dark line between it and the content.
- [ ] **12.2 Do:** hover the traffic lights: ×, − and + appear on all three. Hold the mouse
  button down on the yellow one: it gets darker. Release on it. Expected: the gallery
  disappears (minimised).
- [ ] **12.3 Run:**
  ```sh
  ssh neueos-vm '. ~/neueos/docs/vm-env.sh; wins; neuectl action unminimize-window --window $(win_id)'
  ```
  Expected: the gallery line shows `"minimized":true`, then the gallery comes back.
- [ ] **12.4 Do:** click the green light: the window fills the screen.
  Click it again: it returns to its size.
- [ ] **12.5 Do:** drag the empty middle of the title bar: the window follows. Double-click
  an empty part of the toolbar row: it fills the screen; double-click again: back. Click
  a switch (Light/Dark): the theme changes and the window doesn't move.
- [ ] **12.6 Run** `ssh neueos-vm '. ~/neueos/docs/vm-env.sh; term'`. Expected: the
  gallery's bar, title and lights turn pale grey. Click the gallery: they're coloured
  again. Close foot.
- [ ] **12.7 Do:** Dark and 2×: the chrome stays sharp. Set both back.

## Part 13. Resize edges (task 2.22d)
- [ ] **13.1 Do:** move the pointer just outside the gallery's right edge, at mid height.
  Expected: a left-right resize cursor. Same just below the bottom edge (up-down) and at
  the bottom-right corner (diagonal).
- [ ] **13.2 Do:** drag the right edge to the right, the bottom-right corner down, the left
  edge to the left. Expected: the window grows each time, the content re-lays out and
  stays sharp. **Run** `ssh neueos-vm '. ~/neueos/docs/vm-env.sh; wins'`: the gallery's
  width and height are bigger than the 860 × 560 it started with.
- [ ] **13.3 Do:** drag from inside the toolbar row (not on a switch): the window moves,
  it doesn't resize.
- [ ] **13.4 Do:** click the green light, then hover just outside the window's edge: no
  resize cursor. Click green again.
- [ ] **13.5 Run** `ssh neueos-vm '. ~/neueos/docs/vm-env.sh; term'`, then drag foot's
  right edge. Expected: foot (with the compositor's title bar) still resizes. Close foot.

## Part 14. Menu bar export over D-Bus (task 2.18)
Only commands; watch the gallery window in step 14.2.
- [ ] **14.1 Run:**
  ```sh
  ssh neueos-vm '. ~/neueos/docs/vm-env.sh; test -S $XDG_RUNTIME_DIR/bus && echo BUS OK; neuectl windows | grep "Widget Gallery"; grep -c "no session bus" /tmp/gallery.log'
  ```
  Expected: `BUS OK`; the gallery's line ends in
  `[menu dbusmenu :1.N /org/neueos/MenuBar/0]`; the last line `0`.
- [ ] **14.2 Run:**
  ```sh
  ssh neueos-vm '. ~/neueos/docs/vm-env.sh; svc=$(gallery_menu); p=/org/neueos/MenuBar/0
    gdbus introspect --session --dest $svc --object-path $p | grep -A3 "interface com.canonical.dbusmenu"
    echo ---; gdbus call --session --dest $svc --object-path $p --method com.canonical.dbusmenu.GetLayout -- 0 -1 "[]" | cut -c1-400
    echo ---; gdbus call --session --dest $svc --object-path $p --method com.canonical.dbusmenu.Event -- 2 clicked "<0>" 0
    sleep 1; echo ---; gdbus call --session --dest $svc --object-path $p --method com.canonical.dbusmenu.GetLayout -- 1 1 "[\"label\",\"toggle-state\"]"'
  ```
  Expected, block by block: `interface com.canonical.dbusmenu` with `GetLayout`; a layout
  starting `(uint32 1, (0,` that contains `'View'`, `'Pages'` and `radio`; the third
  block `()`; the last starts `(uint32 2,` and has `'Dark Appearance'` with
  `'toggle-state': <1>`. In the VM window the gallery switched to dark.

## Part 15. Finish in the VM
- [ ] **15.1 Run** (quits the session and gives the console back to the login screen):
  ```sh
  ssh -t neueos-vm '. ~/neueos/docs/vm-env.sh; neuectl quit; sudo pkill -x ydotoold; sudo systemctl start greetd; true'
  ```
  Expected: the VM window shows the login screen.

---

## Part 16. macOS
One gallery window for everything. **Run** in the repo root:
```sh
cargo xtask gallery
```
Pages are in the list on the left (Cmd-1 to Cmd-9 reach the first nine only). VoiceOver:
Cmd-F5 turns it on and off; VO is Ctrl-Option.

Window chrome (task 2.22b):
- [ ] **16.1 Look:** no native title or buttons; our bar and toolbar, under the native
  shadow and rounded corners.
- [ ] **16.2 Do:** the lights hover, press and act (minimise, zoom, close last); dragging
  the bar moves the window; a double-click on the bar zooms it; the bar turns pale when
  another app is in front.
- [ ] **16.3 Note:** in the top 28 px of the window, do clicks and drags behave the same
  as lower in the toolbar, or does macOS take them (for example a double-click doing the
  system's zoom instead of ours)?

Fields (task 2.14):
- [ ] **16.4 Do:** add Japanese – Romaji in System Settings → Keyboard → Text Input, switch
  to it, click the Name field, type `nihon`. Expected: the text is underlined, the
  candidate window sits under the field, the status line only changes when you press
  Return. Japanese glyphs show as boxes (expected: bundled fonts only).
- [ ] **16.5 Do:** back on ABC: Option-E then E gives `é`. In the Password field the input
  menu greys out and typing shows bullets.
- [ ] **16.6 Do:** in the Name field: Cmd-A/C/X/V, Option-Left/Right (words),
  Cmd-Left/Right (ends). In the search field Escape clears it.

Text View (task 2.16):
- [ ] **16.7 Do:** in the editable view: Return, Up/Down across wrapped lines,
  Option-Left/Right, Cmd-Left/Right (line ends), Cmd-Up/Down (text ends), Cmd-A/C/X/V, a
  triple click (paragraph), Tab leaves the view, and Japanese – Romaji as in 16.4
  (underlined, candidates under the caret, committed on Return).

Menus (task 2.17a):
- [ ] **16.8 Do:** two-finger click the area: the menu at the pointer. Press, drag onto an
  item, release: it's chosen. Hover Text Size: the submenu opens. Type `w`: Wrap Lines is
  highlighted. Escape closes. In the text field, Shift-F10 opens the menu and Escape gives
  the focus back. Shortcuts show ⌘.

Pop-ups (tasks 2.17b, 2.24):
- [ ] **16.9 Do:** press and hold the pop-up, drag to an item, release: it's chosen. Click,
  release, click an item: also chosen. Tab to the pop-up: Space, Return and Down each open
  it; with it closed, type `s`: the item starting with S is chosen.
- [ ] **16.10 Do:** two-finger click the text field: Cut and Copy work (paste into another
  app to check).
- [ ] **16.11 Do:** repeat 5.9 to 5.21 quickly (path control, colour well, date picker).

Tables (task 2.19b):
- [ ] **16.12 Do:** repeat 6.2 to 6.5 in the outline.
- [ ] **16.13 VoiceOver:** Tab to the outline. Expected: read as an outline (tree grid)
  with its rows; folders read as collapsed or expanded with their level; VO-Right/Left or
  VO-Shift-Down opens and closes a folder.

Sidebar (task 2.19c):
- [ ] **16.14 Do:** repeat 7.2 to 7.5; for 7.6 click another app instead of foot.
- [ ] **16.15 VoiceOver:** Tab to the list: read as an outline; items read their label
  (badge as a description) and level; groups read collapsed or expanded; VO-Right/Left or
  VO-Shift-Down open and close them.

Icons (tasks 2.20a, 2.20c):
- [ ] **16.16 Do:** repeat 8.2 to 8.8 (Cmd-click instead of Ctrl-click).
- [ ] **16.17 VoiceOver:** the glyphs read as images with their name ("Folder", …), the
  ticks as "Done". The collection reads as a grid with row and column counts; each item as
  a cell with its label and whether it's selected.

Panes (tasks 2.21a, 2.21b):
- [ ] **16.18 Do:** repeat 9.2 to 9.7 (Ctrl-Tab is Ctrl-Tab on the Mac too).
- [ ] **16.19 VoiceOver:** a divider reads as a splitter with its value; VO-Up/Down
  (adjust) moves it. A tab view reads as a tab group with the selected tab and its
  position ("2 of 3"); VO-Up/Down changes tab. (Clicking a tab through VoiceOver isn't
  supported yet: known.)

Toolbar (task 2.22c):
- [ ] **16.20 VoiceOver:** each toolbar reads as a toolbar, its buttons by their label (also
  in the icon-only toolbar), VO-Space presses one.

Popovers (task 2.23):
- [ ] **16.21 Do:** each of Below/Above/Left/Right shows its popover on its side; Escape, a
  click outside, the button again and Done close it.
- [ ] **16.22 VoiceOver:** opening a popover moves VoiceOver into a dialog; VO-Right
  reaches "Popover", the text and Done.
- [ ] **16.23 Do:** click "Save…". Expected: the sheet appears inside the gallery window,
  centred, its top under the toolbar, with a shadow beside and below; clicks on the page
  under it do nothing; Return, Escape and Tab behave as in 11.9. (The toolbar's buttons
  still working is known.)
- [ ] **16.24 VoiceOver:** opening the sheet moves VoiceOver into a dialog named "Save".

Window chrome, VoiceOver:
- [ ] **16.25 VoiceOver:** the three lights read as buttons "Close", "Minimise", "Zoom";
  VO-Space on one acts.

## Report back
Send the agent one message with:
1. the step numbers that failed, each with one line on what you saw (screenshots
   attached),
2. your answer to **16.3**,
3. any part you skipped.

If everything passed, "all passed" is enough. The agent then ticks the parent tasks in
`docs/plan/phase-2.md` (2.23 is still open) and moves on to the exit check, 2.26.
