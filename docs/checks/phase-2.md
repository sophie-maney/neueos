# Phase 2 manual checks

The owner's manual check steps for Phase 2 tasks (`docs/plan/phase-2.md`). They follow
`docs/dev-setup.md` §2: everything runs from the Mac over ssh, from the repo root.

## 1. Checking the sctk driver spike (task 2.2)
Historical: task 2.5 replaced `sctk_spike` with `demo` (§3); these steps no longer build.
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
to layer surfaces yet (task 2.27). Once 2.27 lands, the strip is sharp too.

**8. Clean up:**
```sh
ssh neueos-vm 'pkill -x sctk_spike; pkill -x ime_probe; true'
```

## 2. Checking the winit driver (task 2.4)
Runs natively on the Mac, from the repo root. No VM needed.

**1. Start the demo:**
```sh
RUST_LOG=info,neue_platform=debug cargo run -p neue-platform --example demo
```
Expected: one window, "neue-platform: demo 1", with a label, the Aqua button tile, a
text field, an echo label, a selector and two buttons. Text is sharp (Retina, scale 2). The
log shows `wgpu adapter ... backend=Metal` and `opened window id=window#0`, and no `WARN`.

**2. Typing, clipboard, layers:** click the text field and type `hello`, then press
Return: the label below reads "You typed: hello". Select the text (Shift-Left ×5), Cmd-C,
click into TextEdit and Cmd-V: `hello` appears. Copy something in TextEdit, Cmd-V in the
demo: it is pasted. Click the selector: its menu opens over the window; pick "Aqua". The
I-beam cursor shows over the text field.

**3. IME:** add an input method in System Settings → Keyboard → Text Input (for example
Japanese – Romaji). Switch to it, click the text field and type `nihon`: the preedit is
underlined and the candidate window sits under the caret, not at a screen corner. Press
Space, then Return to commit. With no system fonts (ADR 0030) Japanese glyphs show as boxes;
that is expected, what matters is that preedit and commit arrive (the log shows the text
via the `Return` echo). Also try a dead key on the ABC layout: Option-E then E gives `é`.

**4. VoiceOver:** turn it on with Cmd-F5. Move into the demo window (VO-Right): VoiceOver
reads the labels, "New window, button", "Close, button" and the text field with its
placeholder. Press VO-Space on "New window": a second window opens. Turn VoiceOver off
with Cmd-F5.

**5. Several windows:** in window 2, type into its text field; window 1's field is
unchanged. "Close" in window 2 closes only it. Closing the last window (red button or
"Close") quits the demo.

Report: which steps passed, and the log lines of any step that didn't.

## 3. Checking the Wayland driver (task 2.5)
The same demo as §2, under neue-comp in the VM, set up as in §1 (session running, ydotoold
started, output at scale 1). The agent ran steps 1–4 on 2026-11-04 against the installed
session; the owner ran steps 5 and 6 (cursor shapes, scale 1.5) the same day. All passed.

**1. Sync, build, start the input-method probe and the demo** (the probe first, as in §1
step 3):
```sh
rsync -az --delete --exclude target --exclude .git ./ neueos-vm:neueos/
ssh neueos-vm 'cd neueos && cargo build --release -p neue-platform --examples'
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; pkill -x demo; pkill -x ime_probe; sleep 0.5
  setsid nohup ~/neueos/target/release/examples/ime_probe neue "neueOS IME" >/tmp/ime.log 2>&1 </dev/null &
  export WGPU_BACKEND=gl RUST_LOG=info,neue_platform=debug
  setsid nohup ~/neueos/target/release/examples/demo >/tmp/demo.log 2>&1 </dev/null &
  sleep 3; neuectl windows; grep -E "wgpu adapter|WARN|ERROR" /tmp/demo.log | cut -c1-200; neuectl screenshot /tmp/d1.png'
mkdir -p ~/Desktop/neueos-shots && scp 'neueos-vm:/tmp/d*.png' ~/Desktop/neueos-shots/ && open ~/Desktop/neueos-shots
```
Expected: `neuectl windows` lists `org.neueos.PlatformDemo` "neue-platform: demo 1" at
440x360; the log names the `virgl` adapter with `backend=Gl` and has no `WARN` or `ERROR`.
`d1.png` shows the dark-blue layer-shell strip near the top edge and the window: label,
Aqua button, text field, echo label, "Linen" selector, "New window" and "Close".

