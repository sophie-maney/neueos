# Architecture decision records

One file per decision: `NNNN-short-title.md`. Write one for every new dependency (especially
C), protocol, IPC change, cross-crate interface, or change to a decision in `PLAN.md`.

Format: **Status** (Proposed, Accepted, Superseded by NNNN), **Date**, **Context**,
**Decision**, **Consequences**, **Alternatives considered**, and optionally **Revisit when**.
Keep each under about two pages; link to `docs/research/` for evidence instead of copying it.
Don't edit an accepted ADR's decision; supersede it with a new one.

| ADR | Title | Status |
|---|---|---|
| [0001](0001-smithay.md) | Smithay as the compositor library | Accepted |
| [0002](0002-toolkit-stack.md) | Toolkit stack: neue-paint on vello_cpu/vello_gpu, Masonry for the widget tree | Accepted |
| [0003](0003-ipc.md) | IPC boundaries and the `neuectl` protocol | Accepted |
| [0004](0004-config-format.md) | Configuration format and reload | Accepted |
| [0005](0005-global-menu.md) | Global-menu strategy | Accepted |
| [0006](0006-compositor-test-harness.md) | Compositor test harness | Accepted |
| [0007](0007-neuectl-wire-details.md) | `neuectl` wire details (window ids, events, errors) | Accepted |
| [0008](0008-floating-window-management.md) | Floating window management rules | Accepted |
| [0009](0009-compositor-config-details.md) | Compositor config details (loader, bindings, watcher, events) | Accepted |
| [0010](0010-outputs-and-scaling.md) | Outputs: layout, hotplug and scaling | Accepted |
| [0011](0011-selections-and-dnd.md) | Clipboard, primary selection, data-control and DnD icons | Accepted |
| [0012](0012-cursors.md) | Cursors: XCursor themes, cursor-shape-v1, resize cursors | Accepted |
| [0013](0013-server-side-title-bars.md) | Server-side title bars and text in `neue-paint` | Accepted |
| [0014](0014-chrome-interaction.md) | Chrome interaction: hit-testing, traffic lights, drag, resize edges | Accepted |
| [0015](0015-shell-facing-protocols.md) | Shell-facing protocols: layer shell, session lock, idle, toplevel list and friends | Accepted |
