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
scripted test clients load it too.
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
