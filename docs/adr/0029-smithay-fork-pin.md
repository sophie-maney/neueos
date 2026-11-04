# 0029: Pin Smithay to a fork commit for the popup commit fix (bug B5)

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
During the daily-drive pass (task 1.21) the NVIDIA PC logged
`ERROR smithay::wayland::shell::xdg: surface missing from known popups` about every
seven seconds. The cause is in Smithay (`c7a26ad`, and still on `main` on 2026-11-04):
the `xdg_popup` pre-commit hook is never removed from the `wl_surface`. When a client
destroys the `xdg_popup` but keeps the `wl_surface` and commits it again, which GTK does
on every menu close, the hook finds the role state reset (no parent) and tries to post
`xdg_surface.not_constructed` on a popup that is gone from `known_popups`, so it logs the
error. That commit is valid protocol. Nothing is lost: there is no grab or real error
behind it. cosmic-comp users see the same message.

The hook's ID isn't exposed and our handlers run before Smithay resets the state, so
`neue-comp` can't work around it cleanly. Filtering the log would also hide real errors.

## Decision
1. Pin the workspace `smithay` dependency to
   `github.com/sophie-maney/smithay` rev `32e0137` (branch `neueos`). That is upstream
   `c7a26ad` plus one commit: the popup pre-commit hook returns early when the surface has
   no live popup in `known_popups`.
2. Keep the fork minimal: only patches we would upstream, each on top of an upstream rev.
   A Smithay bump rebases the `neueos` branch onto the new upstream rev (or drops patches
   that upstream has), in its own PR as ADR 0001 says.
3. No upstream PR for now (owner's call). Once one is merged, pin back to
   `Smithay/smithay`.
4. Test: `tests/popups.rs` closes a popup the way GTK does and fails on any ERROR event.

## Consequences
- The PKGBUILD needs no change: `cargo fetch --locked` fetches the fork like any git dep.
- Any fork rev must be reachable on GitHub, so the `neueos` branch must not be rewritten.
  Rebases go to a new branch name or are added on top.
