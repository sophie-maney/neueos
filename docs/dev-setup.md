# Dev setup

Sets up the three Linux machines that neueOS development needs, on an Apple-silicon Mac.
Background and sources: `docs/research/arch-arm-parallels.md`, PLAN.md §6.

| Machine | Tool | Arch | Used for |
|---|---|---|---|
| `neueos-arm64` | OrbStack | aarch64 | Building and testing Linux-only crates (`cargo xtask linux ...`) |
| `neueos-amd64` | OrbStack (Rosetta) | x86_64 | Same, to catch x86_64-only breakage (`cargo xtask linux-amd64 ...`) |
| `neueos-vm` | Parallels | aarch64 | Full graphical session: compositor on DRM, shell, portals |

> **Status: untested draft.** This was written from research, not from a run. While you follow
> it, fix anything that is wrong or missing (see "Record what you actually did" at the end).
> Items marked **(verify)** are the ones most likely to differ.

Run Mac-side commands from the repo root. Machine names must match the xtask defaults
(`neueos-arm64`, `neueos-amd64`) or you must export `NEUE_ORB_ARM64` / `NEUE_ORB_AMD64`.

---

## 1. OrbStack machines (arm64 and amd64)

OrbStack mounts the Mac filesystem at the same absolute path inside the machine, so the repo
is built in place and no sync is needed. `cargo xtask linux` relies on this: it runs
`orb run -m <machine> -w <repo path> env CARGO_TARGET_DIR=target/<machine> <cmd>`.

### 1.1 Create the machines
```sh
orb create arch neueos-arm64
orb create --arch amd64 arch neueos-amd64
orb list                       # both should be "running"
```
Arch has no multi-arch support, hence one machine per architecture.

### 1.2 Install packages and Rust (in each machine)
```sh
orb run -m neueos-arm64 sudo pacman -Syu --noconfirm --needed base-devel git rustup
orb run -m neueos-amd64 sudo pacman -Syu --noconfirm --needed base-devel git rustup
```
The Arch `rustup` package puts its shims in `/usr/bin`, so non-login `orb run` shells find
`cargo` without any PATH setup. Install the pinned toolchain from `rust-toolchain.toml` by
running rustup inside the repo (it reads the file; the pin is the single source of truth):
```sh
orb run -m neueos-arm64 -w "$PWD" rustup toolchain install
orb run -m neueos-amd64 -w "$PWD" rustup toolchain install
```
Library packages for the compositor. Each task that adds a linked C library adds its package
here. `neue-comp` links `libxkbcommon`, and with the `tty` feature (task 1.4) `libinput`,
`libudev` (from `systemd-libs`, already installed), `libseat` (package `seatd`) and `libgbm`
(package `mesa`). EGL, GLES and libwayland are dlopened,
but the headless tests (task 1.2) need `libwayland-client` at runtime: the dev-only `winit`
feature switches `wayland-client` to the system library for the whole build, so the
scripted test clients load it too. The render tests (task 1.16a, ADR 0021) dlopen Mesa's
EGL and draw on its software device (llvmpipe), also from `mesa`.
```sh
orb run -m neueos-arm64 sudo pacman -S --noconfirm --needed libxkbcommon wayland libinput seatd mesa
orb run -m neueos-amd64 sudo pacman -S --noconfirm --needed libxkbcommon wayland libinput seatd mesa
```

### 1.3 Check
```sh
orb run -m neueos-arm64 uname -m                      # aarch64
orb run -m neueos-amd64 uname -m                      # x86_64
cargo xtask linux cargo --version                 # 1.98.1 (the pinned version)
cargo xtask linux-amd64 cargo --version
cargo xtask linux cargo check --workspace
cargo xtask linux-amd64 cargo check --workspace
```
All the commands above must succeed. `orb run -m <machine> <cmd>` is the form xtask itself uses.

Optionally run `cargo xtask linux cargo xtask check` once; it should skip nothing and pass,
including the Linux-only crates.

### 1.4 Notes
- OrbStack machines have no GPU/DRM access. Don't try to run the compositor there; use the
  Parallels VM.
- Linux build output lands in `target/neueos-arm64/` and `target/neueos-amd64/`, so it never
  clobbers the macOS `target/`. All `target/` directories are git-ignored.
- Exit codes and stdin forwarding of `orb run` are undocumented; xtask passes the exit status
  through, so a failing command shows as `failed` in its output **(verify)**.

---

## 2. Parallels VM (Arch Linux ARM)

Route chosen from R2: the **archboot aarch64 ISO** in a Parallels VM, with GRUB (UEFI), the
Arch Linux ARM kernel, virtio-gpu and **SSH + rsync** for files. We do not depend on Parallels
Tools. If archboot fails, the fallback is the ALARM generic rootfs bootstrapped from an arm64
live ISO; if you end up there, document it below instead of this section.

### 2.1 Create the VM
1. Download the latest ISO and its checksum from `https://release.archboot.com/aarch64/latest/iso/`
   (the `.iso` and the matching `b2sum.txt`). Verify it:
   ```sh
   b2sum -c b2sum.txt --ignore-missing   # Homebrew: brew install b2sum (or compare the hash by eye)
   ```
2. Parallels → New → *Install from image* → pick the ISO. Choose **Other Linux**
   (not an Ubuntu/Fedora preset). Tick *Customize settings before installation*.
3. Name it `neueos-vm`. Suggested hardware: **4+ CPUs, 6+ GB RAM, 40+ GB disk**.
4. Graphics: enable **3D acceleration** (virgl). Leave video memory at the default.
5. Network: *Shared* (default). Don't enable the shared Mac folders or clipboard integration
   for now; they need Parallels Tools.
6. Security/boot: if the VM has Secure Boot enabled, turn it off **(verify; option name
   varies by Parallels version)**. aarch64 boots UEFI only.
7. Start the VM. Known quirk: if the keyboard doesn't work in the installer, switch away with
   Cmd+Tab and back.

### 2.2 Install with archboot
The installer is a text menu started with `setup` **(menu names below are approximate)**:
1. Prepare the storage device: **GPT** with a 512 MB EFI system partition (FAT32, mounted at
   `/boot`) and an ext4 root on the rest. No swap needed (use zram later if wanted).
