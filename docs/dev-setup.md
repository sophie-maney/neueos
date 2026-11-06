# Dev setup

How neueOS is built, run and checked from the Mac. §1 lists everyday commands, §2 sets the
rules for writing manual check steps, and §3 sets up the machines from scratch (done once).
Task-specific checks live in `docs/checks/phase-N.md`.

Run every Mac-side command from the repo root. All `ssh` commands are single-quoted, so they
work the same from fish, zsh or bash. The VM and the PC run bash.

---

## 1. Cheat sheet

### 1.1 Machines
| Name | What | Arch | Used for | Reach it with |
|---|---|---|---|---|
| `neueos-arm64` | OrbStack | aarch64 | Building and testing Linux-only crates | `cargo xtask linux <cmd>` |
| `neueos-amd64` | OrbStack (Rosetta) | x86_64 | Same, to catch x86_64-only breakage | `cargo xtask linux-amd64 <cmd>` |
| `neueos-vm` | Parallels | aarch64 | Full graphical session (DRM, greetd) | `ssh neueos-vm '<cmd>'` |
| `neueos-nvidia` | PC, RTX 2080 | x86_64 | Real-hardware checks | `ssh neueos-nvidia '<cmd>'` |

- OrbStack mounts the Mac filesystem at the same path, so `cargo xtask linux` builds the repo
  in place, into `target/<machine>/`. OrbStack has no GPU or DRM: never run the compositor
  there.
- The VM and the PC get the repo by `rsync` (§1.3) or packages (§1.5). Their copy is not a
  git checkout.
- The VM boots into greetd (text login on tty1), which starts `neue-session` from the
  installed packages.
- Look at the VM's screen from the Mac with `prlctl capture neueos-vm --file /tmp/shot.png`.
  It can return a partial frame while something animates.

### 1.2 Build and test on the Mac
```sh
cargo xtask check                          # fmt + clippy + tests, macOS
cargo xtask linux cargo xtask check        # same, Linux aarch64 (includes Linux-only crates)
cargo xtask linux-amd64 cargo xtask check  # same, Linux x86_64 (slow, Rosetta)
cargo xtask linux cargo test -p neue-comp <name>   # one test on Linux
```

### 1.3 Run a dev build in the VM
This is the fast loop: sync, build in the VM, then start the compositor on the VM console from ssh.

**1. Stop whatever holds the console** (greetd and any running session). `-t` lets sudo ask
for the password:
```sh
ssh -t neueos-vm 'sudo systemctl stop greetd; pkill -x weston; true'
```

**2. Sync and build:**
```sh
rsync -az --delete --exclude target --exclude .git ./ neueos-vm:neueos/
ssh neueos-vm 'cd neueos && cargo build --release -p neue-comp -p neuectl -p neue-shell --no-default-features --features neue-comp/tty'
```

**3. Start the compositor and check that it listens:**
```sh
ssh neueos-vm 'export XDG_RUNTIME_DIR=/run/user/1000 RUST_LOG=info,neue_comp=debug; unset WAYLAND_DISPLAY
  setsid nohup ~/neueos/target/release/neue-comp --tty --shell ~/neueos/target/release/neue-shell -c foot >/tmp/comp.log 2>&1 </dev/null &
  sleep 3; . ~/neueos/docs/vm-env.sh; neuectl windows'
```
Expected: one `foot` window. If not, `ssh neueos-vm 'tail -30 /tmp/comp.log'`: the usual
causes are seatd and `/dev/dri` (§3.2). `-c <cmd>` starts a client (repeatable). Super+Q
quits. `--shell` runs the menu bar (ADR 0066, 0068); leave it out for a bare compositor.

**4. When done,** quit and give the console back to greetd:
```sh
ssh -t neueos-vm '. ~/neueos/docs/vm-env.sh; neuectl quit; sudo pkill -x ydotoold; sudo systemctl start greetd; true'
```

The nested backend (neue-comp in a weston window) is only needed for winit-backend work:
replace step 3's command with `weston --backend=drm --idle-time=0 --socket=wl-stock &` and
`WAYLAND_DISPLAY=wl-stock neue-comp -c foot` (built with default features).

### 1.4 Drive the running session over ssh
`docs/vm-env.sh` sets `XDG_RUNTIME_DIR`, `WAYLAND_DISPLAY=wayland-1` and puts
`~/neueos/target/release` first on `PATH`. Source it in every command that talks to the
session:
```sh
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; neuectl windows; neuectl outputs; neuectl focused-window'
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; neuectl actions'                  # list actions
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; neuectl action zoom-window --window 2'
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; timeout 20 neuectl events'       # JSON lines
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; systemd-run --user --collect foot'   # start an app
```

