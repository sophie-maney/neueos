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
