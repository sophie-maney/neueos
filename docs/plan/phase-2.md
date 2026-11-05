# Phase 2: Toolkit and design system v1

Task list for Phase 2 of the roadmap in `PLAN.md` §8. Architecture and decisions
live in `PLAN.md` and `docs/adr/`.

One task = one thread = one PR, as before. Order: the gate 2.1 → 2.2 first (ADR 0002 §4);
2.3 can run alongside it. Then 2.4–2.9 (the core), after which the widget tasks 2.12–2.24
mostly run in parallel. 2.26 closes the phase. Every widget task delivers a gallery page,
goldens (states × light/dark × scale 1 and 2), AccessKit roles and labels, and keyboard
operability, following the 2.9 pattern. If the gate fails (2.2), stop: 2.4 onward is
re-planned around our own tree. Deferred: the backdrop-blur protocol moves to Phase 3
(the menu bar is its first user; Phase 2 materials draw an opaque tinted fallback), and
the paper and leather materials move to Phase 6/7 (Widgets, Write).

- [x] **2.1 Masonry spike A: versions and paint** · Opus. Pin `masonry_core` and
      `masonry_testing`; reconcile `imaging` with our `vello_cpu`/`vello_gpu` 0.3 and
      parley 0.12 pins; add an `imaging::Painter` replay target to neue-paint; a Masonry
      widget paints the Aqua button on macOS (via `masonry_winit`, allowed here only).
      *Accept:* builds on macOS and Linux; the `masonry_testing` golden of the button
      matches the neue-paint golden; patch and fork cost in
      `docs/research/masonry-spike.md`.
      *Done:* fork `sophie-maney/xilem` (ADR 0030), `neue_paint::imaging`, the button goldens
      match on macOS, Linux aarch64 and Linux x86_64.
- [x] **2.2 Masonry spike B: sctk driver** · Opus. Minimal sctk driver for `RenderRoot`:
      one xdg-toplevel and one layer-shell surface, a wgpu surface, fractional scale,
      text-input-v3 into a `TextInput`, and one Masonry layer realised as an xdg-popup.
      *Accept:* works under neue-comp in the VM (checked with `neuectl screenshot`);
      go/no-go recorded as an ADR 0002 amendment.
      *Done:* **go**. New crate `neue-platform` with `wayland` (Linux-only module) and the
      `sctk_spike` (replaced by `demo` in 2.5) and `ime_probe` examples, checked in the VM (`docs/checks/phase-2.md` §1).
      Findings for 2.5/2.6 and Phase 3 are in `docs/research/masonry-spike.md`: neue-comp
      sends no scale to layer surfaces, and it has no virtual-keyboard, so fcitx5 can't bind.
- [x] **2.3 `neue-theme` crate** · Opus. New portable crate: semantic colours, metrics,
      radii, type scale and durations; light and dark; accent colour (blue, graphite);
      increased contrast; spring presets and the reduced-motion flag. neue-paint materials
      take their values from it.
      *Accept:* no colour or metric literals left in materials; existing goldens unchanged
      bit-for-bit; neue-comp still builds.
      *Done:* crate `neue-theme` (ADR 0031): `Variant` (appearance × accent × contrast)
      selects colours; metrics, radii, type scale and motion are constants; a materials
      table for the Aqua button, linen and title bar. Existing goldens re-blessed with no
      byte change; 24 new goldens for graphite and increased contrast.
- [ ] **2.4 `neue-platform` winit driver** · Opus. Our own `RenderRoot` driver replacing
      `masonry_winit`: wgpu surface, scale, pointer and keyboard, IME, cursor, clipboard,
      animation frames, multiple windows, AccessKit (`accesskit_winit`).
      *Accept:* a text-field demo runs on macOS, IME works, VoiceOver reads the tree.
      *Status:* implemented. Owner checks (`docs/checks/phase-2.md` §2): steps 1, 2 and 5
      (demo, typing/clipboard/layers, several windows) passed; 3 (IME) and 4 (VoiceOver)
      pending. `neue_platform::winit` (ADR 0032) is **macOS only**:
      on Linux, `ui-events-winit` would pull in X11 crates. Shared `neue_platform::gpu`
      renderer and `Options`; `App`/`Ctx` API with run-time windows; `winit_demo` example (renamed `demo` in 2.5).
- [x] **2.5 `neue-platform` Wayland driver: core** · Opus. Harden the 2.2 driver:
      xdg-toplevel, layer-shell, fractional scale + viewporter, frame callbacks,
      cursor-shape, text-input-v3, keyboard repeat.
      Align the driver with the winit driver's `App`/`Ctx` API (ADR 0032), so the 2.4 demo
      runs on both.
      *Accept:* the 2.4 demo runs under neue-comp in the VM; a headless neue-comp test maps
      a window from it.
      *Done:* ADR 0033. `App`/`Ctx`/`WindowSpec` (with `WindowKind::Layer`) at the crate
      root, `neue_platform::run` per host; `wayland::run_with_connection`; per-surface
      frame-callback pacing; cursor shapes with xcursor fallback; key repeat and winit's
      key tables; click counting. `examples/demo.rs` replaces `winit_demo` and
      `sctk_spike`; `neue-comp/tests/platform.rs` maps and draws a driver window. VM check
      `docs/checks/phase-2.md` §3 passed: steps 1–4 run by the agent, 5–6 (cursor shapes,
      scale 1.5) by the owner.