**Screenshots** (copy them to the Mac to look at them):
```sh
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; neuectl screenshot /tmp/a.png'
mkdir -p ~/Desktop/neueos-shots && scp 'neueos-vm:/tmp/*.png' ~/Desktop/neueos-shots/ && open ~/Desktop/neueos-shots
```

**Scripted input.** Start ydotool's daemon once per boot (the user is in the `input`
group, which may write `/dev/uinput`, so no root is needed), then move, click and type:
```sh
ssh neueos-vm 'setsid nohup ydotoold --socket-path=/tmp/.ydotool_socket >/dev/null 2>&1 </dev/null &'
ssh neueos-vm 'export YDOTOOL_SOCKET=/tmp/.ydotool_socket; ydotool mousemove -x 40 -y 40; ydotool click 0xC0; ydotool type "echo ok"; ydotool key 28:1 28:0'
prlctl send-key-event neueos-vm -s 29 -e press     # key chords: press each (Ctrl=29), then release
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; click_in_window 100 50'   # click at (100,50) in the first window (scale 1)
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; sweep menu 30; sweep popover 30'   # count see-through frames while hovering popups (task 2.30)
ssh neueos-vm 'export YDOTOOL_SOCKET=/tmp/.ydotool_socket; ydotool key 115:1 115:0'  # Volume Up (114 down, 113 mute, 225/224 brightness); hold with 114:1; sleep 1; 114:0
```

**Logs:**
```sh
ssh neueos-vm 'tail -50 /tmp/comp.log'                                    # dev build (§1.3)
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; journalctl --user -u neue-comp -b | tail -50'   # greetd session
```

### 1.5 Packages
Build on the Mac, then install in the VM (aarch64) or on the PC (x86_64):
```sh
cargo xtask package neue-comp --arch aarch64     # or x86_64, or all
scp target/package/aarch64/*.pkg.tar.* neueos-vm:/tmp/
ssh -t neueos-vm 'sudo pacman -U --noconfirm /tmp/neue*.pkg.tar.* && rm /tmp/neue*.pkg.tar.*'
```
Then log in at the VM console (greetd) to start the installed session. On the PC, use
`x86_64` and `neueos-nvidia`. `pacman -U` refuses files a dev install put in `/usr/bin` by
hand: add `--overwrite '*'`.

### 1.6 Compositor config
`~/.config/neueos/compositor.toml` on the VM. It reloads on save. Errors show up as
`config_error` in `neuectl events` and the old config stays. Schema and defaults:
`crates/neue-config`.
```sh
ssh neueos-vm 'cat ~/.config/neueos/compositor.toml'
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; neuectl reload-config'
```

### 1.6a Settings service
`neue-settingsd` (ADR 0074) on the VM's session bus; build it with the step 2 command plus
`-p neue-settingsd`. Keys are `<file>.<dotted path>` (`appearance.mode`,
`compositor.pointer.natural-scroll`).
```sh
ssh neueos-vm 'export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus
  setsid nohup ~/neueos/target/release/neue-settingsd >/tmp/settingsd.log 2>&1 </dev/null &
  sleep 1; busctl --user call org.neueos.Settings /org/neueos/Settings org.neueos.Settings1 Set sv appearance.mode s dark
  gsettings get org.gnome.desktop.interface color-scheme'
```
`Get s <key>`, `GetAll`, `Reset s <key>`; `busctl --user monitor org.neueos.Settings` shows
`Changed`.

### 1.6b Mock UPower and BlueZ (battery and Bluetooth extras)
The VM has no battery or Bluetooth. `neue-dbus`'s `mock_power_bluetooth` example serves both
on the **system** bus (a battery at 64 %, a powered adapter, devices Earbuds, Keyboard and
Broken whose `Connect` fails) and prints each call it gets. The bus refuses the names to
everyone but the real services, so give root a policy for it first, and stop the real
`upower` (the shell would otherwise start it by activation). Start the mock before the
session; the extras appear within 5 s of it starting and disappear within 5 s of it stopping.
```sh
ssh -t neueos-vm 'printf "%s\n" "<busconfig><policy user=\"root\"><allow own=\"org.bluez\"/><allow own=\"org.freedesktop.UPower\"/></policy><policy context=\"default\"><allow send_destination=\"org.bluez\"/><allow send_destination=\"org.freedesktop.UPower\"/></policy></busconfig>" | sudo tee /usr/share/dbus-1/system.d/neue-mock.conf >/dev/null; sudo systemctl reload dbus; sudo systemctl stop upower'
ssh neueos-vm 'cd neueos && cargo build -p neue-dbus --example mock_power_bluetooth'
ssh -t neueos-vm 'sudo sh -c "setsid nohup ~sophie/neueos/target/debug/examples/mock_power_bluetooth >/tmp/mock.log 2>&1 </dev/null &"'
ssh neueos-vm 'cat /tmp/mock.log'                                  # the calls the menus made
ssh -t neueos-vm 'sudo pkill -x mock_power_blue; sudo rm -f /usr/share/dbus-1/system.d/neue-mock.conf; sudo systemctl reload dbus'   # when done
```
`--battery 20 --charging` set the battery.

