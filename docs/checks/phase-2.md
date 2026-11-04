# Phase 2 manual checks

The owner's manual check steps for Phase 2 tasks (`docs/plan/phase-2.md`). They follow
`docs/dev-setup.md` §2: everything runs from the Mac over ssh, from the repo root.

## 1. Checking the sctk driver spike (task 2.2)
Runs inside a neue-comp session in the VM: the installed greetd session (log in at the
console), or a dev build started as in `docs/dev-setup.md` §1.3. The output must be at
scale 1 for steps 3–6, because `click_in_window` relies on the ydotool quirk in §1.7. The
agent ran these on 2026-11-04 against the installed session.

**1. Install once** (ydotool and jq are already there if the Phase 1 checks ran):
```sh
ssh -t neueos-vm 'sudo pacman -S --needed ydotool jq'
```

**2. Sync, build, start ydotoold** (as in `docs/dev-setup.md` §1.4):
```sh
rsync -az --delete --exclude target --exclude .git ./ neueos-vm:neueos/
ssh neueos-vm 'cd neueos && cargo build --release -p neue-platform --examples'
ssh -t neueos-vm 'pgrep -x ydotoold || sudo sh -c "setsid nohup ydotoold --socket-path=/tmp/.ydotool_socket --socket-own=1000:1000 >/dev/null 2>&1 </dev/null &"'
```

**3. Start the input-method probe, then the spike.** The probe goes first: neue-comp only
sends text-input `enter` while an input method is bound. fcitx5 can't stand in for it yet,
because neue-comp has no virtual-keyboard protocol.
```sh
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; pkill -x fcitx5; pkill -x ime_probe; pkill -x sctk_spike; sleep 0.5
  setsid nohup ~/neueos/target/release/examples/ime_probe neue "neueOS IME" >/tmp/ime.log 2>&1 </dev/null &
  export WGPU_BACKEND=gl RUST_LOG=info,neue_platform=debug
  setsid nohup ~/neueos/target/release/examples/sctk_spike >/tmp/spike.log 2>&1 </dev/null &
  sleep 3; neuectl windows; grep "wgpu adapter" /tmp/spike.log; neuectl screenshot /tmp/s1.png'
mkdir -p ~/Desktop/neueos-shots && scp 'neueos-vm:/tmp/s*.png' ~/Desktop/neueos-shots/ && open ~/Desktop/neueos-shots
```
Expected: `neuectl windows` lists `org.neueos.SctkSpike` at 420x320, and the log names the
`virgl` adapter with `backend=Gl`. `s1.png` shows a dark-blue strip near the top edge
reading "neueOS layer-shell surface" (the layer-shell surface), and the slate-grey window: a white label, the
Aqua button on linen, a dark text field with the placeholder "Type here (text-input-v3)",
and a "Linen" button.

**4. Open the menu (xdg-popup) and pick an item:**
```sh
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; click_in_window 209 136; sleep 1; neuectl screenshot /tmp/s2.png
  click_in_window 176 200; sleep 1; neuectl screenshot /tmp/s3.png
  grep -E "popup configured|SelectionChanged" /tmp/spike.log | cut -c1-200'
scp 'neueos-vm:/tmp/s*.png' ~/Desktop/neueos-shots/
```
Expected: `s2.png` shows "Linen", "Aqua" and "Graphite" in a dark list right under the
button; `s3.png` shows the list gone and the button reading "Aqua". The log has one
`popup configured origin=(143.0, 156.0) width=90 height=90` and one
`SelectionChanged { selected_content: "Aqua", index: 1 }`.

**5. Dismiss the menu with an outside click.** The first nudge gives the window pointer focus
back: after a client destroys a grabbed popup, neue-comp ignores the first absolute ydotool
motion (`docs/research/masonry-spike.md`).
```sh
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; YDOTOOL_SOCKET=/tmp/.ydotool_socket ydotool mousemove -x 5 -y 0; sleep 0.3
  click_in_window 209 136; sleep 1; click_in_window -150 140; sleep 1
  neuectl screenshot /tmp/s4.png; grep "popup dismissed" /tmp/spike.log | cut -c1-120'
scp 'neueos-vm:/tmp/s*.png' ~/Desktop/neueos-shots/
```
Expected: the log line `popup dismissed by the compositor`, and `s4.png` shows no list,
with the button still reading "Aqua". Clicking the button again (step 4's first command) opens the
list again.

**6. text-input-v3** (a probe preedit, then a commit, 1.5 s apart):
```sh
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; click_in_window 98 90; sleep 0.7; neuectl screenshot /tmp/s5.png
  sleep 1.5; neuectl screenshot /tmp/s6.png; cat /tmp/ime.log; grep text-input /tmp/spike.log | cut -c1-200'
scp 'neueos-vm:/tmp/s*.png' ~/Desktop/neueos-shots/
```
Expected: `s5.png` shows "neue" underlined in the field (the preedit), and `s6.png` shows
"neueOS IME" without the underline (committed). `ime.log` ends with
`preedit neue` and `commit neueOS IME`, and the spike log has `text-input-v3 enter`,
`preedit text=neue` and `commit text=neueOS IME`.

**7. Fractional scale 1.5.** Back up the config, add an output scale (the compositor
reloads on save), then restore it:
```sh
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; cp ~/.config/neueos/compositor.toml /tmp/compositor.toml.bak
  printf "\n[outputs.Virtual-1]\nscale = 1.5\n" >> ~/.config/neueos/compositor.toml; sleep 2
  neuectl outputs; grep "scale changed" /tmp/spike.log | cut -c1-120; neuectl screenshot /tmp/s7.png
  cp /tmp/compositor.toml.bak ~/.config/neueos/compositor.toml'
scp 'neueos-vm:/tmp/s*.png' ~/Desktop/neueos-shots/
```
Expected: `neuectl outputs` shows `scale 1.5`, and the log has
`scale changed window=0 scale=1.5`. In `s7.png` the window is 1.5× larger with sharp text
and button edges. The layer-shell strip is also larger but blurry: neue-comp sends no scale
to layer surfaces yet (a known gap, `docs/research/masonry-spike.md`).

**8. Clean up:**
```sh
ssh neueos-vm 'pkill -x sctk_spike; pkill -x ime_probe; true'
```