**2. IME, typing with key repeat, Return:**
```sh
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; export YDOTOOL_SOCKET=/tmp/.ydotool_socket
  click_in_window 208 91; sleep 2.5
  ydotool key 30:1; sleep 1.2; ydotool key 30:0; sleep 0.3; ydotool key 28:1 28:0; sleep 0.8
  neuectl screenshot /tmp/d2.png; cat /tmp/ime.log; grep Entered /tmp/demo.log | cut -c1-200'
scp 'neueos-vm:/tmp/d*.png' ~/Desktop/neueos-shots/
```
Expected: `ime.log` ends with `preedit neue` and `commit neueOS IME`; holding A for 1.2 s
types a row of `a`s (repeat); Return logs `Entered("neueOS IMEaaaa…")`, and `d2.png` shows
"You typed: neueOS IMEaaaa…".

**3. The selector's menu (xdg-popup):**
```sh
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; click_in_window 219 164; sleep 1; neuectl screenshot /tmp/d3.png
  click_in_window 186 228; sleep 1; grep -E "popup configured|SelectionChanged" /tmp/demo.log | cut -c1-200'
scp 'neueos-vm:/tmp/d*.png' ~/Desktop/neueos-shots/
```
Expected: `d3.png` shows "Linen", "Aqua", "Graphite" under the selector; the log has
`popup configured origin=(153.0, 184.0) width=90 height=90` and
`SelectionChanged { selected_content: "Aqua", index: 1 }`.

**4. Several windows and closing.** Restart the demo first if step 3 ran (after a popup is
destroyed, neue-comp leaves no window focused, so `click_in_window` may miss):
```sh
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; click_in_window 59 209; sleep 1.5; neuectl windows
  W2=$(neuectl --json windows | jq -r ".[] | select(.title==\"neue-platform: demo 2\") | .id")
  neuectl action close-window --window $W2; sleep 1; neuectl windows
  click_in_window 164 209; sleep 1.5; neuectl windows; pgrep -x demo || echo "demo exited"'
```
Expected: "New window" opens "neue-platform: demo 2"; the close action closes only it;
"Close" in window 1 closes it, the strip goes with it, and the demo exits.

**5. Cursor shapes** (owner, with the mouse in the VM window): start the demo as in step
1. Over the text field the pointer is an I-beam, elsewhere an arrow; over the title bar it
is whatever neue-comp draws. `grep "can't set the cursor" /tmp/demo.log` prints nothing.

**6. Fractional scale 1.5** (as §1 step 7, with the demo running):
```sh
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; cp ~/.config/neueos/compositor.toml /tmp/compositor.toml.bak
  printf "\n[outputs.Virtual-1]\nscale = 1.5\n" >> ~/.config/neueos/compositor.toml; sleep 2
  neuectl outputs; grep "scale changed" /tmp/demo.log | cut -c1-120; neuectl screenshot /tmp/d6.png
  cp /tmp/compositor.toml.bak ~/.config/neueos/compositor.toml'
scp 'neueos-vm:/tmp/d*.png' ~/Desktop/neueos-shots/
```
Expected: `scale 1.5` and one `scale changed ... scale=1.5` line for the window. In
`d6.png` the window is 1.5× larger with sharp text. The strip is blurry until task 2.27.

**7. Clean up:**
```sh
ssh neueos-vm 'pkill -x demo; pkill -x ime_probe; true'
```