### 1.6c Mock NetworkManager (network extra)
The VM has no Wi-Fi. `neue-dbus`'s `mock_network_manager` example serves a
wired device (connected; `Disconnect` and `ActivateConnection` toggle it) and a Wi-Fi device
with Home (saved, connected), Cafe (open) and Library (protected, not saved) on the
**system** bus, and prints each call it gets. Stop the real NetworkManager and give root a
policy for the name first.
```sh
ssh -t neueos-vm 'printf "%s\n" "<busconfig><policy user=\"root\"><allow own=\"org.freedesktop.NetworkManager\"/></policy><policy context=\"default\"><allow send_destination=\"org.freedesktop.NetworkManager\"/></policy></busconfig>" | sudo tee /usr/share/dbus-1/system.d/neue-mock-nm.conf >/dev/null; sudo systemctl reload dbus; sudo systemctl stop NetworkManager'
ssh neueos-vm 'cd neueos && cargo build -p neue-dbus --example mock_network_manager'
ssh -t neueos-vm 'sudo sh -c "setsid nohup ~sophie/neueos/target/debug/examples/mock_network_manager >/tmp/mock-nm.log 2>&1 </dev/null &"'
ssh neueos-vm 'cat /tmp/mock-nm.log'                               # the calls the menu made
ssh -t neueos-vm 'sudo pkill -x mock_network_m; sudo rm -f /usr/share/dbus-1/system.d/neue-mock-nm.conf; sudo systemctl reload dbus; sudo systemctl start NetworkManager'   # when done
```
The shell follows NetworkManager within 5 s of the mock starting or stopping; open the
network menu (the extra left of Bluetooth) to scan and list the networks.
Without a private system bus the example can also be tried in OrbStack:
`orb run -m neueos-arm64 dbus-run-session -- sh -c 'DBUS_SYSTEM_BUS_ADDRESS=$DBUS_SESSION_BUS_ADDRESS target/neueos-arm64/debug/examples/mock_network_manager'`
and `busctl --user …` against `org.freedesktop.NetworkManager`.

### 1.6d Tray apps: the watcher, the host and a mock item (status-icon tray)
The shell owns the StatusNotifierWatcher on the **session** bus (3.13c) and shows the
items as icons left of the other extras. Without a shell, `neue-dbus`'s `sni_host` example
owns it instead and prints what a tray would show (items added, changed and removed, with
icon, status, tooltip and menu path; stop the shell first). `mock_sni_item` is a stand-in
tray app (a drawn blue icon, a tooltip, a menu of Open, Mark Attention, a check item and
Quit; Mark Attention turns the icon red and back; `--named` gives the icon by theme name)
that prints the calls it gets. Run them in the VM's session (over ssh, with the session's
bus address) or in OrbStack under `dbus-run-session`.
```sh
orb run -m neueos-arm64 dbus-run-session -- sh -c 'target/neueos-arm64/debug/examples/sni_host & sleep 1; target/neueos-arm64/debug/examples/mock_sni_item'
# the shell's worker itself, against the mock, in OrbStack (prints what the bar would get):
cargo xtask linux dbus-run-session -- sh -c 'target/neueos-arm64/debug/examples/mock_sni_item & sleep 1; cargo test -p neue-shell the_worker_shows -- --ignored --nocapture'
```
In the VM (compositor and shell running as in §1.3):
```sh
ssh neueos-vm 'cd neueos && cargo build --release -p neue-dbus --examples'
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; setsid nohup ~/neueos/target/release/examples/mock_sni_item >/tmp/mock-sni.log 2>&1 </dev/null &'
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; neuectl screenshot /tmp/tray.png'   # a blue disc left of the network extra
# left click it: the log shows `Activate`; right click: its menu opens, choose Mark Attention
# (log `clicked 3`, the disc turns red) and Quit (the icon goes away)
ssh neueos-vm 'cat /tmp/mock-sni.log'
```
A real tray app (`nm-applet --indicator`) can replace the mock. Menu clicks can be sent
with `busctl --user call <service> /StatusNotifierItem/Menu com.canonical.dbusmenu Event
isvu 3 clicked i 0 0`.