2. Network: the Shared network should come up with DHCP. Check with `ping archlinux.org`.
3. Install packages: accept the Arch Linux ARM base set. Make sure the kernel is `linux-aarch64`.
4. Configure the system: hostname `neueos-vm`, a user matching your Mac username (add to
   `wheel`, the admin group), a non-empty root password and user password (sshd rejects
   empty ones; a short throwaway is fine), timezone, locale `en_US.UTF-8`.
   If the installer doesn't add the user to `wheel` or enable sudo for it, do it after the
   first boot as root: `usermod -aG wheel <user>`, then `EDITOR=nano visudo` and uncomment
   `%wheel ALL=(ALL:ALL) ALL` **(verify whether archboot does this itself)**.
5. Install the bootloader: **GRUB_UEFI**. The installer also offers EFISTUB, which depends on a firmware boot entry that VM firmware can lose. (systemd-boot is not offered by archboot.)
6. Reboot, eject the ISO (Parallels → Devices → CD/DVD → disconnect).

### 2.3 First boot: packages and services
Log in on the console as your user, then:
```sh
sudo pacman -Syu --needed base-devel git rustup rsync openssh sudo \
    seatd mesa mesa-utils libinput vulkan-swrast vulkan-tools
sudo systemctl enable --now sshd seatd
sudo usermod -aG seat,video,input "$USER"
ip -4 addr show scope global      # note the IP address
```
Log out and back in so the group changes apply. Group membership is needed for the compositor
to open DRM and input devices through seatd.

archboot sets up `kmscon` as the tty1 console. kmscon keeps DRM master, so no compositor can
modeset (seatd logs "Could not make device fd drm master: Device or resource busy" and commits
fail with "Permission denied"). Replace it with the stock getty:
```sh
sudo systemctl disable --now kmsconvt@tty1.service
sudo rm -f /etc/systemd/system/autovt@.service    # archboot's link to kmscon
sudo systemctl daemon-reload
sudo systemctl enable --now getty@tty1.service
```
For compositor tests you also want a stock Wayland compositor and its demo clients:
`sudo pacman -S --needed weston wayland-utils`. Compositors run fine from an SSH session
through seatd, with `XDG_RUNTIME_DIR=/run/user/$(id -u)`. See `docs/research/vm-graphics.md`.

### 2.4 Reach the VM from the Mac
Add to `~/.ssh/config` on the Mac (use the IP from the previous step; it is stable on the
Parallels Shared network, but if it changes, update it):
```
Host neueos-vm
    HostName 10.211.55.X
    User <your user>
```
```sh
[ -f ~/.ssh/id_ed25519 ] || ssh-keygen -t ed25519   # only if you have no key yet
ssh-copy-id neueos-vm
ssh neueos-vm uname -m            # aarch64
```
If your key lives in the 1Password SSH agent (set through `IdentityAgent` in `~/.ssh/config`),
`ssh-copy-id` can't see it, because it reads `$SSH_AUTH_SOCK`. Either prefix the command with
`env SSH_AUTH_SOCK="$HOME/Library/Group Containers/2BUA8C4S2C.com.1password/t/agent.sock"`, or
save the public key to a file and run `ssh-copy-id -f -i <key>.pub neueos-vm`. With several keys
in the agent, add `IdentityFile <key>.pub` and `IdentitiesOnly yes` to the `neueos-vm` host entry.

`cargo xtask vm sync` and `cargo xtask vm run` are not implemented yet. Their task will wrap
the manual steps below and use the host name `neueos-vm`.

### 2.5 Sync files and build (manual, until `cargo xtask vm` exists)
From the repo root on the Mac:
```sh
rsync -az --delete --exclude target --exclude .git ./ neueos-vm:neueos/
ssh neueos-vm 'cd neueos && rustup toolchain install && cargo check --workspace'
```
`rustup toolchain install` reads `rust-toolchain.toml`. Run it once per toolchain bump, not on
every sync. Build output stays on the VM disk (`~/neueos/target`) and is excluded from the sync.

### 2.6 Check
- `ssh neueos-vm uname -m` prints `aarch64`.
- `ssh neueos-vm 'cd neueos && cargo --version'` prints the pinned version.
- `ssh neueos-vm 'cd neueos && cargo check --workspace'` succeeds.
- `ssh neueos-vm ls /dev/dri` lists `card0` and `renderD128`. If it is empty, 3D acceleration
  is off or virtio-gpu isn't active; note it, since task 0.5 depends on it.