- [x] **2.6 Wayland driver: popups, clipboard, a11y** · Opus. xdg-popup positioning for
      all Masonry layers, clipboard and primary selection, xdg-activation, xdg-dialog,
      AccessKit on Linux (`accesskit_unix`).
      *Accept:* in the VM, a popup flips at screen edges, copy and paste works with foot,
      and Orca or `accerciser` sees the tree.
      *Done:* ADR 0034. Per-kind placement (menus anchor to the widget they open under,
      tooltips don't grab), nested menus, `xdg_popup.reposition`; clipboard both ways;
      primary selection **paste only** (Masonry reports no selections: 2.14);
      `TakeFocus` and the start-up token through xdg-activation; `WindowSpec::parent` and
      `modal`; one `accesskit_unix` adapter per window. Also fixes neue-comp, which never
      unconstrained layer-shell popups. VM check `docs/checks/phase-2.md` §4, run by the
      agent: an AT-SPI client from Python stands in for Orca and accerciser (neither is
      installed).
- [x] **2.7 `neue-kit` app model** · Opus. Our public API over Masonry: widget facade,
      Elm-style app model, background tasks returning messages. ADR.
      *Accept:* counter and async examples on macOS; app crates don't depend on
      `masonry_*` (checked by xtask).
      *Done:* ADR 0035. New crate `neue-kit`: `Application` (model, `Message`, `update`)
      over retained widgets changed through `Handle`s; builders `Label`, `Button`,
      `Stack` (interim looks; 2.9 makes the real push button); `Cx::spawn`,
      `spawn_future`, `sender` and cancellable `Task`s. neue-platform gains `Ctx::waker`
      and `App::on_wake`. Examples `counter` and `async_tasks` checked on macOS; headless
      app-model tests on a fake host. `cargo xtask check` fails if a crate under `apps/`
      or `shell/` depends on `masonry*`. No goldens yet: 2.9 builds the golden helper.
- [x] **2.8 Springs and animation** · Opus. Spring animation on `on_anim_frame` with the
      `neue-theme` presets, honouring reduced motion.
      *Accept:* unit tests step deterministically; reduced motion snaps to the end value.
- [x] **2.9 Gallery, golden harness, push button (the pattern)** · Opus. `apps/gallery`
      with page list and light/dark, scale and accent switches; a golden helper in
      neue-kit (states × light/dark × scale 1 and 2), gallery added to `GOLDEN_CRATES`;
      the Aqua push button with default-pulse, focus ring, AccessKit and keyboard.
      *Accept:* the gallery runs on macOS and in the VM; button goldens pass on macOS and
      Linux.
      *Done:* ADR 0037. `apps/gallery` (`neue-gallery`, `cargo xtask gallery`): page list,
      switches for appearance, accent, contrast, reduced motion and zoom (zoom multiplies
      the display scale; a real scale override would need a platform API). Golden
      comparison moved to `neue_paint::golden`; `neue_kit::testing` (feature) renders a
      `Fixture` in states and `check_states` covers states × light/dark × 1x/2x; 52
      push-button goldens. Push button: neutral gel, accent when default or held, pulse,
      focus ring, disabled; Space presses the focused button, Return the default button
      via a window root that is the focus fallback (new `WindowSpec::focus_fallback`).
      Also `Cx::set_theme` and `Zoom`. Goldens pass on macOS, Linux aarch64 and x86_64;
      VM check `docs/checks/phase-2.md` §5 run by the agent.
- [x] **2.10 UI font decision (Q2)** · Sonnet. Gallery page comparing the PLAN.md §4 candidates
      side by side; **the owner picks**; bundle the font (OFL) and write an ADR.
      *Accept:* the theme's type scale uses it; goldens re-blessed after diff review.
      *Done:* ADR 0038. The owner compared the four §4 candidates plus DejaVu Sans on a
      temporary Fonts page and picked **DejaVu Sans** (Bitstream Vera licence, not OFL).
      `UI_FAMILY` and the bundled font in `neue-paint` changed; the 88 text-bearing goldens
      (gallery buttons, title bars, button materials) were re-blessed after spot-checking.
      The comparison page and its multi-font plumbing were removed again (commits
      b57c90c, edf9bc5 in the branch history); `NEUE_GALLERY_PAGE=<n>` stays for screenshots.
      Regular weight only, so no bold yet.
- [x] **2.11 Materials for Phases 2–3** · Sonnet. Dark HUD, translucent menu bar, glass
      dock shelf, in light and dark, plus a Materials gallery page.
      *Accept:* goldens per material; CPU and GPU renders match within tolerance.
      *Done:* ADR 0039. `neue_theme::materials::{menu_bar, dock_shelf, hud}` palettes with
      alpha and `Backdrop::{Opaque, Translucent}` (opaque is the derived Phase 2 fallback;
      increased contrast forces it); drawing in `neue_paint::materials` (dock shelf in
      perspective and flat). 48 neue-paint goldens plus 16 gallery goldens of the new
      `neue_kit::Specimen`; `neue-paint/tests/gpu.rs` compares CPU and GPU renders (max
      channel difference 2 on Apple GPUs; skips without an adapter). Gallery page
      "Materials" (window now 860×560). Goldens pass on macOS, Linux aarch64 and x86_64.
- [x] **2.12 Buttons batch** · Sonnet. Bevel and recessed buttons, checkbox, radio,
      segmented control.
      *Done:* ADR 0040. `neue_paint::materials::{bevel_button, recessed_button, toggle,
      segmented}` (gel reused from the push button), neue-kit `BevelButton` (bevel and
      recessed, momentary or toggle), `Checkbox` (with mixed), `Radio`, `Segmented`; the
      first springs in use (check mark fade, sliding highlight). Gallery page "Buttons";
      168 goldens (42 states × light/dark × 1x/2x). Radio groups are the app's job for now
      (no arrow keys between radio buttons); segments are not separate AccessKit nodes.
- [x] **2.13 Value controls** · Sonnet. Slider, stepper, progress bar with barber pole,
      spinner.
      *Done:* ADR 0041. `neue_paint::materials::{slider, stepper, progress_bar, spinner}`
      over a shared groove, with palettes in `neue_theme::materials`; neue-kit `Slider`,
      `Stepper`, `ProgressBar` (determinate on a spring, indeterminate barber pole) and
      `Spinner`; `Ticker` for ambient animation that stops under reduced motion. Gallery
      page "Values" (the slider drives a bar and the stepper a label); 88 goldens, passing on
      macOS and Linux aarch64 (x86_64 not run). New
      widget types must be added to `theme::default_properties` (ADR 0041 §8). Not yet:
      slider ticks, vertical slider, stepper hold-to-repeat.
- [x] **2.14 Text fields** · Opus. Text, search and secure fields.
      *Accept also:* IME composition works on macOS and in the VM.
      *Done:* ADR 0042. `neue_kit::TextField` (`new`, `search`, `secure`) on parley's
      `PlainEditor`: one line that scrolls to the caret, `on_change` and `on_submit`
      (without a submit handler Return reaches the default button), Tab selects all,
      Escape and the cancel button clear a search field, blinking caret (steady under
      reduced motion). Secure fields hold bullets in the editor, so the real text never
      reaches layout or AccessKit; no copy, no IME. Well in
      `neue_paint::materials::text_field`; new tokens `text_selection`,
      `TEXT_FIELD_WIDTH`, caret blink. `Fixture::{type_text, select_text, compose}`;
      gallery page "Fields"; 68 goldens, passing on macOS and Linux aarch64 (x86_64 not
      run). IME: VM check `docs/checks/phase-2.md` §6 (input-method probe); macOS IME
      step for the owner in the same section. Not yet: undo, primary selection (2.28),
      context menu (2.17b).
- [x] **2.15 Scroll view** · Opus. Overlay scrollbars and virtual scrolling (the base for
      tables and collection views).
      *Done:* ADR 0043. `neue_kit::ScrollView` (`ScrollAxes::{Vertical, Horizontal,
      Both}`, `.width`/`.height`, `Handle::{scroll_to, offset}`): compose-time
      translation in a clip; wheel, keys (a Tab stop with a focus ring), AccessKit scroll
      actions, scroll-into-view. Overlay scrollers show on scroll or pointer entry, fade
      after `SCROLLER_HIDE_DELAY` (no fade under reduced motion), widen on hover; drag
      the knob or press the track to page. `neue_kit::VirtualList` (fixed row height)
      builds only the rows near the view; row handlers reach the runner through the
      internal `Rewired` action. Paint in `neue_paint::materials::scroller`; tokens
      `SCROLLER_WIDTH`, `SCROLL_LINE`. `Fixture::{hover_at, scroll}`; gallery page
      "Scrolling"; 28 goldens, passing on macOS and Linux aarch64 (x86_64 not run). Not
      yet: rubber-band and momentum, "always show scroll bars", variable row heights,
      grid virtualisation (2.20), a row count for AT (2.19).
- [x] **2.16 Text view** · Sonnet. Multi-line text in the scroll view.
      *Done:* ADR 0044. `neue_kit::TextView` (`new`, `text`, `editable`, `width`, `height`,
      `on_change`, `disabled`; `Handle::{set_text, text, set_enabled}`): a framed
      `ScrollView` (the well from the text field, focus ring for focus inside, not a Tab
      stop) around a wrapping editor widget that is as tall as its text. Return inserts a
      line break, Up and Down move between lines, Page Up and Down scroll, Tab moves focus;
      the caret is kept in view (the scroll view now re-applies a scroll-into-view request
      after the content grows). Read-only views select and copy. Tokens `TEXT_VIEW_WIDTH`,
      `TEXT_VIEW_HEIGHT`, `TEXT_VIEW_INSET`. Gallery page "Text View"; 48 goldens, passing
      on macOS and Linux aarch64 (x86_64 not run). `Fixture::focus` descends to the first
      focusable widget; `Fixture::select_text` is now Cmd/Ctrl-A. IME in a text view: VM and
      macOS steps in `docs/checks/phase-2.md` §7, not run. Not yet: undo, context menu (2.17b),
      primary selection (2.28), tab characters, caret moves on Page Up/Down.
- [x] **2.17a Menus: model and context menus** · Opus. Split from 2.17. The menu model
      (data for 2.18's DBusMenu export: items, separators, submenus, check and radio
      states, shortcuts, disabled), the menu layer widget (xdg-popup on Wayland, overlay
      on macOS), context menus with nested submenus, keyboard navigation, type-select.
      *Accept:* choosing by click, press-drag-release, keyboard and AccessKit sends the
      item's message; separators and disabled items are skipped; focus returns on close;
      gallery page and goldens (states × light/dark × 1×/2×).
      *Done:* ADR 0045. `neue_kit::{Menu, MenuItem, MenuToggle, Shortcut, ContextMenu}`
      (`Handle::set_menu`); `widgets/menu.rs` (`MenuWidget` layer, `MenuRowWidget` rows,
      submenus as attached layers, session state for outside presses and release-to-choose,
      focus on the highlighted row); tokens `materials::menu`, `TYPE_SELECT_RESET`; paint
      `neue_paint::materials::{menu_panel, menu_highlight, menu_separator, menu_mark,
      submenu_arrow}`. `Fixture::{context_click, press_key}`, `testing::NavKey`. Drivers
      ignore removing a layer twice. Gallery page "Menus"; 12 goldens, passing on macOS
      and Linux aarch64 (x86_64 not run); 15 end-to-end tests. VM steps in
      `docs/checks/phase-2.md` §8, not run. Not yet: Wayland submenu flip to the left,
      keeping overlays inside the window, focus return after a compositor dismissal,
      submenu delay, tall-menu scrolling, Ctrl-click, animation.
- [x] **2.17b Pop-up button, pull-down button, combo box** · Sonnet (the 2.17a pattern).
      Built on the 2.17a model and `widgets::menu::open`.
      *Accept:* a pop-up button shows its selected item and opens its menu with that item
      over the button (Wayland: anchored so the selected row lines up; overlay: same),
      sends the chosen index, and type-selects while closed; a pull-down button opens
      a menu of commands under itself with a fixed title; a combo box is a text field with
      an arrow button whose menu fills the field; Space, Return or Down open them with the
      keyboard; AccessKit roles (`ComboBox`, `has_popup`, expanded); gallery entries and
      goldens (normal, pressed/open, focused, disabled × light/dark × 1×/2×). Also the edit
      context menu (Cut, Copy, Paste, Select All) on text fields and text views.
      *Done:* ADR 0046. `neue_kit::{PopUpButton, PullDownButton, ComboBox}`: one widget for
      the first two (`widgets/popup.rs`; the pop-up's menu opens with the chosen item over
      the button, checked and highlighted; the pull-down's below it), the combo box as a
      mode of the text field (arrow button inside the well, Down opens, menu as wide as the
      field). The menu layer got `Anchor` (min width, initial highlight, `on_closed`
      callback, refocus) and opens from deferred mutations too. Default edit menu (Cut,
      Copy, Select All) on text fields and text views: secondary press, Menu key,
      Shift-F10, AccessKit; an app's `ContextMenu` around the widget wins (`menu::opened`
      counter). Paint `neue_paint::materials::{popup_button, combo_arrow}`, tokens
      `materials::popup`, `control::{POPUP_*, COMBO_ARROW_WIDTH}`. `Fixture::press_at`;
      `Fixture::new` makes shortcuts read Ctrl. Gallery page "Pop-ups"; 68 goldens, passing
      on macOS and Linux aarch64 (x86_64 not run); 20 end-to-end tests. VM steps in
      `docs/checks/phase-2.md` §9, not run. **Not done: Paste in the edit menu** (no way yet
      for a widget to request the clipboard; needs a Masonry signal and both drivers).
      Not yet: changing a pop-up's items from the app, separators and disabled items in a
      pop-up, keeping the field's selection visible while its menu is up.
- [x] **2.18 DBusMenu export** · Opus. New `neue-dbus` crate (Linux-only): export the 2.17a
      model as `com.canonical.dbusmenu` and set the KDE appmenu address (ADR 0005).
      *Accept:* a zbus test client reads the layout and fires an action; `gdbus
      introspect` works in the VM.
      *Done:* ADR 0047. `neue_dbus::{MenuExport, MenuNode, Layout, connect_session,
      testing::pair}` on zbus (`async-io` executor, blocking API; no tokio), with an
      ObjectManager at `/org/neueos/MenuBar` that also starts zbus's object server before
      any export (a lazily started one dropped early calls). neue-kit:
      `Window::menu_bar`, `Cx::set_menu_bar` (`menubar.rs`; ids = model ids + 1; chosen
      items reach `update` through a channel and the waker). neue-platform:
      `MenuAddress`, `Ctx::set_menu_address`; the Wayland driver binds
      `org_kde_kwin_appmenu_manager` and sets the window's address. The gallery has a menu
      bar. 6 client tests (peer to peer) and 3 neue-kit tests, passing on Linux aarch64
      (x86_64 not run); `gdbus introspect`, `GetLayout` and `Event` checked on a real
      session bus in OrbStack (`menu_demo` example), not in the VM: `docs/checks/phase-2.md`
      §10, not run. Not yet: menu bars on macOS hosts, key equivalents, icons,
      `ItemsPropertiesUpdated`.
- [x] **2.19a Table view** · Opus. Split from 2.19 (table, outline view, source-list
      sidebar); sets the pattern for 2.19b and 2.19c. A `Table` on `VirtualList`/
      `RowsWidget` (fixed row height): app-given columns (title, width, min/max,
      alignment, sortable) and a cell closure `|row, col| Element`; data through shared
      state and `reload` (ADR 0043 §8). Header row with a sort indicator (a click sends a
      sort message; the app sorts), column resize by dragging header dividers, optional
      alternating rows, single or multiple selection (click, Shift-click, Cmd/Ctrl-click),
      key and non-key selection colours. Keys: Up/Down (Shift extends), Home/End, Page
      Up/Down, type-select on the first column, Return or double-click sends "activate".
      *Accept:* AccessKit `Table`/`Row`/`Cell`/`ColumnHeader` with row and column counts
      (the 2.15 gap), selected state, sort direction; gallery page "Tables"; goldens
      (normal, selected, selected unfocused, header pressed/sorted, disabled ×
      light/dark × 1×/2×); end-to-end tests for selection, keys, sort and resize; ADR.
      *Done:* ADR 0048. `neue_kit::{Table, Column, Cell, SortOrder}` (`multiple`,
      `alternating_rows`, `selected`, `sorted`, `on_select`, `on_activate`, `on_sort`;
      `Handle::{set_count, reload, set_selection, set_sorted, scroll_to_row, set_enabled}`).
      `widgets/table.rs`: the body is `RowsWidget` with a `TableBody` (focus, selection,
      type-select, stripes and selection bands, AccessKit `Table`), `TableRowWidget`,
      `CellWidget` (text or element), `HeaderWidget` (sort, divider resize, `ColResize`
      cursor) and `ColumnHeaderWidget`; state shared through `TableState`. The scroll view
      got a header slot, `Frame::{Well, Border}`, and tells rows the visible rectangle.
      Paint `neue_paint::materials::{table_header, table_border, column_edges,
      sort_arrow_rect}`; tokens `neue_theme::materials::table`. Gallery page "Tables"
      (page 8); 20 goldens, passing on macOS and Linux aarch64 (x86_64 not run); 10
      end-to-end tests. VM and macOS steps in `docs/checks/phase-2.md` §11, passed. Not
      yet: variable row heights, editable cells, column reordering, row drag and drop,
      autoscroll while drag-selecting, ellipsis truncation, white element cells on a
      selection.
- [x] **2.19b Outline view** · Sonnet (the 2.19a pattern). A table whose first column is
      a tree: an app tree source (children of a node, expandable), disclosure triangles,
      expand and collapse by click, Left/Right and Option/Alt-click (recursive), indent
      per level; the visible rows are flattened and virtualised.
      *Accept:* AccessKit `TreeGrid`/`Row` with level, expanded, set size and position;
      gallery entries; goldens (collapsed, expanded, nested, selected × light/dark ×
      1×/2×); keyboard tests.
      *Done:* ADR 0049. `neue_kit::Outline<M, Id>` on a `Table`: `Outline::new(columns,
      children, cell)` or `with_expandable`; `expanded`, `selected`, `on_select(&[Id])`,
      `on_activate(&Id)` and the table's options; `Handle::{reload, set_expanded,
      set_selection, scroll_to, set_enabled}`. `widgets/outline.rs`: `Tree<Id>` (flattened
      visible rows, expansion by id) behind the object-safe `FlatTree` that the table body
      reads; the selection follows its rows through every change. First cells indent and
      draw a disclosure triangle; click (Option: all levels), Right and Left (Option: all
      levels), and the AccessKit `Expand`/`Collapse` actions toggle; AccessKit `TreeGrid`
      with `level`, `expanded`, `size_of_set`, `position_in_set`. Paint
      `neue_paint::materials::{disclosure_rect, disclosure_triangle, outline_text_inset}`,
      tokens `neue_theme::materials::table::{OUTLINE_INDENT, DISCLOSURE_*}` and
      `Palette::disclosure`. Gallery: an outline on the "Tables" page; 24 goldens (closed,
      closed disabled, one level, two levels, selected, selected focused × light/dark ×
      1×/2×; Linux aarch64 and x86_64 not run); 8 end-to-end and 7 tree tests. Steps for
      the VM and macOS in `docs/checks/phase-2.md` §12, not run. Not yet: animated
      expansion, lazy loading placeholders, drag and drop, `on_expand` notifications.
- [x] **2.19c Source-list sidebar** · Sonnet (on 2.19b). An outline style: group headers
      (small caps, Show/Hide on hover), rows with icon, label and optional count badge,
      the sidebar background, the source-list selection gradient (focused and unfocused,
      light and dark). Paint materials `sidebar`, `source_list_selection` and tokens.
      *Accept:* gallery page "Sidebar"; goldens (states × light/dark × 1×/2×); keyboard
      and AccessKit roles.
      *Done:* ADR 0050. `neue_kit::{SourceList<M, Id>, Entry, SourceIcon}`: `SourceList::new(
      children, entry)` over an `Outline` in the sidebar style; the top level is the groups
      (small capitals, not selectable, Show or Hide under the pointer), under them items
      with an `Entry`'s icon, label and count badge, which can nest. `expanded`,
      `collapsed`, `selected(id)`, `on_select(Option<&Id>)`, `on_activate(&Id)`;
      `Handle::{reload, set_expanded, set_selection(Option<Id>), scroll_to, set_enabled}`.
      One selection; Up, Down, Home, End, type-select skip the headers. `Table::build(..,
      source_list)`, `Frame::Sidebar`, `Cell::source`, `SourceContent` (draws the row).
      Paint `neue_paint::materials::{sidebar, source_list_selection, source_badge,
      source_icon, SourceIcon, SourceState, source_item, ...}`; tokens
      `neue_theme::materials::sidebar`. Gallery page "Sidebar" (page 9); 32 goldens (source
      list normal, group hovered, disabled, selected, selected focused, nested, nested
      focused, collapsed × light/dark × 1×/2×; macOS only, Linux aarch64 and x86_64 not
      run); 11 end-to-end and 4 geometry tests. Steps for the VM and macOS in
      `docs/checks/phase-2.md` §13, not run. Not yet: label truncation, drag and drop and
      rename of items, keyboard collapse of groups, multiple selection, a general icon type
      (2.20).
      Deferred from 2.19: variable row heights, editable cells, column reordering, row drag
      and drop, auto-size on divider double-click, row context menus (beyond wrapping a
      cell in `ContextMenu`).
- [x] **2.20 Icon and collection view** · Sonnet. Split into 2.20a to 2.20c, all done.
  - [x] **2.20a Icon type** · Sonnet. A general `Icon` in `neue-paint` and `neue-kit`: the
        named glyph set (today's `SourceIcon`, renamed `Glyph`) plus a vector case, SVG path
        data parsed once into vello paths (no new dependency), with paints that follow the
        text colour or have fixed light and dark values. `SourceList`'s `Entry::icon` takes
        an `Icon`. A kit `Icon` widget draws one at a size.
        *Accept:* the source-list goldens don't change; goldens for the glyphs and a vector
        icon (light/dark × 1×/2×, disabled); an AccessKit `Image` role with the icon's label;
        a gallery page "Icons"; an ADR that supersedes 0050 §6.
        *Done:* ADR 0051. `neue_paint::icon::{Icon, Glyph, VectorIcon, IconPaint, IconError,
        icon}` (re-exported from `neue_kit`): `Glyph` is the old `SourceIcon`; a vector icon
        is filled shapes from SVG path data (`BezPath::from_svg`, no new dependency) with
        `IconPaint::Current { alpha }` or `Fixed { light, dark }`; `icon(scene, rect, &Icon,
        color, variant)` fits it in a rectangle. `neue_kit::IconImage` (`new`, `size`,
        `label`; `Handle::set_icon`) is an AccessKit `Image` named by its label or a glyph's
        name; a disabled vector icon is drawn at half opacity. `Entry::icon` takes
        `impl Into<Icon>`; `SourceIcon` and `source_icon` are gone. Gallery page "Icons"
        (page 10); 16 new goldens (glyphs and the vector icon × normal, disabled ×
        light/dark × 1×/2×; macOS only, Linux aarch64 and x86_64 not run), the source-list
        goldens unchanged; 5 paint and 3 end-to-end tests. Steps for the VM and macOS in
        `docs/checks/phase-2.md` §14, not run. Not yet: icon themes and lookup by name,
        raster images, strokes and gradients in vector icons.
  - [x] **2.20b Grid virtualisation** · Sonnet. `VirtualGrid`: fixed item size, columns from
        the width, only the items near the view are built, scroll to an item, rewiring as in
        ADR 0043.
        *Accept:* tests for the column count on resize and for which items exist; a gallery
        entry; an addendum to ADR 0043.
        *Done:* addendum to ADR 0043. `neue_kit::VirtualGrid::new(count, cell_width,
        cell_height, |i| item)` (`width`, `height`, `bind`; `Handle::{set_count, reload,
        scroll_to_item}`) is `RowsWidget` with a cell width: columns are `floor(width /
        cell_width)` (at least one), set from the scroll view's window; whole rows of items
        near the view exist; items are centred in cells of `width / columns`. A list is the
        one-column case, so its behaviour and the table's are unchanged. 3 unit tests (column
        count, wanted range, row spans) and 6 end-to-end tests (columns and built items,
        item positions, resize, scrolling, set-count/reload/scroll-to-item, handlers of
        dropped items). Gallery "Scrolling" page: "Virtual grid (1,000,000 items)". No
        goldens (2.20c adds them). Steps for the VM in `docs/checks/phase-2.md` §15, not
        run. Not yet: AccessKit grid roles, gaps between cells, variable cell sizes,
        horizontal scrolling.
  - [x] **2.20c Collection view** · Sonnet (on 2.20a and 2.20b). `Collection<M, Id>` on the
        grid: items as an icon over a label, an icon-size option, single or multiple
        selection (click, Shift-click, Cmd/Ctrl-click), key and non-key colours, 2D arrow
        keys, Home/End, type-select, Return or double-click activates. AccessKit
        `Grid`/`GridCell` with row and column counts.
        *Accept:* gallery entries on "Icons"; goldens (normal, selected, selected unfocused,
        disabled × light/dark × 1×/2×); end-to-end tests for selection and keys; ADR.
        Not in scope: drag and drop, rename, variable item sizes, rubber-band selection,
        sections.
        *Done:* ADR 0052. `neue_kit::Collection<M, Id>` (`new(ids, |&id| Item)`,
        `icon_size`, `multiple`, `selected`, `on_select(&[Id])`, `on_activate(&Id)`, `width`,
        `height`, `disabled`, `bind`; `Handle::{set_items, reload, set_selection, scroll_to,
        set_icon_size, set_enabled}`) and `Item::new(label).icon(icon)`: a collection body
        on `RowsWidget` beside the table's, items that draw their icon, selection backdrop
        and label pill, a long label cut with an ellipsis. Cell size from
        `neue_theme::materials::collection` (also the palette of the icon backdrop). Click,
        Shift and Cmd/Ctrl select (in item order), 2D arrows, Home/End, Page keys, Cmd-A,
        type-select, Return and double-click activate; AccessKit `Grid` with row and
        column counts and `GridCell` items. 10 end-to-end tests, 1 theme test; 2 new
        golden sets (`collection`, `collection-selected` × states × light/dark × 1×/2×,
        16 images; macOS only, Linux aarch64 and x86_64 not run). Gallery "Icons" page: two
        collections (1,000 items at 32 px, 12 at 64 px) with status messages. Steps for the
        VM in `docs/checks/phase-2.md` §16, not run. Not yet: the items listed under "Not
        in scope", gaps between cells, a borderless style, and the VoiceOver check on macOS.
- [x] **2.21 Split view and tab view** · Sonnet. Split into 2.21a and 2.21b.
  - [x] **2.21a Split view** · Sonnet. `SplitView`: panes side by side or stacked, thin
        and thick (grip) dividers, per-pane minimum and maximum, a pane that holds its size
        on resize, collapsing (drag, double-click, Return, handle), keyboard and AccessKit
        `Splitter`.
        *Accept:* tests for drag, limits, collapse, resize and keys; goldens (thin and
        thick × normal, focused, disabled; stacked; collapsed) × light/dark × 1×/2×; a
        gallery page; an ADR.
        Not in scope: saved positions, animated collapse, nested-split snapping.
        *Done:* ADR 0053. `neue_kit::{SplitView, Pane, DividerStyle}` (`SplitView::{
        horizontal, vertical}`, `with`, `with_pane`, `divider`, `on_resize(&[f64])`,
        `width`, `height`, `bind`; `Handle::{set_position, set_collapsed}`; `Pane::{size,
        min, max, holds_size, collapsible, collapsed}`). A divider is a gap in
        `SplitWidget`'s layout that it draws and hit-tests; `fit` gives a resize to the
        panes that don't hold their size; `drag_pair` clamps a divider's pair and collapses
        a collapsible pane below half its minimum. Home and End move the divider to either
        end (collapsing a collapsible pane there); the plan's "Home/End collapse" became
        that, as in the WAI-ARIA splitter. Material `neue_theme::materials::split`,
        `neue_paint::materials::split_divider`. 4 unit tests and 10 end-to-end tests; 36
        goldens (macOS only, Linux aarch64 and x86_64 not run); gallery page "Panes"
        (page 11). Steps for the VM in `docs/checks/phase-2.md` §17, not run. Not yet: the
        items under "Not in scope", a wider grab area for thin dividers, pushing past a
        pane into the next one.
  - [x] **2.21b Tab view** · Sonnet. `TabView`: a strip of tabs over a bordered box (the
        `NSTabView` look), all tab contents built up front and only the selected one
        shown, its own strip widget (not `Segmented`). Left/Right move between tabs,
        Ctrl-Tab and Ctrl-Shift-Tab cycle; AccessKit `TabList`, `Tab`, `TabPanel`;
        `Handle::{set_selected, set_tab_enabled}`.
        *Accept:* tests for selection, keys and a disabled tab; goldens (normal, selected,
        disabled, unfocused × light/dark × 1×/2×); a gallery page; an ADR.
        Not in scope: closable or reorderable tabs, tabs on other edges.
        *Done:* ADR 0054. `neue_kit::TabView` (`new`, `with_tab`, `with_disabled_tab`,
        `selected`, `on_select(usize)`, `width`, `height`, `disabled`, `bind`;
        `Handle::{set_selected, set_tab_enabled, set_enabled}`). One widget owns the
        strip, box and panels; panels other than the selected one are stashed. Left and
        Right (and Home and End) on the focused strip skip disabled tabs; Ctrl-Tab and
        Ctrl-Shift-Tab wrap, from anywhere inside, and focus the strip. The tab list and
        tabs are synthetic AccessKit nodes (read, but not clickable through AT; the view
        takes `Increment` and `Decrement`). Material `neue_theme::materials::tabs`,
        `neue_paint::materials::{tab_strip, tab_box}`; the selected tab is neutral while
        the window is inactive. 7 end-to-end tests; 20 goldens (macOS only, Linux aarch64
        and x86_64 not run); gallery: a "Tab view" section on the "Panes" page. Steps for
        the VM in `docs/checks/phase-2.md` §18, not run. Not yet: the items under "Not in
        scope", a strip wider than the view, clickable tabs for assistive technology.
- [ ] **2.22 Window chrome and unified toolbar** · Opus. Split into 2.22a to 2.22d. Kit
      windows draw their own chrome (client-side), from neue-paint's title-bar pieces
      (PLAN §3.1); the ADR in 2.22a records why not a private protocol that extends the
      server-side bar (ADR 0013). Order: a → b → c; d any time before 2.22b's VM check.
      Not in scope: toolbar overflow (chevron), toolbar customisation, reading
      `title-bar-double-click` from clients (kit windows zoom), shadows for client-side
      windows (they come with the compositor's shadow shader).
  - [x] **2.22a Unified chrome material and ADR** · Opus. `neue_paint::materials::
        window_chrome`: a bar of any height (title bar plus toolbar) with the title-bar
        gradient, top highlight, bottom separator, rounded top corners, traffic lights and
        title, light/dark × focused/unfocused; tokens in `neue_theme::materials`. The
        server-side bar is its 22 px case.
        *Accept:* the `titlebar-*` goldens unchanged bit-for-bit; new goldens (22 and 52 px
        × focused/unfocused × light/dark × 1×/2×); neue-comp builds; ADR.
        *Done:* ADR 0055 (client-side chrome for kit windows; the private-protocol
        alternative rejected). `neue_paint::materials::{window_chrome,
        window_chrome_titled}` and `title_bar::MIN_CHROME_HEIGHT`: one gradient over the
        whole height, lights and title in the top 22 px row; the bar's private
        `background` takes a height, so the slices are unchanged. No new theme tokens
        (the bar's palette and metrics serve both). `titlebar-*` goldens unchanged; a unit
        test that 22 px chrome matches `title_bar`; 16 goldens `chrome-{22,52}-…` (macOS;
        Linux not run). neue-comp checked on Linux aarch64.
  - [ ] **2.22b Kit window chrome** · Opus. `Window::toolbar(...)` wraps the root in a
        chrome widget: the bar, traffic lights (glyphs on group hover, pressed, close /
        minimise / zoom), the title, drag to move, double-click to zoom, the inactive look
        when the window loses focus. The Wayland driver asks for client-side decorations
        (xdg-decoration) for windows with chrome; winit on macOS uses a transparent
        full-size title bar with the native buttons hidden. AccessKit labels on the buttons.
        *Accept:* end-to-end tests (button actions, drag signal, double-click, inactive
        look); goldens; the gallery window uses it; VM steps in `docs/checks/phase-2.md`.
  - [ ] **2.22c Toolbar items** · Sonnet. `Toolbar` items: icon-and-label buttons,
        segmented control, search field, flexible and fixed space, separator; display modes
        (icon and text, icon only, text only); keyboard and the AccessKit `Toolbar` role.
        *Accept:* tests for layout and keys; goldens; a gallery page.
  - [ ] **2.22d neue-comp: resize edges for client-side windows** · Sonnet.
        `State::chrome_hit_at` gives the 5 px resize edges to every resizable toplevel, not
        only to windows with a server-side bar.
        *Accept:* a headless test: a client-side window resizes from its edge; VM check.
- [ ] **2.23 Sheet, popover, HUD panel** · Opus. Sheets map to xdg-dialog; popovers are
      xdg-popups with an arrow.
- [ ] **2.24 Colour well, date picker, path control** · Sonnet.
- [ ] **2.25 `docs/design/` specs** · Sonnet. Token reference, materials spec, motion
      spec.
- [ ] **2.26 Exit check** · Sonnet, plus owner VM steps. Screenshot every gallery page
      with `neuectl` in the VM, compare with macOS and the goldens, fix drift, tick
      Phase 2.
- [ ] **2.27 neue-comp: scale for layer surfaces** · Sonnet. Found in 2.2:
      `State::refresh_surface_scales` (`compositor/neue-comp/src/outputs.rs`) sends
      `wp_fractional_scale_v1.preferred_scale` and `wl_surface.preferred_buffer_scale`
      to windows only, so layer surfaces stay at scale 1 and are upscaled (blurry) on a
      1.5× output. Also walk every output's `layer_map_for_output(output).layers()` and
      call `send_scale`'s body for each `LayerSurface` (with `with_surfaces`, so its
      popups and subsurfaces get it too) using that output's scale. Keep "only changes are
      sent". Add a layer-surface variant of the test client's preferred-scale tracking
      (`tests/harness/client.rs`, now toplevel-only).
      *Accept:* a headless test in `tests/outputs.rs` or `tests/shell_protocols.rs`: a
      layer surface on a 1.5 output gets `preferred_scale` 180, and 240 after a config
      reload to 2; in the VM, `docs/checks/phase-2.md` §3 step 6 shows the demo's
      layer-shell strip sharp. Order: any time, but before 2.5's VM check.
- [ ] **2.28 Primary selection from text fields** · Sonnet. Found in 2.14: the Wayland
      driver pastes the primary selection but never owns it. Add a platform call to set
      the primary selection (`Ctx`, no-op on winit), a runner hook, and have
      `TextField` report its selected text when a pointer or Shift selection ends (not in
      secure fields). ADR amendment to 0034.
      *Accept:* in the VM, selecting text in the gallery's field and middle-clicking in
      foot pastes it.
- [ ] **2.29 macOS: one see-through, garbled frame** · Opus (hard debugging). Found in
      2.17a, on the macOS host (winit driver, Metal). Now and then, for a single frame,
      the whole gallery window is drawn see-through (the desktop shows through it), and
      its top-left corner (the first sidebar buttons) fills with garbled pixels that
      look like menu rows. Seen at 60 fps while hovering menu items: when the highlight
      moved from Cut to Copy, and when Text Size opened its submenu. The next frame is fine.
      *Ruled out:* rendering the same frames offscreen does not reproduce it. Over 200
      frames at 2× (a window of buttons with the menu open, hover cycling through items),
      neither the headless vello_hybrid renderer nor `texture_render::Renderer` into an
      `Rgba8Unorm` texture on Metal produced a bad frame. That test skipped only the
      surface steps (`get_current_texture`, the blit and `present` in
      `neue-platform/src/gpu.rs`).
      *Hypotheses:* (1) the surface path: a frame presented before it was fully rendered,
      or a surface reconfigure mid-frame (`Outdated`/`Lost` → `configure` in `Gpu::render`),
      shows leftover pixels with alpha below 1; (2) a vello_hybrid bug with clip layers.
      The menu panel's shadow uses `push_clip`, and the Aqua buttons use layers too.
      *Steps:*
      1. Log while reproducing: `RUST_LOG=neue_platform=debug cargo xtask gallery 2>&1 |
         tee /tmp/gallery.log`, then open "Menus", right-click, and sweep the pointer over
         the items for a minute. Then run `grep -E "WARN|ERROR|skipping|reconfigure|failed"
         /tmp/gallery.log`. Lines like "no surface texture", "skipping frame" or
         "rendering Masonry content failed" point to (1).
      2. Is it specific to menus? Sweep and click quickly over the Push Button and
         Buttons pages, and scroll on Scrolling, for a minute each with a screen
         recording. A bad frame without a menu means the cause is the winit/gpu path,
         not the menu paint.
      3. Note the display: built-in ProMotion, external, or scaled. Check whether
         dragging the window to the other display changes how often it happens.
      4. If (1): add a `debug!` with the frame number and timing around
         `get_current_texture`, `render_to_texture`, the blit and `present`. Check
         whether the bad frame matches a reconfigure or a `Suboptimal` texture. Fixes to
         try: skip presenting after a reconfigure in the same frame, or render straight
         to the surface texture when its format allows. Compare with `masonry_winit`'s
         render loop.
      5. If (2): draw the menu shadow without the clip (the panel is opaque until
         Phase 3) and see whether it goes away. If it does, make a minimal vello_hybrid
         repro (base scene with layers + an overlay with a clip) and report it upstream,
         noting it in ADR 0002's vello_gpu caveats.
      *Accept:* the cause is found and fixed (or worked around, with an upstream issue).
      A one-minute screen recording of menu hovering on macOS has no bad frame (check at
      60 fps, e.g. `ffmpeg -vf signalstats` and look for dips in mean brightness).
- **Exit:** the gallery shows the whole widget set, in light and dark, identically on
  macOS and in the VM.