### 1.6e Audio: test devices and the volume keys
The VM's sound card belongs to the seat, which greetd's greeter holds (§1.3 starts the
compositor beside it), so PipeWire sees no devices. Give it two null sinks and a source,
once (no root needed): copy `docs/vm-audio.conf` into PipeWire's config folder.
```sh
ssh neueos-vm 'mkdir -p ~/.config/pipewire/pipewire.conf.d && cp ~/neueos/docs/vm-audio.conf ~/.config/pipewire/pipewire.conf.d/neue-test-devices.conf && systemctl --user restart pipewire wireplumber'
```
Building `neue-audio` needs `clang` (bindgen). If the VM lacks it, build in OrbStack and
copy the binaries (same architecture):
```sh
cargo xtask linux cargo build --release -p neue-comp -p neuectl -p neue-shell --no-default-features --features neue-comp/tty
scp target/neueos-arm64/release/{neue-comp,neuectl,neue-shell} neueos-vm:neueos/target/release/
```
Check the volume keys (§1.3 session running, `ydotoold` as in §1.4; key codes 115 up,
114 down, 113 mute, 248 mic mute):
```sh
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; export YDOTOOL_SOCKET=/tmp/.ydotool_socket
  ydotool key 115:1 115:0; sleep 0.3; wpctl get-volume @DEFAULT_AUDIO_SINK@; wpctl status | sed -n "/Sinks/,/Filters/p"'
```
`neue-audio`'s example drives the backend without the shell: `cargo run -p neue-audio
--example audio -- volume 0.5` (also `mute`, `unmute`, `mic-mute`, `default <node.name>`).

### 1.7 Known quirks
- `ydotool mousemove --absolute` lands at twice the coordinates. Relative moves are 1:1.
- Nested (winit) backend: after the pointer jumps into the window, send one relative move
  before clicking.
- virtio-gpu logs "Failed to destroy old mode property blob" at start-up and "Failed to
  restore previous state" at exit. Both are harmless.
- `cargo xtask package --arch x86_64` may print `libfakeroot internal error: payload not
  recognized!` (fakeroot under Rosetta). It is harmless if `bsdtar -tvf <pkg>` shows only
  `root root` owners.
- Use `pkill -x <name>` over ssh: `pkill -f` matches the remote shell's own command line and kills it.

---

## 2. Writing manual check steps (for agents)
The owner runs these by hand, often many tasks in one sitting, so every step must be
executable as written, without opening another document.

**Where.** A task's check goes into `docs/checks/phase-N.md` as `## <n>. Checking <what>
(task X.Y)`. When a phase has several unrun checks, collect them into one runbook,
`docs/checks/phase-N-owner.md` (model: `docs/checks/phase-2-owner.md`): one setup part,
then one part per gallery page or app, then all macOS steps together, then a "Report
back" list. The task's own section then points to the runbook's step numbers.

**Steps.**
- Number every step (`5.3`) and give it one action and an **Expected:** that says exactly
  what output or picture means pass. Never "check that it works".
- Mark each step **Run** (a command block, pasted into the Mac terminal from the repo
  root) or **Do** (by hand in the Parallels window or on the Mac).
- **Self-contained:** no "as §8 step 1", no "positions from `t1.png`", no "the helpers of
  §4". Repeat the command, or put it in `docs/vm-env.sh` as a named helper and call that.
  Within a runbook, "repeat 6.2 to 6.5" is fine.
- Commands are single-quoted `ssh neueos-vm '…'`, working from fish, zsh and bash; sudo
  needs `ssh -t`. Source `docs/vm-env.sh` in every command that talks to the session.
- **Pointer and keyboard input that a person does is done by hand** in the Parallels
  window ("click Pictures", "press Shift-Home"), not with ydotool and screen coordinates.
  Use ydotool only for what a Mac keyboard or trackpad can't send (a middle click), with
  a `sleep` so the owner can place the pointer first. Name the Mac keys (Fn-Left for
  Home, Option for Alt, Ctrl not Cmd).
- Use `neuectl` helpers (`wins`, `child_check`, `win_id`) for anything the eye can't
  judge exactly (geometry, parent, state, log lines).
- Mark steps that need real hardware.
- Say when to report: **STOP AND REPORT** where the owner can't go on, **Note** where an
  observation is needed even on a pass; everything else is collected and sent once at the
  end.

---

## 3. Setting up the machines (once)
Only needed to rebuild a machine. Last done with macOS 27.2, Parallels 27.1.0, OrbStack
2.2.5 and archboot 2026.11.02 (aarch64). Background: `docs/research/arch-arm-parallels.md`.
Machine names must match the xtask defaults or be exported as `NEUE_ORB_ARM64` /
`NEUE_ORB_AMD64`.

### 3.1 OrbStack machines
```sh
orb create arch neueos-arm64
orb create --arch amd64 arch neueos-amd64
for m in neueos-arm64 neueos-amd64; do
  orb run -m $m sudo pacman -Syu --noconfirm --needed base-devel git rustup \
    libxkbcommon wayland libinput seatd mesa libpipewire clang
  orb run -m $m -w "$PWD" rustup toolchain install     # reads rust-toolchain.toml