Deeper graphics checks (`eglinfo`, Smithay's `anvil`, vello demo) were task 0.5; results are in
`docs/research/vm-graphics.md`. To see the VM screen from the Mac, run
`prlctl capture neueos-vm --file screen.png` (it can return a partial frame while something is
animating).

---

## Record what you actually did
This doc is accepted when it matches reality. When you finish, edit the sections above where you
deviated, and fill in:

| Item | Result |
|---|---|
| macOS / Parallels / OrbStack versions | macOS 27.2 / Parallels 27.1.0 / Orbstack 2.2.5 |
| `neueos-arm64`: created, `cargo xtask linux cargo check` passes | Yes |
| `neueos-amd64`: created, `cargo xtask linux-amd64 cargo check` passes | Yes |
| archboot ISO file name and version used | archboot-2026.11.02-02.34-7.3.1-2-aarch64-ARCH-aarch64 |
| Parallels install route (archboot, or fallback) | archboot |
| Keyboard worked in installer without the Cmd+Tab workaround? | Yes |
| `/dev/dri` contents in the VM | by-path, card0, renderD128 |
| Parallels Tools tried? Shared folders usable? | No |
| Anything in this doc that was wrong | No |

## 3. Running neue-comp nested in the VM
For compositor tasks checked "nested under weston in the VM". One-time packages:
`sudo pacman -S --needed weston foot ydotool`.
```sh
rsync -az --delete --exclude target --exclude .git ./ neueos-vm:neueos/
ssh neueos-vm 'cd neueos && cargo build --release -p neue-comp'
ssh neueos-vm 'export XDG_RUNTIME_DIR=/run/user/1000
  setsid nohup weston --backend=drm --idle-time=0 --socket=wl-stock >/tmp/weston.log 2>&1 </dev/null &
  sleep 2; WAYLAND_DISPLAY=wl-stock setsid nohup ~/neueos/target/release/neue-comp \
    -c foot -c weston-terminal >/tmp/neue-comp.log 2>&1 </dev/null &'
prlctl capture neueos-vm --file /tmp/shot.png      # look at the result from the Mac
```
`neue-comp -c <cmd>` spawns `<cmd>` as a client (repeatable); `RUST_LOG=neue_comp=debug` logs
window mapping and clicks. Super+Q quits.

**State and actions:** inside the VM session `neuectl windows`, `neuectl outputs` and
`neuectl action <name>` talk to the compositor (`NEUE_SOCKET` is set for `-c` children;
over ssh set `XDG_RUNTIME_DIR` and `WAYLAND_DISPLAY`). It has no input injection.

**Scripted input:** `prlctl send-key-event neueos-vm
--scancode <set-1 scancode>` types into the focused window. For the pointer, run ydotool's
daemon as root and drive it as the user:
```sh
ssh neueos-vm 'sudo sh -c "setsid nohup ydotoold --socket-path=/tmp/.ydotool_socket \
  --socket-own=1000:1000 >/dev/null 2>&1 </dev/null &"'
ssh neueos-vm 'export YDOTOOL_SOCKET=/tmp/.ydotool_socket
  ydotool mousemove -x 10 -y 10; ydotool click 0xC0; ydotool type "echo ok"; ydotool key 28:1 28:0'
```
Quirks: `mousemove --absolute` lands at twice the requested coordinates (relative moves are
1:1). Smithay's winit backend ignores pointer-enter, so after the pointer jumps into the
nested window, send one relative move before clicking, or the click uses a stale position.
Stop with `sudo pkill -x ydotoold; pkill -x weston` (`pkill -x`, see `vm-graphics.md`).

## 4. Running neue-comp on the VM console (tty backend)
For compositor tasks checked "in the VM on DRM under seatd". Needs §2.3 (seatd running, the
user in `seat`, `video` and `input`, kmscon replaced by getty) and nothing else holding DRM
master (stop weston first: `pkill -x weston`).
```sh
rsync -az --delete --exclude target --exclude .git ./ neueos-vm:neueos/
ssh neueos-vm 'cd neueos && cargo build --release -p neue-comp -p neuectl --no-default-features --features tty'
ssh neueos-vm 'export XDG_RUNTIME_DIR=/run/user/1000; unset WAYLAND_DISPLAY
  setsid nohup ~/neueos/target/release/neue-comp --tty -c foot >/tmp/neue-comp.log 2>&1 </dev/null &'
prlctl capture neueos-vm --file /tmp/shot.png
```
Without `WAYLAND_DISPLAY`, the tty backend is the default, so `--tty` is optional. It takes over
the active VT (tty1). Ctrl+Alt+F1…F12 switch VTs, Super+Q quits, and so does
`WAYLAND_DISPLAY=wayland-1 neuectl quit` over ssh. The console comes back on exit.
`NEUE_DRM_DEVICE=/dev/dri/cardN` picks a GPU.

**Scripted input** works as in §3: `prlctl send-key-event` reaches libinput through the VM
keyboard, and ydotool's uinput device is picked up by libinput as a new device. Chords need
explicit press/release: `prlctl send-key-event neueos-vm -s 29 -e press` (Ctrl), then 56
(Alt) and 60 (F2), then the same with `-e release`.

**Known log noise on virtio-gpu:** "Failed to destroy old mode property blob" at start-up and
"Failed to restore previous state. Error: Invalid argument" on exit. The DRM state that fbcon
left behind is stale. Nothing breaks: the console redraws on exit.

## 5. Checking config and hot reload in the VM (task 1.6)
Rebuild first (rsync and `cargo build --release -p neue-comp -p neuectl ...` as in §4; the
old binaries don't know `reload-config`). Run the compositor as in §3 (nested) or §4 (tty), with
`foot` as the client, then, in a second ssh session (use `XDG_CONFIG_HOME` if you don't want to touch `~/.config`):
```sh
export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-1
export PATH=~/neueos/target/release:$PATH   # neuectl is not installed; §4 builds it here
mkdir -p ~/.config/neueos
neuectl events &                      # prints config_reloaded / config_error lines
cat > ~/.config/neueos/compositor.toml <<'T'
[keyboard]
layouts = ["us", "de"]
options = ["grp:alt_shift_toggle"]
T
```
Expect a `config_reloaded` event within about 150 ms. In `foot`, press Alt+Shift, then
Z and Y: after the switch they come out swapped (German layout). Then:
- Write `layouts = ["nonexistent"]` or break the syntax: expect a `config_error` event (and a
  `neuectl reload-config` error naming the file, line and column) while the German layout
  keeps working. Fix the file: `config_reloaded`, no restart.
- Add `[keybindings]` with `"Super+Q" = "none"` and `"Super+Shift+Q" = "quit"`: Super+Q does
  nothing now, Super+Shift+Q quits.
- Output settings (`[outputs."<name>"] scale` and `position`) are checked in §6.

## 6. Checking outputs, hotplug and scaling in the VM (task 1.7)
Rebuild and start on the tty as in §4 (`-c foot`). In a second ssh session:
```sh
export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-1
export PATH=~/neueos/target/release:$PATH
neuectl outputs                      # Virtual-1 at the VM window's size, +0+0
neuectl events                       # leave running: output_* and window_* lines
```
1. **Hotplug by resizing:** drag a corner of the Parallels VM window. Expect an `output_changed` line with the new `width`/`height` within a second
   or two, the desktop filling the window again, and `foot` still usable. Zoom `foot`
   first (`neuectl action zoom-window`) and it should be refitted to the new size. If no
   event arrives and Parallels just stretches the picture, the guest resolution isn't
   following the window (a Parallels display setting, not a compositor bug); note it.
2. **Scale:** `printf '[outputs."Virtual-1"]\nscale = 1.5\n' >> ~/.config/neueos/compositor.toml`
   (back the file up first). Expect `output_changed` with `"scale":1.5` and `foot`'s text
   redrawn larger but sharp (fractional scaling, not a blurry upscale). Remove the lines
   again afterwards.
3. **Second display (optional):** if Parallels offers a second display for the VM
   (Hardware → Graphics), enable it: expect `output_added` for `Virtual-2` to the right of
   `Virtual-1`; `neuectl action move-window --x <its x + 100> --y 100` moves `foot` there.
   Disable it again: `output_removed`, and `foot` comes back to `Virtual-1`.

Report the `neuectl events` lines and anything that looks wrong (black output, stuck
frames, cursor in the wrong place).

## 7. Checking clipboard and drag-and-drop in the VM (task 1.8)
Install the test tools once: `sudo pacman -S --needed wl-clipboard gedit gnome-text-editor
nautilus` (any two GTK apps work; a text editor and a file manager are the easiest pair).
Rebuild and start on the tty as in §4 (`-c foot`). In a second ssh session:
```sh
export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-1
export PATH=~/neueos/target/release:$PATH
```
1. **Clipboard tools:** `echo hello | wl-copy`, then `wl-paste` prints `hello`; also try
   `wl-paste --primary` after `echo x | wl-copy --primary`.
2. **Clipboard between apps:** start two GTK apps (`gnome-text-editor &` twice, or
   `gedit &`), type text in the first, select it, copy, click the second one and paste.
   Selecting text and middle-clicking in the other app tests primary selection.
3. **DnD of a file:** open `nautilus ~` and a second `nautilus /tmp`. Drag a file from one
   to the other with the mouse. Expect a drag icon following the pointer, the file
   appearing in the target folder, and the icon disappearing on release. Also drag a file
   onto the text editor (it opens it or inserts its path) and press Esc during a drag:
   the icon must go away and nothing is dropped.

Report which of these worked and anything odd (no icon, icon stuck after the drop, icon
offset from the pointer, paste empty).

## 8. Checking cursors in the VM (task 1.9)
Install a theme once if the VM has none: `sudo pacman -S --needed adwaita-cursors` (or any
XCursor theme). Rebuild and start on the tty as in §4 (`-c foot`); optionally set
`XCURSOR_THEME=Adwaita XCURSOR_SIZE=24` for the compositor. Use `gnome-text-editor`
(GTK 4, uses cursor-shape-v1) and `foot`:
1. **Text field:** move over the editor's text area: the pointer becomes an I-beam; over
   the header bar's buttons it is the arrow. In `foot` it is also an I-beam.
2. **Links / buttons:** hover a link (e.g. in `gnome-text-editor`'s About dialog, or any
   GTK link button): the hand pointer.
3. **Resize edges:** grab a window edge or corner with the mouse (GTK draws its own
   invisible edges). While dragging, the pointer shows the matching arrow (`nw-resize`,
   `e-resize`...) even when it leaves the window, and returns to normal on release.
4. **Scale:** add `[outputs."<name>"] scale = 2` (§6) and check the pointer stays sharp and
   the same apparent size as at scale 1.
5. **No theme:** `XCURSOR_THEME=nonexistent`: the built-in arrow is drawn for every shape.
6. **Hardware plane:** the pointer must move smoothly while a window animates or `foot`
   scrolls output (the plane is used when the cursor fits it; no log line says so).

Report what showed the wrong cursor and which app it was.

## 9. Checking server-side title bars in the VM (task 1.10)
Install a GTK3 app without a header bar once: `sudo pacman -S --needed mousepad`. Rebuild
and start on the tty as in §4, with debug logs for the chrome:
```sh
ssh neueos-vm 'export XDG_RUNTIME_DIR=/run/user/1000 RUST_LOG=info,neue_comp::decorations=debug
  unset WAYLAND_DISPLAY
  setsid nohup ~/neueos/target/release/neue-comp --tty -c foot -c mousepad >/tmp/neue-comp.log 2>&1 </dev/null &'
```
1. **GTK3 (KDE protocol):** `mousepad` has our title bar: grey gradient, rounded top
   corners, red/yellow/green buttons on the left, its title centred.
2. **xdg-decoration:** `foot` has the same bar.
3. **Focus:** click into one window, then the other: the unfocused bar turns lighter and its
   buttons grey.
4. **Resize and zoom:** resize a window from its client edge (mousepad) or zoom it with
   `neuectl action zoom-window --window ID`: the bar follows, the title stays centred and
   gets an ellipsis when the window is narrow; a zoomed window's bar sits at the top of the
   screen.
5. **Scale:** with `[outputs."<name>"] scale = 2` (§6) the bar stays sharp.
6. **Timings:** `grep rendered /tmp/neue-comp.log` shows "rendered title bar slices" and
   "rendered window title" with `micros=`.

Header-bar apps (`gnome-text-editor`, `gtk3-demo`) keep their own title bars by design.
Clicking, dragging and resizing the bars is §10.

## 10. Checking chrome interaction in the VM (task 1.11)
Rebuild and start as in §9 (`-c foot -c mousepad`; add `RUST_LOG=info,neue_comp=debug` to see
"click on chrome"). Use the VM's own pointer. Check each line:
1. **Hover:** move over the three buttons: all three show their glyphs (×, −, +), also in
   the unfocused window, where they turn coloured. Moving off puts them back.
2. **Press:** hold the mouse down on a button: it darkens. Drag off it: it pops back up;
   release there and nothing happens. Release on it: close ends the app, minimise hides the
   window (bring it back with `neuectl action unminimize-window --window ID`), zoom fills
   the work area and a second click restores it.
3. **Drag:** drag the bar: the window follows the pointer. A click that wobbles by 1–2 px
   does not move it. Drag a zoomed window: it restores under the pointer.
4. **Double click:** double-click the bar to zoom and again to restore. With
   `[windows]` `title-bar-double-click = "minimize"` in `compositor.toml` (and
   `neuectl action reload-config`) it minimises instead; `"none"` does nothing.
5. **Edges:** the pointer turns into a resize cursor 5 px outside the window's left, right,
   top and bottom edges and at the corners; dragging resizes (mousepad and foot). A
   zoomed window has no edges.
6. **Focus:** clicking an unfocused window's bar focuses and raises it. Clicking a
   button of an unfocused window does not.
7. **Clients don't see it:** moving over the bar must not change a text cursor to the
   client's, and clicking on it must not move the text caret in mousepad.

## 11. Checking the shell-facing protocols in the VM (task 1.12)
Install the clients once: `sudo pacman -S --needed swaylock swaybg swayidle fuzzel waybar foot`.
Rebuild and start on the tty as in §4 (`-c foot`, `RUST_LOG=info,neue_comp=debug`), then run
each client from a foot terminal (the compositor's `WAYLAND_DISPLAY` is set there).
1. **Panel (layer shell, exclusive zone):** `waybar` appears along the top edge. New windows
   open below it, and `neuectl action zoom-window --window ID` fills only the space under
   it. Killing waybar gives the space back and zoomed windows refit.
2. **Background:** `swaybg -c '#336699'` paints behind the windows; windows stay on top and
   clicking it focuses nothing (the focused window loses focus).
3. **Launcher (exclusive keyboard):** `fuzzel` opens on top, takes the keyboard (typing goes
   to it), and Escape closes it; the previous window has the keyboard again
   (`neuectl focused-window`). Clicking another window while it is open does not take the
   keyboard.
4. **Session lock:** `swaylock -c 000000` covers the screen. Typing and clicking never reach
   the windows underneath, Super+Q (or any binding) does nothing, Ctrl+Alt+F2 still
   switches VT. Type your password (the VM user's) and the session unlocks with the
   previous window focused. Kill swaylock with `pkill -9 swaylock` while locked: the screen
   stays locked; starting `swaylock` again and unlocking works.
5. **Idle:** `swayidle -w timeout 5 'echo idle' resume 'echo resume'` prints `idle` after
   5 s without input and `resume` on the next key or pointer move.
6. **Globals:** `wayland-info | grep -E 'layer_shell|session_lock|idle_notifier|idle_inhibit|foreign_toplevel_list|xdg_foreign|xdg_wm_dialog|toplevel_icon|system_bell|shortcuts_inhibit'`
   lists all of them. The toplevel list's identifiers (`window-<id>`) are covered by the
   headless tests.
Record which of these you ran in the PR.

## 12. Checking the input protocols in the VM (task 1.13)
Install once: `sudo pacman -S --needed fcitx5 fcitx5-mozc fcitx5-gtk gtk3-demos wev foot`
(any input method engine works; `fcitx5-chinese-addons` and `fcitx5-hangul` are alternatives).
Rebuild and start on the tty as in §4 (`-c foot`, `RUST_LOG=info,neue_comp=debug`).
1. **Globals:** `wayland-info | grep -E 'pointer_constraints|relative_pointer|pointer_gestures|text_input|input_method'`
   lists all five.
2. **Input method (the acceptance check):** from a foot terminal run
   `GTK_IM_MODULE=wayland fcitx5 -d`, then `gtk3-demo` and open *Entry*. Add the engine in
   `fcitx5-configtool` (or `~/.config/fcitx5/profile`). Click the entry, switch to the engine
   (Ctrl+Space) and type: text is composed and committed into the entry, and the candidate
   popup shows next to the caret. Switching windows and back keeps working.
   `RUST_LOG=neue_comp=debug` shows no "cannot track an input method popup" warning.
3. **Relative pointer and lock:** `wev` shows pointer motion; for a lock use any client that
   locks (a game, `mpv` in a 3D mode, Blender's *Cursor warp*). The pointer stops while
   locked and relative motion still arrives. Switching windows (a key binding) releases it.
   Parallels' pointer is absolute: a lock there only holds while the VM captures the mouse
   (Ctrl+Alt to release). Locks are best judged on real hardware.
4. **Gestures:** on a touchpad (real hardware, or a Mac trackpad forwarded to the VM):
   `wev` does not show gestures; use `libinput debug-events --show-keycodes` for the device
   side and a client with pinch zoom (`gtk3-demo` *Gestures*, Firefox) for the compositor side.
   Setting `[touchpad] gestures = false` in `compositor.toml` stops them reaching clients.
5. **Touchpad config:** set `tap-to-click = false`, save, and tapping stops clicking without a
   restart; the log line `touchpad configured` appears for each touchpad at start-up.
Record which of these you ran in the PR; 3 and 4 need real hardware.

## 13. Checking presentation and sync in the VM (task 1.14)
Install once: `sudo pacman -S --needed weston wayland-utils flatpak mesa-utils vulkan-tools`
(`weston` only for its demo clients). Rebuild and start on the tty as in §4 (`-c foot`,
`RUST_LOG=info,neue_comp=debug`).
1. **Globals:** `wayland-info | grep -E 'wp_presentation|tearing_control|security_context|drm_syncobj'`
   lists the first three. The log says either `explicit sync (linux-drm-syncobj-v1)
   enabled` (then `wp_linux_drm_syncobj_manager_v1` is listed too) or `GPU lacks
   syncobj_eventfd`; note which one the VM's virtio GPU gives.
2. **Presentation feedback:** `weston-presentation-shm -f` runs and prints timings with a
   refresh near 16.67 ms (60 Hz) and increasing sequence numbers; nothing reports
   `discarded` in a steady state.
3. **Sandboxed clients:** `flatpak install flathub org.gnome.Calculator`, then
   `flatpak run org.gnome.Calculator`. It opens and works; the log shows `security context
   created` with `engine=Some("org.flatpak")`. From the host, `wayland-info` still lists
   `zwlr_layer_shell_v1` and `ext_session_lock_manager_v1` (unsandboxed clients keep them).
4. **Explicit sync (NVIDIA PC, real hardware; see 1.18):** with the proprietary driver
   (560 or newer) the log says explicit sync is enabled; `vkcube` and Firefox run without
   flicker or stalls, and `WAYLAND_DEBUG=1 vkcube 2>&1 | grep -m3 set_acquire_point`
   shows the client using it.
Record which of these you ran in the PR; 4 needs the NVIDIA PC.

## 14. Checking full-screen windows in the VM (task 1.15a)
Install once: `sudo pacman -S --needed mpv waybar`. Rebuild and start on the tty as in §4
(`-c foot`). Get a short video onto the VM (any file; `mpv` also plays
`av://lavfi:testsrc` without one).
1. **Client request:** `waybar &`, then `mpv --fs av://lavfi:testsrc`. The video fills the
   whole screen: no title bar, the bar of `waybar` is hidden behind it, and
   `neuectl windows` shows mpv with `[fullscreen]` at `0,0` and the output's size.
   Press `f` in mpv: it goes back to its window, at its old place and size, below
   `waybar` again. Press `f` again, then `q`.
2. **Action:** open `foot`, then `neuectl action fullscreen-window`: it fills the screen;
   run it again (`--window ID` from `neuectl windows`) and it comes back where it was.
3. **Dialog:** in `gnome-text-editor` (or `gedit`) press F11 for full screen, then open
   the Save dialog (Ctrl+Shift+S): the dialog shows over it and `waybar` stays hidden.
   Escape, then F11 to leave.
4. **Mode change:** with mpv full screen, resize the Parallels window: mpv follows the new
   size.
Record which of these you ran in the PR.

## 15. Checking draw-only-when-needed in the VM (task 1.15b)
Rebuild and start on the tty as in §4, with
`RUST_LOG=info,neue_comp::tty=trace neue-comp --tty -c foot 2>/tmp/comp.log`.
1. **Idle renders nothing:** with `foot` open and the pointer still, wait 10 s, then from
   another VT (or over ssh) run `grep -c rendered /tmp/comp.log` twice, 10 s apart, with
   your hands off the VM: the count does not change.
2. **Redraws on demand:** move the pointer, type in `foot`, drag and resize a window, hover
   and click the title-bar buttons, `neuectl action minimize-window` and restore it: each
   shows at once, nothing lags behind by a frame or stays stale.
3. **Animations keep running:** `mpv av://lavfi:testsrc` plays smoothly; `vkcube` (or
   `glxgears` via `xwayland-satellite` later) spins at the refresh rate.
4. **Layers, lock, config:** start `waybar`, then kill it: the bar disappears. Lock with
   `swaylock` and unlock. Edit `~/.config/neueos/compositor.toml` (change a key binding)
   and save: no stale frames.
5. **Animated cursor:** with a theme that animates the busy cursor (Adwaita's `wait` and
   `progress`), point at an app showing it (Firefox while a page loads): it animates and
   the log keeps counting renders; once the cursor is back to the arrow, it stops.
Record which of these you ran in the PR.

## 16. Checking direct scanout and adaptive sync (task 1.15c)
Install once: `sudo pacman -S --needed mpv drm_info`. Steps 1–2 work in the VM; 3–5 need
an x86_64 PC with a FreeSync / G-Sync Compatible / VESA Adaptive-Sync monitor (usually
over DisplayPort; HDMI VRR needs HDMI 2.1 and, on AMD, a recent kernel), with VRR (often
called FreeSync) switched on in the monitor's own menu. Rebuild and start on the tty as in
§4, with `RUST_LOG=info,neue_comp=debug neue-comp --tty -c foot 2>/tmp/comp.log`.
1. **Config:** `neuectl outputs` ends each line with `vrr off`. Add
   `[outputs."NAME"]` with `adaptive-sync = "fullscreen"` to
   `~/.config/neueos/compositor.toml` (NAME from `neuectl outputs`) and save: the line
   says `vrr fullscreen` (`vrr fullscreen (unsupported)` in the VM, plus a warning
   `adaptive sync not applied` in the log). `adaptive-sync = "sometimes"` is rejected
   with a config error and the old value stays.
2. **Direct scanout:** `WAYLAND_DEBUG=1 mpv --fs --vo=gpu-next av://lavfi:testsrc 2>&1 |
   grep -m5 -E 'tranche_flags|presented'`. Once full screen, the feedback shows a
   `tranche_flags(1)` (scanout) tranche and `presented(…)` events end in flags `15`
   (`zero_copy` set). The VM's virtio GPU may refuse the plane: then flags stay `7`, which
   is fine there; note which one you saw. Leave full screen (`f`): flags drop back to `7`
   and the picture stays right (no black frame, no stale image). Move the pointer over
   the video: it stays smooth (the cursor has its own plane).
3. **Scanout on the PC:** repeat step 2 on the x86_64 PC (each GPU you have: AMD, Intel,
   NVIDIA with the proprietary driver). Flags `15` while full screen; no flicker when
   entering or leaving full screen, or when a notification or `swaylock` appears over it.
4. **VRR `fullscreen`:** with `adaptive-sync = "fullscreen"`, `neuectl outputs` shows
   `vrr fullscreen` on the desktop and `vrr fullscreen (active)` while `mpv --fs` (or a
   game) is up; the log says `adaptive sync switched`. `sudo drm_info | grep -E
   'vrr_capable|VRR_ENABLED'` shows `vrr_capable` 1 and `VRR_ENABLED` 1 while full
   screen, 0 after. With the monitor's refresh-rate overlay on, `mpv --fs
   --video-sync=audio` of a 24 or 25 fps file shows the rate following the video (48/50
   Hz or similar) instead of the maximum. If the log instead says `the driver needs a
   modeset`, note it and try step 5.
5. **VRR `on`:** with `adaptive-sync = "on"`, `vrr on (active)` all the time (one blank
   of about a second when it is first applied is expected on drivers that need a
   modeset); the desktop has no visible flicker when idle (some panels flicker at low
   rates: if yours does, note it, that is why `fullscreen` exists).
Record which of these you ran, on which GPU and monitor, in the PR.

## 17. Checking screenshots and screencopy in the VM (task 1.16d)
Everything runs from the Mac over ssh, from the repo root. `neueos-vm` is the host from
§2.4; the session is on the tty as in §4. The commands contain no local shell syntax, so
they work from fish, zsh or bash. Over ssh the Wayland tools need `XDG_RUNTIME_DIR` and
`WAYLAND_DISPLAY`: `docs/vm-env.sh` (rsynced with the repo) sets them.

**1. Install once** (needs `grim` 1.5 or newer, which speaks `ext-image-copy-capture`;
older ones only know wlr-screencopy and say the compositor "doesn't support the screen
capture protocol"):
```sh
ssh -t neueos-vm 'sudo pacman -S --needed grim wl-mirror jq ydotool'
ssh neueos-vm 'grim -h 2>&1 | head -3; wl-mirror --version'
```

**2. Build, start, and wait until it listens.** Stop weston and any old compositor first.
```sh
rsync -az --delete --exclude target --exclude .git ./ neueos-vm:neueos/
ssh neueos-vm 'cd neueos && cargo build --release -p neue-comp -p neuectl --no-default-features --features tty'
ssh neueos-vm 'export XDG_RUNTIME_DIR=/run/user/1000
  pkill -x weston; WAYLAND_DISPLAY=wayland-1 ~/neueos/target/release/neuectl quit 2>/dev/null; sleep 1
  unset WAYLAND_DISPLAY; export RUST_LOG=info,neue_comp=debug
  setsid nohup ~/neueos/target/release/neue-comp --tty -c foot >/tmp/comp.log 2>&1 </dev/null &
  sleep 3; WAYLAND_DISPLAY=wayland-1 ~/neueos/target/release/neuectl windows'
```
`neuectl windows` must list one `foot` window. If not, read `ssh neueos-vm tail -30
/tmp/comp.log` (`seatd` and `/dev/dri` are the usual causes, §2.3).

**3. Put the pointer on the screen** (ydotool as in §3; the cursor is only drawn when the
pointer is over an output, and the tty backend draws it itself):
```sh
ssh neueos-vm 'sudo sh -c "setsid nohup ydotoold --socket-path=/tmp/.ydotool_socket \
  --socket-own=1000:1000 >/dev/null 2>&1 </dev/null &"; sleep 1
  YDOTOOL_SOCKET=/tmp/.ydotool_socket ydotool mousemove -x 40 -y 40'
```

**4. `neuectl screenshot`.** Each line sources `docs/vm-env.sh` on the VM (it sets
`XDG_RUNTIME_DIR`, `WAYLAND_DISPLAY` and `PATH`). All the commands are single-quoted, so
they work unchanged from fish, zsh or bash on the Mac.
```sh
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; neuectl screenshot /tmp/a.png'
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; cd /tmp && neuectl --json screenshot b.png'
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; neuectl screenshot --cursor /tmp/cursor.png'
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; neuectl screenshot --output NOPE /tmp/c.png'
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; neuectl screenshot /tmp/nodir/d.png'
```
Expected: the first three print the file (or JSON), the output name and its size
(`1920x1080` or whatever the VM runs); the last two print `unknown_output` and
`capture_failed` and exit non-zero.

**5. `grim`** (shm captures; `-c` is the cursor path that only the tty backend has):
```sh
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; grim /tmp/grim.png; grim -c /tmp/grim-cursor.png'
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; grim -g "$(window_geometry)" /tmp/grim-win.png'
```

**6. dmabuf.** `wf-recorder` is no use here: it only speaks `wlr-screencopy`, which
`neue-comp` does not offer (ADR 0022). `wl-mirror` speaks `ext-image-copy-capture` and asks for
dmabufs when the GPU allows. It mirrors an output into a window; mirroring the output it
sits on gives a picture-in-picture tunnel, which is fine for this. The compositor logs which
kind of buffer each frame went into. Count the log lines before and after, and take a
screenshot while it runs:
```sh
ssh neueos-vm 'grep -c "into a shm buffer" /tmp/comp.log; grep -c "into a dmabuf" /tmp/comp.log'
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; (timeout -s INT 10 wl-mirror "$(output_name)" >/tmp/mirror.log 2>&1 &); sleep 4; neuectl windows; neuectl screenshot /tmp/mirror.png; sleep 7; cat /tmp/mirror.log'
ssh neueos-vm 'grep -c "into a shm buffer" /tmp/comp.log; grep -c "into a dmabuf" /tmp/comp.log'
```
`neuectl windows` while it runs lists a second window (`wl-mirror`), and `mirror.png`
shows the screen inside itself. If the `dmabuf` count goes up, the dmabuf path works on
this GPU; if only the `shm` count goes up, `wl-mirror` chose shm (the VM's virtio GPU may
not offer one it likes: then dmabuf stays unverified, and the x86_64 PC with a real GPU is
the place for it). If `/tmp/mirror.log` shows a protocol or EGL error instead, paste it
into the PR.

**7. Look at the results on the Mac.**
```sh
mkdir -p ~/Desktop/neueos-shots && scp 'neueos-vm:/tmp/*.png' ~/Desktop/neueos-shots/
open ~/Desktop/neueos-shots
```
Check: `a.png` is the whole screen with the `foot` window and title bar; `cursor.png` and
`grim-cursor.png` show a pointer near the top-left and `a.png` and `grim.png` do not;
`grim-win.png` is only the window; `mirror.png` shows the screen inside itself, repeating.

**8. Clean up.**
```sh
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; neuectl quit; sudo pkill -x ydotoold; true'
```
Record which steps you ran, and what the log counts in step 6 were, in the PR.

## 18. Checking the global-menu addresses in the VM (task 1.17)
Runs from the Mac over ssh, as §17. Needs the compositor from §17 step 2 running (rebuild
it first: the rsync and `cargo build` lines there), and these packages (GTK4 and Qt6 are
already there from the 0.7 run):
```sh
ssh -t neueos-vm 'sudo pacman -S --needed kate gnome-text-editor python-gobject'
```

**1. Protocols are offered.**
```sh
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; wayland-info | grep -E "appmenu|gtk_shell"'
```
Expected: `org_kde_kwin_appmenu_manager` version 2 and `gtk_shell1` version 5.

**2. Kate and GNOME Text Editor.** The script starts its own session bus with a stand-in
registrar (Kate exports nothing without one, ADR 0005), starts both apps, and prints
`neuectl windows` and the bus names they own after eight seconds:
```sh
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; sh ~/neueos/docs/vm-appmenu.sh'
```
Expected: the Kate line ends with `[menu dbusmenu :1.N /MenuBar...]` (a unique name and
`/MenuBar` or `/MenuBar/N`), and the GNOME Text Editor line with `[menu gmenu :1.N
menubar=... app=/org/gnome/TextEditor win=/org/gnome/TextEditor/window/N]` (it may have no
`menubar=`: it only has a primary menu; the `app=` and `win=` paths are what matter). Paste the two lines
into the PR. If a line has no `[menu ...]`, run the app by hand with
`WAYLAND_DEBUG=1 ... 2>&1 | grep -E "appmenu|set_dbus_properties|set_address"` to see
whether it sent anything, and compare with `docs/research/global-menu-checklist.md`.

**3. Events.** In a second ssh session, `neuectl events` must print a `window_changed` line
with `menu` when an app announces its address (run step 2 again to see it).

Record which of these you ran in the PR. Not checkable here: Firefox, Electron and
libadwaita apps send nothing and get the fallback menu (Phase 3).

## 19. Checking xwayland-satellite on demand in the VM (task 1.18)
Runs from the Mac over ssh, as §17. Rebuild and restart the compositor first (the rsync and
`cargo build` lines in §17 step 2). Needs satellite 0.7 or newer (the second command must print `exit=0`; satellite has no
`--help`) and an X11 app:
```sh
ssh -t neueos-vm 'sudo pacman -S --needed xwayland-satellite xorg-xeyes xterm'
ssh neueos-vm 'pacman -Q xwayland-satellite; xwayland-satellite :9 --test-listenfd-support; echo exit=$?'
```
Start the compositor with the log on: `RUST_LOG=info neue-comp --tty -c foot 2>/tmp/comp.log`
(from the VM console, as §4). Its log should say `X11 apps start xwayland-satellite on
demand` with a display, normally `:0`.

**1. Nothing runs before the first X client.**
```sh
ssh neueos-vm 'pgrep -a -f "xwayland|Xwayland" || echo none; ls -la /tmp/.X11-unix /tmp/.X0-lock'
```
Expected: `none`, the socket `X0` and the lock file `.X0-lock`.

**2. An X11 app appears.** `DISPLAY` reaches the compositor's `--command` children; from
ssh, set it by hand:
```sh
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; DISPLAY=:0 setsid xeyes >/dev/null 2>&1 & sleep 3; neuectl windows; pgrep -a -f "xwayland-satellite|Xwayland"'
```
Expected: `xeyes` is a window on the VM screen (the eyes follow the pointer), is listed by
`neuectl windows`, and `xwayland-satellite` and `Xwayland` now run. `/tmp/comp.log` has
`started xwayland-satellite`.

**3. More apps, and a restart.** Run `DISPLAY=:0 xterm` too: no second satellite starts.
Then `pkill xwayland-satellite` and run `xeyes` again: it appears again, after at most
a second (the log says `xwayland-satellite exited`, then `started` again).

**4. Clean exit.** Quit the compositor (Super+Q): `ls /tmp/.X0-lock /tmp/.X11-unix/X0`
reports no such files and no satellite is left.

Record which of these you ran in the PR. Not covered: the shell and launcher getting
`DISPLAY` (the session exports it, task 1.19), global menus for X11 apps, scaling of X11
windows on HiDPI outputs.

## 20. Checking the session from greetd in the VM (task 1.19)
Needs a build of `neue-comp` and `neue-session` in `/usr/bin`: no package yet (task 1.20), so
install by hand from the Mac (sync first: `rsync -az --delete --exclude target --exclude .git ./ neueos-vm:neueos/`, the VM copy is not a git checkout). The VM console is the greetd VT, so steps 3 to 5 are typed
there; the rest runs over ssh.
```sh
ssh neueos-vm 'cd ~/neueos && cargo build --release -p neue-comp -p neue-session -p neuectl'
ssh -t neueos-vm 'cd ~/neueos && sudo install -m755 target/release/{neue-comp,neue-session,neuectl} /usr/bin/ &&
  sudo install -m644 session/systemd/user/* /usr/lib/systemd/user/ &&
  sudo install -m644 session/wayland-sessions/neueos.desktop /usr/share/wayland-sessions/ &&
  sudo pacman -S --needed greetd greetd-agreety &&
  sudo install -m644 session/greetd/config.toml /etc/greetd/config.toml &&
  sudo systemctl enable greetd.service'
```
Stop any other display manager first (`sudo systemctl disable --now sddm` or the like) and
reboot, or `sudo systemctl start greetd` from the console.

**1. Units are valid.**
`ssh neueos-vm 'systemd-analyze --user verify neue-comp.service neue-session.target'`:
no errors.

**2. Login.** Log in at the console prompt. Expected: the compositor starts (the cursor
appears). Over ssh:
```sh
ssh neueos-vm 'XDG_RUNTIME_DIR=/run/user/1000 systemctl --user is-active neue-comp.service neue-session.target graphical-session.target'
ssh neueos-vm 'XDG_RUNTIME_DIR=/run/user/1000 systemctl --user show-environment | grep -E "WAYLAND_DISPLAY|DISPLAY|XDG_CURRENT_DESKTOP|XDG_SESSION_TYPE"'
```
Expected: three `active`; `WAYLAND_DISPLAY=wayland-1`, `DISPLAY=:0` (satellite found),
`XDG_CURRENT_DESKTOP=neueOS`, `XDG_SESSION_TYPE=wayland`.

**3. A client sees the environment.** Units get it: `systemd-run --user --wait --pty env | grep DISPLAY`
prints both, and `systemd-run --user foot` opens a terminal on the screen.

**4. Second start refuses.** `neue-session` over ssh prints `neue-session.target is already
active` and exits 1.

**5. Clean logout.** Quit the compositor (Super+Q, or `neuectl quit` over ssh with
`XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-1`). Expected: the console returns to the
greetd login within a few seconds; `journalctl --user -u neue-comp -b` ends with `neue-comp exited`
and no SIGKILL; `systemctl --user show-environment | grep -c WAYLAND_DISPLAY` is `0`;
`ls /tmp/.X0-lock` fails. Also try `neue-session stop` from ssh while logged in.

Things to watch: if the target never reaches active, `journalctl --user -u neue-comp` says
whether `READY=1` was sent; if `ExecStop` cannot find the socket, the service logs a
`neuectl` error and the 10 s timeout kills the compositor (a bug to report).

Record which of these you ran in the PR. Not covered: the graphical greeter, the shell as a
unit, lingering users.
