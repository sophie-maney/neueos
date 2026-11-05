# session

Everything that starts the neueOS user session (task 1.19, ADR 0027).

| Path | Installed to |
|---|---|
| `neue-session/` | `/usr/bin/neue-session` (what greetd runs) |
| `systemd/user/*` | `/usr/lib/systemd/user/` |
| `wayland-sessions/neueos.desktop` | `/usr/share/wayland-sessions/` |
| `greetd/config.toml` | `/etc/greetd/config.toml` (VM and early installs; the graphical greeter replaces it) |

Flow: greetd → `neue-session` → `systemctl --user start --wait neue-session.target` →
`neue-comp.service` (`neue-comp --tty --notify --shell /usr/bin/neue-shell`; the compositor
starts the shell and restarts it if it exits, ADR 0066) and `graphical-session.target`. Logout is
the compositor exiting (Super+Q, `neuectl quit` or `neue-session stop`).
