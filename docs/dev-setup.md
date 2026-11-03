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
Library packages for the compositor (libinput, mesa, seatd, libxkbcommon, ...) are **not**
needed yet. The task that adds each dependency adds its package here.

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

Route chosen from R2: the **archboot aarch64 ISO** in a Parallels VM, with systemd-boot, the
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
5. Install the bootloader: **systemd-boot**.
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

### 2.4 Reach the VM from the Mac
Add to `~/.ssh/config` on the Mac (use the IP from the previous step; it is stable on the
Parallels Shared network, but if it changes, update it):
```
Host neueos-vm
    HostName 10.211.55.X
    User <your user>
```
```sh
ssh-copy-id neueos-vm
ssh neueos-vm uname -m            # aarch64
```
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

Deeper graphics checks (`eglinfo`, Smithay's `anvil`, vello demo) are task 0.5, not part of this setup.

---

## Record what you actually did
This doc is accepted when it matches reality. When you finish, edit the sections above where you
deviated, and fill in:

| Item | Result |
|---|---|
| macOS / Parallels / OrbStack versions | |
| `neueos-arm64`: created, `cargo xtask linux cargo check` passes | |
| `neueos-amd64`: created, `cargo xtask linux-amd64 cargo check` passes | |
| archboot ISO file name and version used | |
| Parallels install route (archboot, or fallback) | |
| Keyboard worked in installer without the Cmd+Tab workaround? | |
| `/dev/dri` contents in the VM | |
| Parallels Tools tried? Shared folders usable? | |
| Anything in this doc that was wrong | |
