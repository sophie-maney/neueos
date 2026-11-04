# Phase 1 daily-drive pass (task 1.21)

The Phase 1 exit criterion: *daily-drivable with third-party apps (foot/Alacritty, Firefox,
Flatpaks) on the PC and the VM.* This file records each machine's results. The steps for
the PCs are in `docs/dev-setup.md` §22.

**Status:** the VM and the NVIDIA PC pass. There is no AMD or Intel PC to test on, so that
column was dropped.

## Machines
| Machine | CPU / GPU | Kernel / Mesa or driver | neueOS build | Run by, date |
|---|---|---|---|---|
| VM | Parallels on an M4 Max, virtio-gpu, 1024×768 | 7.3.1-2-aarch64 / Mesa 26.2.6 | packages from `8561a41` (task 1.20), plus this branch's `neue-comp` and unit | agent over ssh, 2026-11-04 |
| NVIDIA PC | i7-9700K, RTX 2080 (TU104), one 2560×1440 HDMI monitor | 7.2.14-arch1-1 / `nvidia-open` 615.78.08, Mesa 26.2.6 | x86_64 packages from this branch (`cargo xtask package`) | owner by hand, 2026-11-04; logs checked over ssh |

## Results
✅ works · ⚠️ works with a bug (linked below) · ❌ broken · — not run

| Check | VM | NVIDIA PC |
|---|---|---|
| greetd login → session (`neue-session`) | ✅ | ✅ |
| foot 1.28 | ✅ | ✅ |
| Alacritty 0.17 | ✅ | ✅ |
| Firefox 159 (Wayland, CSD tab bar) | ✅ | ✅ |
| Qt: Kate 26.08 (server-side title bar) | ✅ | ✅ |
| GTK 4: GNOME Text Editor 51 (CSD, gmenu address) | ✅ | ✅ |
| Flatpak: VS Code 1.139 (`com.visualstudio.code`) | ✅ | ✅ |
| X11: xterm 411 via xwayland-satellite 0.8.3 | ✅ | ✅ |
| X11 Flatpak or game (Steam, Wine) on the PC | n/a | ✅ Steam (Flatpak) |
| Clipboard Wayland ↔ X11, both ways, and primary | ✅ | ✅ |
| Video: mpv 0.41, 720p60 test pattern + audio | ✅ after B1 (1 dropped frame in 8 s) | ✅ mpv and Firefox |
| Resize / zoom / full screen (actions) | ✅ | ✅ |
| Typing, mouse, drag and drop by hand | — (owner) | ✅ |
| HiDPI output, fractional scale | n/a | — (no HiDPI monitor) |
| Multi-monitor, hotplug | n/a | — (one monitor) |
| A day of real use without a crash | — | — (short test session, no crash) |
| Logout returns to greetd, unit ends `success` | ✅ after B2 | ✅ (B6 in the log) |
| Compositor log free of errors | ⚠️ B3 | ⚠️ B3 (B5 fixed) |

The VM checks ran over ssh: apps were started with `systemd-run --user`, as a launcher would,
and checked with `neuectl windows` and `neuectl screenshot`. Input by hand was not tested.

## Bugs
| # | Bug | Where | Status |
|---|---|---|---|
| B1 | Apps that pick their own size opened larger than the screen (mpv opened at 1280×720 on a 1024×768 output, so its title went off screen). The compositor never sent `xdg_toplevel.configure_bounds`. | `neue-comp` | **Fixed in this PR:** the initial configure carries the active work area minus the title bar. Test `wm::new_windows_are_told_the_work_area_size`. |
| B2 | When the compositor quit by itself (Super+Q, `neuectl quit`), systemd still ran `ExecStop=neuectl quit`, which failed, so `neue-comp.service` ended `failed`. | `session/systemd/user/neue-comp.service` | **Fixed in this PR:** `ExecStop=-…` ignores its exit status. |
| B3 | On exit, Smithay logs `ERROR … Failed to restore previous state: Invalid argument`. Seen on virtio-gpu and on NVIDIA. The console comes back anyway. | `neue-comp` tty backend | Open, cosmetic. |
| B4 | Kate shows no global-menu address without a DBusMenu registrar. | — | Expected until Phase 3 (ADR 0025). |
| B5 | NVIDIA PC: Smithay logged `ERROR smithay::wayland::shell::xdg: surface missing from known popups` 11 times in 75 s. Cause: Smithay's `xdg_popup` pre-commit hook stays on the `wl_surface` after the client destroys the `xdg_popup`, and GTK commits that bare surface when it closes a menu. That is valid protocol, so the hook found no popup and logged instead. Nothing was lost (no grab, no real error). | Smithay | **Fixed:** Smithay pinned to a fork commit that skips the hook once the popup is gone (ADR 0029). Test `popups::committing_a_closed_popups_surface_logs_no_error`. Rechecked on the NVIDIA PC: no occurrences in a session with every app's menus used (the old build logged 12 in 90 s). |
| B6 | At logout, `xwayland-satellite` panics with `Failed flushing clientside events: … EPIPE` (`src/server/mod.rs:802`) once the compositor has gone. | xwayland-satellite 0.8.3 | Upstream, harmless (session ending). |

## Not covered
No GPU-heavy load in the VM (games, WebGL). No suspend/resume. No touchpad gestures, because
there is no shell yet. No IME.
