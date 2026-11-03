# Arch Linux ARM VM in Parallels (research, 2026-11-03)

Tags: **[V]** read in a source, **[I]** inference, **[?]** not found, verify by hand.

## Recommendation
Use the **archboot aarch64 ISO** in a Parallels "Other Linux" ARM VM, with systemd-boot, the
ALARM `linux-aarch64` kernel, virtio-gpu, and **SSH + rsync** for files. Don't depend on
Parallels Tools. Backup route: ALARM generic rootfs tarball bootstrapped from a Fedora/Debian
arm64 live ISO (manual partition, chroot, bootloader; a 2022 report says a kernel update broke
boot until downgraded [V, old]).

- archboot lists Parallels, UTM and VMware on M1+ as supported. aarch64 is UEFI-only, and `setup`
  offers GRUB, systemd-boot, Limine, rEFInd or a UKI [V]. A user booted it natively on
  Parallels 26.1.1; x86_64 EFI images don't boot there [V].
- Quirk: the keyboard may stop working in the installer; Cmd+Tab out and back [V].
- Asahi-style or VM-conversion routes: nothing relevant found [?]. Old prebuilt `.pvm` images are likely stale [I].

## Step outline
1. Download the ISO from release.archboot.com/aarch64/latest and check the b2sum.
2. New Parallels VM from the ISO, "Other Linux", 4+ CPUs, 6+ GB RAM, 40+ GB disk, 3D acceleration on.
3. Run `setup`: GPT with ESP and ext4 root, install, configure, systemd-boot.
4. After first boot, install `openssh seatd mesa libinput vulkan-swrast base-devel rustup`.
5. `cargo xtask vm sync` rsyncs over SSH, builds in the guest, and runs the compositor on a TTY via seatd.

## Parallels Tools
- 26.1 (Oct 2025) ships "driverless" Tools on virtio-vsock, meant to survive kernel updates [V].
  Shared folders need `CONFIG_VIRTIO_VSOCKETS` / `vmw_vsock_virtio_transport` [V, forum]. Whether it works on Arch is unconfirmed [?].
- The old module-based Tools failed on Arch aarch64 kernel 6.12 (`prl_tg` missing) [V, Dec 2024].
- Wayland sessions lost dynamic resolution and DPI integration [V, 20.4 era, no public fix]. We run
  our own DRM compositor, so these won't apply anyway [I].

## Expected GPU situation
- Parallels says VirGL (virtio-gpu 3D) is on by default for Linux ARM VMs and works without Tools.
  Check `dmesg | grep virgl` for `+virgl` and `glxinfo` for a `virgl` renderer [V, KB 128518]. An Arch guest confirmed it [V, older].
- Expect `/dev/dri/card0` and `renderD128` [I], hardware GL/GLES via virgl, and **no hardware
  Vulkan** (no Venus mention) [?]. wgpu should use `lavapipe` or its GL backend [I].
  Virgl broke with some kernel/Mesa pairs (Fedora, anecdotal) [V]; fall back to llvmpipe with `LIBGL_ALWAYS_SOFTWARE=1` [I].
- Atomic modesetting and cursor planes on Parallels' virtio-gpu: nothing found [?]. Upstream
  virtio-gpu supports both [I]. Test Smithay's DRM backend for page-flip/vblank timing and cursor handling.

## Input and display
- libinput should see virtio/USB-HID devices [I].
- A bare DRM compositor must handle mode changes itself; whether a window resize raises a DRM
  hotplug uevent is unverified [?]. HiDPI: use our own output scale setting [I].

## Fallback: UTM (QEMU)
- archboot lists UTM as supported (QEMU backend, not Apple Virtualization) [V]. `virtio-gpu-gl-pci`
  gives virgl via ANGLE/Metal; one M1 Pro openSUSE guest reported accelerated GL, no HiDPI [V, anecdotal].
  Free and scriptable (`utmctl`), likely slower and less integrated than Parallels [I].

## OrbStack
- Arch is supported [V]. `orb create arch neue-arm`; Intel via Rosetta: `orb create --arch amd64 arch neue-amd64`
  (`--memory/--cpus/--disk` exist [V]; the `arch` name is inferred [I]). Arch lacks multi-arch,
  so use one machine per architecture [V].
- Run a command: `orb -m neue-arm <cmd...>` or `orbctl run`. Mac files are at `/mnt/mac`; `mac <cmd>` runs on macOS [V].
  Exit-code, stdin and cwd behaviour are undocumented [?].
- Limits: no GPU/DRI documented; graphics only via XQuartz/Xrdp [V]. Use it for builds and tests,
  not for running the compositor. Arch should use systemd [I].

## Verify by hand
1. Does archboot install cleanly on Parallels 26.x, with a working keyboard?
2. `ls /dev/dri; dmesg | grep virgl; glxinfo -B; vulkaninfo --summary`.
3. Does Smithay's DRM backend run on it (atomic, cursor plane, vblank)?
4. Do Tools shared folders work on Arch, or is rsync enough?
5. Does a window resize send a DRM hotplug event?
6. OrbStack: `orb create arch`, exit codes, `/mnt/mac`.

## Sources
- https://archboot.com/
- https://forum.parallels.com/threads/cannot-boot-arch-based-iso-with-parallels-26.368841/
- https://kb.parallels.com/en/128518
- https://forum.parallels.com/threads/virgil-in-pd-17-1-if-your-graphics-slow-there-is-an-answer.355201
- https://forum.parallels.com/threads/new-vm-kernel-option-needed-in-parallels-desktop-for-mac-26-1-1.368537/
- https://www.omgubuntu.co.uk/2025/10/parallels-desktop-tools-linux-virtio-sock-update
- https://forum.parallels.com/goto/post?id=941967
- https://forum.parallels.com/tags/linux/
- https://forum.endeavouros.com/t/install-endeavouros-arm-aarch64-on-parallel-desktop-mac-m1/21538
- https://forums.opensuse.org/t/cant-get-graphics-acceleration-in-tumbleweed-guest-vm/186200
- https://docs.orbstack.dev/machines
- https://docs.orbstack.dev/machines/distros
- https://docs.orbstack.dev/machines/commands