done
cargo xtask linux cargo xtask check && cargo xtask linux-amd64 cargo check --workspace
```
Each task that links a new C library adds its package to the `pacman` line above. The
headless tests need `libwayland-client` (`wayland`) and Mesa's EGL (`mesa`).

### 3.2 Parallels VM (Arch Linux ARM)
1. Download the archboot aarch64 ISO from `https://release.archboot.com/aarch64/latest/iso/`
   and check it against `b2sum.txt`.
2. Parallels → New → *Install from image*, **Other Linux**, *Customize settings*: name
   `neueos-vm`, 4+ CPUs, 6+ GB RAM, 40+ GB disk, **3D acceleration** on, network *Shared*,
   no shared folders, Secure Boot off.
3. In archboot's `setup`: GPT with a 512 MB EFI partition at `/boot` and ext4 root;
   kernel `linux-aarch64`; hostname `neueos-vm`; user `sophie` in `wheel`, with non-empty
   passwords; bootloader **GRUB_UEFI**. Reboot and disconnect the ISO.
4. On the console, as the user:
   ```sh
   sudo pacman -Syu --needed base-devel git rustup rsync openssh sudo seatd mesa mesa-utils \
       libinput vulkan-swrast vulkan-tools weston wayland-utils foot jq ydotool grim clang \
       pipewire wireplumber
   sudo systemctl enable --now sshd seatd
   sudo usermod -aG seat,video,input "$USER"
   sudo systemctl disable --now kmsconvt@tty1.service      # kmscon holds DRM master
   sudo rm -f /etc/systemd/system/autovt@.service
   sudo systemctl daemon-reload && sudo systemctl enable getty@tty1.service
   ip -4 addr show scope global                            # note the IP for §3.3
   ```
   Log out and in again for the groups. Without the kmscon change, the compositor fails
   with "Could not make device fd drm master".
5. After §3.3: sync and build once (§1.3 step 2, plus `ssh neueos-vm 'cd neueos && rustup
   toolchain install'`), install the packages (§1.5), then
   `ssh -t neueos-vm 'sudo pacman -S --needed greetd greetd-agreety && sudo install -m644 /usr/share/neueos/greetd/config.toml /etc/greetd/config.toml && sudo systemctl enable greetd'`.
   Check: `ssh neueos-vm 'ls /dev/dri'` lists `card0` and `renderD128`.

### 3.3 SSH from the Mac
In `~/.ssh/config` on the Mac (the Parallels Shared IP is stable):
```
Host neueos-vm
    HostName 10.211.55.X
    User sophie
```
```sh
ssh-copy-id neueos-vm && ssh neueos-vm uname -m        # aarch64
```
With the 1Password SSH agent, `ssh-copy-id` can't see the key. Prefix it with
`env SSH_AUTH_SOCK="$HOME/Library/Group Containers/2BUA8C4S2C.com.1password/t/agent.sock"`,
or use `ssh-copy-id -f -i <key>.pub`. With several keys in the agent, add `IdentityFile
<key>.pub` and `IdentitiesOnly yes` to the host entry.

### 3.4 The x86_64 PC
Arch Linux x86_64 via `archinstall` (minimal profile), a user with sudo, `sshd` enabled and a
`Host neueos-nvidia` entry as in §3.3. Drivers: AMD and Intel need `mesa` plus
`vulkan-radeon` or `vulkan-intel`. NVIDIA (Turing or newer) needs the open module with KMS:
```sh
ssh -t neueos-nvidia 'sudo pacman -S --needed nvidia-open nvidia-utils greetd greetd-agreety'
ssh neueos-nvidia 'cat /sys/module/nvidia_drm/parameters/modeset; ls /dev/dri'   # Y after a reboot; card*, renderD128
```
Then install the x86_64 packages and greetd as in §1.5 and §3.2 step 5.
