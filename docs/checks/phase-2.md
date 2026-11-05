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
Since task 2.6 the demo's strip sits at the bottom edge with a selector, and the window
has a "Dialog" button; the coordinates below are from before that change.
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

## 4. Checking popups, clipboard, dialogs and AccessKit (task 2.6)
Runs under a dev build of neue-comp (`docs/dev-setup.md` §1.3), because the layer-popup
fix is in neue-comp. Output at scale 1, ydotoold running, the demo from §3. `click_at X Y`
clicks at absolute logical coordinates (the ydotool quirk of §1.7 halves them). The agent
ran every step on 2026-11-04 and all passed. Orca and accerciser aren't installed in the
VM, so step 5 reads the tree with an AT-SPI client in Python (python-gobject and
at-spi2-core are installed); the owner can repeat it with accerciser.

**1. Sync, build, start neue-comp and the demo:**
```sh
rsync -az --delete --exclude target --exclude .git ./ neueos-vm:neueos/
ssh neueos-vm 'cd neueos && cargo build --release -p neue-comp -p neuectl --no-default-features --features tty && cargo build --release -p neue-platform --examples'
ssh -t neueos-vm 'sudo systemctl stop greetd; true'
ssh neueos-vm 'export XDG_RUNTIME_DIR=/run/user/1000 RUST_LOG=info; unset WAYLAND_DISPLAY
  setsid nohup ~/neueos/target/release/neue-comp --tty >/tmp/comp.log 2>&1 </dev/null &
  sleep 3; . ~/neueos/docs/vm-env.sh; export WGPU_BACKEND=gl RUST_LOG=info,neue_platform=debug
  setsid nohup ~/neueos/target/release/examples/demo >/tmp/demo.log 2>&1 </dev/null &
  sleep 4; neuectl windows; grep -E "WARN|ERROR" /tmp/demo.log | cut -c1-200'
```
Expected: demo window 1 at 440x360+292+150, the strip centred on the bottom edge, no
`WARN` or `ERROR`.

**2. The strip's menu flips above it; the window's menu still drops down:**
```sh
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; export YDOTOOL_SOCKET=/tmp/.ydotool_socket
  click_at() { ydotool mousemove --absolute -x $(($1 / 2)) -y $(($2 / 2)); sleep 0.2; ydotool mousemove -x 1 -y 0; ydotool click 0xC0; }
  click_at 590 743; sleep 1; neuectl screenshot /tmp/g2.png; click_at 553 679; sleep 1
  click_at 511 314; sleep 1; click_at 900 100; sleep 0.5
  grep -oE "popup configured.*|popup dismissed.*|SelectionChanged.*" /tmp/demo.log | cut -c1-120'
scp 'neueos-vm:/tmp/g*.png' ~/Desktop/neueos-shots/
```
Expected: the first popup is configured at `origin=(192.0, -89.0)`, above the strip, and
`g2.png` shows One, Two and Three above the strip. Clicking "Two" logs
`SelectionChanged { selected_content: "Two", index: 1 }`. The window's selector menu is
configured at `(153.0, 184.0)`, below the selector, and the outside click dismisses it.

**3. Copy and paste with foot, both ways, and the primary selection:**
```sh
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; export YDOTOOL_SOCKET=/tmp/.ydotool_socket; rm -f /tmp/pasted
  move_to() { ydotool mousemove --absolute -x $(($1 / 2)) -y $(($2 / 2)); sleep 0.2; ydotool mousemove -x 1 -y 0; sleep 0.1; }
  setsid nohup foot sh -c "echo neuefoot; cat > /tmp/pasted" >/dev/null 2>&1 </dev/null &
  sleep 2; F=$(neuectl --json windows | jq -r ".[] | select(.app_id==\"foot\") | .id")
  neuectl action resize-window --window $F --width 440 --height 160; neuectl action move-window --window $F --x 292 --y 540; sleep 0.5
  move_to 314 564; ydotool click -r 2 -D 60 0xC0; ydotool key 29:1 42:1 46:1 46:0 42:0 29:0; sleep 0.3
  move_to 500 241; ydotool click 0xC0; ydotool key 29:1 47:1 47:0 29:0; sleep 0.5; ydotool key 28:1 28:0; sleep 0.5
  ydotool key 29:1 30:1 30:0 29:0; ydotool type neueclip; ydotool key 29:1 30:1 30:0 29:0 29:1 46:1 46:0 29:0; sleep 0.5
  move_to 500 640; ydotool click 0xC0; ydotool key 29:1 42:1 47:1 47:0 42:0 29:0; sleep 0.5; ydotool key 28:1 28:0; sleep 0.5
  echo "foot got: $(cat /tmp/pasted)"
  move_to 314 564; ydotool click -r 2 -D 60 0xC0; sleep 0.3
  move_to 600 241; ydotool click 0xC0; ydotool key 29:1 30:1 30:0 29:0 14:1 14:0; sleep 0.2
  ydotool click 0xC2; sleep 0.6; ydotool key 28:1 28:0; sleep 0.5
  grep -oE "Entered.*" /tmp/demo.log'
```
Expected:
- The word "neuefoot", selected and copied in foot (Ctrl-Shift-C), pastes into the
  demo's field with Ctrl-V: `Entered("neuefoot")`.
- "neueclip", copied in the demo with Ctrl-C, pastes into foot with Ctrl-Shift-V:
  `foot got: neueclip`.
- With the field emptied, a middle click pastes foot's primary selection: a second
  `Entered("neuefoot")`.

**4. A modal dialog.** Start the demo in step 1 with `WAYLAND_DEBUG=1`, so its log shows
the requests:
```sh
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; export YDOTOOL_SOCKET=/tmp/.ydotool_socket
  move_to() { ydotool mousemove --absolute -x $(($1 / 2)) -y $(($2 / 2)); sleep 0.2; ydotool mousemove -x 1 -y 0; sleep 0.1; }
  move_to 540 359; ydotool click 0xC0; sleep 1.5; neuectl windows
  grep -oE "(xdg_wm_dialog_v1|xdg_dialog_v1)#[0-9]+\.[a-z_]+\([^)]*\)|xdg_toplevel#[0-9]+\.set_parent\([^)]*\)" /tmp/demo.log'
```
Expected:
- "neue-platform: dialog" opens at 300x120, listed as `[child of N]` of the demo window.
- The log has `set_parent`, `get_xdg_dialog` and `xdg_dialog_v1#….set_modal()`.
- Its OK button closes it, and the log shows `xdg_dialog_v1#….destroy()` before the
  toplevel's `destroy()`.

neue-comp only records the modal hint so far; sheets come with Phase 4.

**5. The accessibility tree:**
```sh
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; export DBUS_SESSION_BUS_ADDRESS=unix:path=$XDG_RUNTIME_DIR/bus
  busctl --user set-property org.a11y.Bus /org/a11y/bus org.a11y.Status IsEnabled b true
  pkill -x demo; sleep 0.5; export WGPU_BACKEND=gl
  setsid nohup ~/neueos/target/release/examples/demo >/tmp/demo.log 2>&1 </dev/null &
  sleep 5; timeout 20 python3 - <<"EOF"
import gi
gi.require_version("Atspi", "2.0")
from gi.repository import Atspi
def walk(node, depth=0):
    print("  " * depth + node.get_role_name(), repr(node.get_name()))
    for i in range(node.get_child_count()):
        walk(node.get_child_at_index(i), depth + 1)
    if node.get_role_name() == "button" and node.get_name() == "New window":
        node.get_action_iface().do_action(0)
desktop = Atspi.get_desktop(0)
for i in range(desktop.get_child_count()):
    app = desktop.get_child_at_index(i)
    if app is not None and app.get_name() == "demo":
        walk(app)
EOF
  sleep 1.5; neuectl windows'
```
Expected:
- `application 'demo'` has two frames. The window's frame holds labels,
  `button 'Aqua button'`, an `entry`, a `combo box` with `label 'Linen'`, and the New
  window, Close and Dialog buttons. The strip's frame holds a label and a `combo box`
  with `label 'One'`.
- Pressing "New window" through AT-SPI opens "neue-platform: demo 2".
- Frames have no name yet (ADR 0034).

**6. Done:** give the console back as in `docs/dev-setup.md` §1.3 step 4.

## 5. Checking the widget gallery and the push button (task 2.9)
Runs under a dev build of neue-comp (`docs/dev-setup.md` §1.3), output at scale 1,
ydotoold running. `click_at X Y` is the helper from §4. The agent ran every step on
2026-11-04 and all passed.

**1. Sync, build, start neue-comp and the gallery:**
```sh
rsync -az --delete --exclude target --exclude .git ./ neueos-vm:neueos/
ssh neueos-vm 'cd neueos && cargo build --release -p neue-comp -p neuectl --no-default-features --features tty && cargo build --release -p neue-gallery'
ssh -t neueos-vm 'sudo systemctl stop greetd; true'
ssh neueos-vm 'export XDG_RUNTIME_DIR=/run/user/1000 RUST_LOG=info; unset WAYLAND_DISPLAY
  setsid nohup ~/neueos/target/release/neue-comp --tty >/tmp/comp.log 2>&1 </dev/null &
  sleep 3; . ~/neueos/docs/vm-env.sh; export WGPU_BACKEND=gl RUST_LOG=info
  setsid nohup ~/neueos/target/release/neue-gallery >/tmp/gallery.log 2>&1 </dev/null &
  sleep 4; neuectl windows; grep -E "WARN|ERROR" /tmp/gallery.log; neuectl screenshot /tmp/gal1.png'
```
Expected: "Widget Gallery" at 860x560+82+110, no `WARN` or `ERROR`. `gal1.png` shows the
page list, the switches, and the Push Button page with a blue "OK" default button.

**2. Keyboard: Return, Tab, Space:**
```sh
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; export YDOTOOL_SOCKET=/tmp/.ydotool_socket
  click_at() { ydotool mousemove --absolute -x $(($1 / 2)) -y $(($2 / 2)); sleep 0.2; ydotool mousemove -x 1 -y 0; ydotool click 0xC0; }
  click_at 600 500; sleep 0.3
  ydotool key 28:1 28:0; sleep 0.5; neuectl screenshot /tmp/gal2.png
  ydotool key 15:1 15:0; sleep 0.3; ydotool key 15:1 15:0; sleep 0.3; neuectl screenshot /tmp/gal3.png
  ydotool key 57:1 57:0; sleep 0.5; neuectl screenshot /tmp/gal4.png'
scp 'neueos-vm:/tmp/gal*.png' ~/Desktop/neueos-shots/
```
Expected: with nothing focused, Return shows `Pressed “OK”` (`gal2.png`). Two Tabs put
the focus ring on the "Text" page button (`gal3.png`); Space switches to the Text page
(`gal4.png`).

**3. Switches:** click Appearance (318,141), Accent (438,141) and Zoom (774,141) twice,
then `neuectl screenshot`. Expected: dark appearance, graphite accent, the page at 2×.

**4. The pulse:** back on the Push Button page, take six screenshots 230 ms apart and
compare a pixel of the "OK" button's gel: it brightens and fades (the agent measured
(105,117,134) → (150,158,170) → (117,127,144) with dark graphite at zoom 2×).

**5. Done:** give the console back as in `docs/dev-setup.md` §1.3 step 4.

## 6. Checking text fields and IME (task 2.14)
Steps 1–3 run in the VM, set up as in `docs/dev-setup.md` §1.3 (greetd stopped, ydotoold
started, output at scale 1). The agent ran them on 2026-11-04: all passed. Step 4 is on
the Mac, for the owner.

**1. Build and start the input-method probe, then the gallery on its Fields page:**
```sh
rsync -az --delete --exclude target --exclude .git ./ neueos-vm:neueos/
ssh neueos-vm 'cd neueos && cargo build --release -p neue-comp -p neuectl --no-default-features --features tty && cargo build --release -p neue-gallery -p neue-platform --examples --bins'
ssh neueos-vm 'export XDG_RUNTIME_DIR=/run/user/1000 RUST_LOG=info; unset WAYLAND_DISPLAY
  setsid nohup ~/neueos/target/release/neue-comp --tty >/tmp/comp.log 2>&1 </dev/null &
  sleep 3; . ~/neueos/docs/vm-env.sh
  setsid nohup ~/neueos/target/release/examples/ime_probe neue "neueOS IME" >/tmp/ime.log 2>&1 </dev/null &
  sleep 0.5; export WGPU_BACKEND=gl RUST_LOG=info NEUE_GALLERY_PAGE=3
  setsid nohup ~/neueos/target/release/neue-gallery >/tmp/gallery.log 2>&1 </dev/null &
  sleep 4; neuectl windows; grep -E "WARN|ERROR" /tmp/gallery.log; neuectl screenshot /tmp/f1.png'
```
Expected: "Widget Gallery" at 860x560+82+84, no `WARN` or `ERROR`. `f1.png`: the Fields
page with an empty "Name" field, a disabled "Mountain pass" field, the round search field
with its magnifier, a "Password" field, and a narrow field showing "Longer than" (the
start of its text: unfocused fields aren't scrolled).

**2. IME composition, commit, Return:**
```sh
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; export YDOTOOL_SOCKET=/tmp/.ydotool_socket
  click_in_window 271 168; sleep 0.8; neuectl screenshot /tmp/f2.png
  sleep 2; neuectl screenshot /tmp/f3.png
  ydotool type "!"; sleep 0.3; ydotool key 28:1 28:0; sleep 0.5; neuectl screenshot /tmp/f4.png
  tail -3 /tmp/ime.log'
scp 'neueos-vm:/tmp/f*.png' ~/Desktop/neueos-shots/
```
Expected: `ime.log` ends with `activated`, `preedit neue`, `commit neueOS IME`. `f2.png`:
the Name field focused with "neue" underlined (preedit) and the status line unchanged
("Press a specimen…"). `f3.png`: "neueOS IME" without underline and the status line
`Name: “neueOS IME”`. `f4.png`: `Submitted “neueOS IME!”`.

**3. The secure field bypasses the IME:**
```sh
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; export YDOTOOL_SOCKET=/tmp/.ydotool_socket
  click_in_window 271 324; sleep 2; ydotool type "abc"; sleep 0.5; neuectl screenshot /tmp/f5.png
  tail -1 /tmp/ime.log'
```
Expected: `ime.log` ends with `deactivated` (no new `activated`, no commit into the
field); `f5.png` shows three bullets and `Password: 3 characters`. Give the console back
as in `docs/dev-setup.md` §1.3 step 4.

**4. macOS (owner):** `cargo xtask gallery`, open the Fields page. Add Japanese – Romaji in
System Settings → Keyboard → Text Input, switch to it, click the Name field and type
`nihon`: the preedit is underlined, the candidate window sits under the field, and the
status line doesn't change until you commit (Return), when it shows the committed text.
Japanese glyphs show as boxes (bundled fonts only, ADR 0030). Then on the ABC layout:
Option-E, E gives `é`; in the Password field the input menu greys out (no IME) and
typing shows bullets. Also check Cmd-A/C/X/V, Option-Left/Right (words) and
Cmd-Left/Right (ends) in the Name field, and that Escape clears the search field.

## 7. Checking the text view (task 2.16)
Not run yet (the agent had no VM). Steps 1 and 2 run in the VM, set up as in
`docs/dev-setup.md` §1.3; step 3 is on the Mac. The gallery's "Text View" page is page 4.

**1. Build and open the page:**
```sh
rsync -az --delete --exclude target --exclude .git ./ neueos-vm:neueos/
ssh neueos-vm 'cd neueos && cargo build --release -p neue-comp -p neuectl --no-default-features --features tty && cargo build --release -p neue-gallery -p neue-platform --examples --bins'
ssh neueos-vm 'export XDG_RUNTIME_DIR=/run/user/1000 RUST_LOG=info; unset WAYLAND_DISPLAY
  setsid nohup ~/neueos/target/release/neue-comp --tty >/tmp/comp.log 2>&1 </dev/null &
  sleep 3; . ~/neueos/docs/vm-env.sh
  setsid nohup ~/neueos/target/release/examples/ime_probe neue "neueOS IME" >/tmp/ime.log 2>&1 </dev/null &
  sleep 0.5; export WGPU_BACKEND=gl RUST_LOG=info NEUE_GALLERY_PAGE=4
  setsid nohup ~/neueos/target/release/neue-gallery >/tmp/gallery.log 2>&1 </dev/null &
  sleep 4; neuectl windows; grep -E "WARN|ERROR" /tmp/gallery.log; neuectl screenshot /tmp/t1.png'
scp 'neueos-vm:/tmp/t*.png' ~/Desktop/neueos-shots/
```
Expected: no `WARN` or `ERROR`; `t1.png` shows three text views side by side (editable,
read-only, disabled), each wrapped, with a thin scroller on the editable one only while the
pointer is over it.

**2. Typing, IME and scrolling:** click into the editable view (the first), then:
```sh
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; export YDOTOOL_SOCKET=/tmp/.ydotool_socket
  ydotool key 29:1 107:1 107:0 29:0; sleep 0.3
  ydotool type "typed"; ydotool key 28:1 28:0; ydotool type "more"; sleep 0.5; neuectl screenshot /tmp/t2.png
  tail -3 /tmp/ime.log'
```
(Ctrl-End, type, Return, type.) Expected: `t2.png` shows the view scrolled to its end with the
caret after "more" on a new line and the status line `Notes: “…typed\nmore”`; `ime.log`
ends with `activated`. Then click into the read-only view: typing changes nothing, Cmd/
Ctrl-A selects, and `ime.log` ends with `deactivated`.

**3. macOS (owner):** `cargo xtask gallery`, open "Text View". In the editable view check
Return, Up/Down across wrapped lines, Option-Left/Right (words), Cmd-Left/Right (line ends),
Cmd-Up/Down (text ends), Cmd-A/C/X/V, a triple click (paragraph), that Tab leaves the view,
and the Japanese – Romaji preedit as in §6 step 4 (underlined, candidate window under the
caret, committed on Return, not before).


## 8. Checking menus (task 2.17a)
Not run yet (the agent had no VM). Steps 1–3 run in the VM, set up as in
`docs/dev-setup.md` §1.3; step 4 is on the Mac. The gallery's "Menus" page is page 6.

**1. Build and open the page:**
```sh
rsync -az --delete --exclude target --exclude .git ./ neueos-vm:neueos/
ssh neueos-vm 'cd neueos && cargo build --release -p neue-comp -p neuectl --no-default-features --features tty && cargo build --release -p neue-gallery'
ssh -t neueos-vm 'pgrep -x ydotoold || sudo sh -c "setsid nohup ydotoold --socket-path=/tmp/.ydotool_socket --socket-own=1000:1000 >/dev/null 2>&1 </dev/null &"'
ssh neueos-vm 'export XDG_RUNTIME_DIR=/run/user/1000 RUST_LOG=info; unset WAYLAND_DISPLAY
  setsid nohup ~/neueos/target/release/neue-comp --tty >/tmp/comp.log 2>&1 </dev/null &
  sleep 3; . ~/neueos/docs/vm-env.sh; export WGPU_BACKEND=gl RUST_LOG=info NEUE_GALLERY_PAGE=6
  setsid nohup ~/neueos/target/release/neue-gallery >/tmp/gallery.log 2>&1 </dev/null &
  sleep 4; neuectl windows; grep -E "WARN|ERROR" /tmp/gallery.log; neuectl screenshot /tmp/m1.png'
scp 'neueos-vm:/tmp/m*.png' ~/Desktop/neueos-shots/
```
Expected: no `WARN` or `ERROR`; `m1.png` shows "Right-click here" and a text field.

**2. Keyboard:** click into the text field (its position is in `m1.png`, in screenshot
pixels):
```sh
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; export YDOTOOL_SOCKET=/tmp/.ydotool_socket
  click_at() { ydotool mousemove --absolute -x $(($1 / 2)) -y $(($2 / 2)); sleep 0.2; ydotool mousemove -x 1 -y 0; ydotool click 0xC0; }
  click_at X Y'
```
Then:
```sh
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; export YDOTOOL_SOCKET=/tmp/.ydotool_socket
  ydotool key 42:1 68:1 68:0 42:0; sleep 0.5; neuectl screenshot /tmp/m2.png
  ydotool key 20:1 20:0; ydotool key 106:1 106:0; sleep 0.5; neuectl screenshot /tmp/m3.png
  ydotool key 108:1 108:0 28:1 28:0; sleep 0.5; neuectl screenshot /tmp/m4.png
  grep -E "new Masonry layer|WARN|ERROR" /tmp/gallery.log | tail -5'
```
(Shift-F10; T and Right; Down and Return.) Expected: `m2.png` shows the menu at the field's
middle with Cut highlighted, the field's focus ring gone; `m3.png` shows the Text Size
submenu to the right of the menu, Small highlighted (a second `new Masonry layer` line:
it nests on the first popup); `m4.png` shows both menus closed, the status line
`Pressed “Regular”` and the text field focused again (caret blinking).

**3. Pointer:** with `move_to` and `click_at` as in §4 and positions from `m1.png`,
right-click "Right-click here" (`ydotool click 0xC1`), take `/tmp/m5.png`, move over
"Text Size", take `/tmp/m6.png`, click "Large", take `/tmp/m7.png`. Then right-click again
and click outside the menu, and take `/tmp/m8.png`. Expected: `m5.png` the menu at the
pointer, nothing highlighted; `m6.png` the submenu open beside it; `m7.png` both closed and
`Pressed “Large”`; `m8.png` no menu, the status line unchanged. Right-click near the right
edge of the screen: the menu slides or flips to stay on screen (the submenu flips over its
parent: a known gap, ADR 0045).

**4. macOS (owner):** `cargo xtask gallery`, open "Menus". Right-click (two-finger click)
the area: the menu opens at the pointer; press-drag-release onto an item chooses it;
hovering Text Size opens the submenu; type "w" to jump to Wrap Lines; Escape closes. In the
text field, Shift-F10 opens the menu and Escape gives focus back to the field. Shortcuts
show ⌘ here (⌃ on Linux).

## 9. Checking pop-up buttons, combo boxes and the edit menu (task 2.17b)
Not run yet (the agent had no VM). Steps 1–3 run in the VM, set up as in
`docs/dev-setup.md` §1.3; step 4 is on the Mac. The gallery's "Pop-ups" page is page 7.
The helpers are those of §8.

**1. Build and open the page:** as §8 step 1 with `NEUE_GALLERY_PAGE=7`, screenshot
`/tmp/p1.png`. Expected: no `WARN` or `ERROR`; `p1.png` shows a pop-up button ("Medium"),
a pull-down ("Actions"), disabled copies of both, two combo boxes (the second disabled), a
text field and a text view.

**2. Pop-up button:** click the pop-up button (`click_at X Y` from §8, positions from
`p1.png`) and take `/tmp/p2.png`. Expected: a menu over the button with "Medium" checked,
highlighted and level with where the button was; the button's cap dark. Move the pointer to
"Large" and click: `p3.png` shows the menu gone and the button reading "Large", the status
line `Size: “Large”`. Open it again, press Escape: no change, and the pop-up focus ring is
back after Tab. Wayland: near the top of the screen the popup may be moved down by the
compositor, so the chosen item no longer lines up with the button (a known gap).

**3. Pull-down and combo box:** click "Actions" (`p4.png`: a menu right below it, nothing
highlighted), click "Rename" (`Pressed “Rename”`). Click the combo box's arrow (`p5.png`:
a menu below the field, as wide as the field, the arrow button dark), click "Large": the
field reads "Large". Type into the field: the status line follows. Click into the field,
press Down: the menu opens with its first item highlighted. Right-click the text field
(`p6.png`): Cut, Copy and Select All; Select All selects everything and keeps focus in
the field; the same on the text view. Shift-F10 in the field opens it too.

**4. macOS (owner):** `cargo xtask gallery`, open "Pop-ups". Press and hold the pop-up,
drag to an item, release: it chooses. Click the pop-up, release, click an item. Tab to a
pop-up: Space, Return and Down open it; type "s" while it is closed. Two-finger click a text
field: the edit menu; Cut and Copy put text on the clipboard (paste it into another app);
Paste is missing on purpose (ADR 0046).

## 10. Checking the DBusMenu export (task 2.18)
Not run yet (the agent had no VM). Run in the VM, set up as in `docs/dev-setup.md` §1.3.
The gallery exports a menu bar: View (Dark Appearance, Graphite Accent, Increase
Contrast, Reduce Motion, Next Zoom) and Pages (one radio item per page). Item ids are the
menu model's depth-first ids plus one, so View is 1 and Dark Appearance is 2. The session
bus is the user's (`$XDG_RUNTIME_DIR/bus`); check it exists first.

**1. Build, start, and read the address:**
```sh
rsync -az --delete --exclude target --exclude .git ./ neueos-vm:neueos/
ssh neueos-vm 'cd neueos && cargo build --release -p neue-comp -p neuectl --no-default-features --features tty && cargo build --release -p neue-gallery'
ssh neueos-vm 'export XDG_RUNTIME_DIR=/run/user/1000 RUST_LOG=info; unset WAYLAND_DISPLAY
  test -S $XDG_RUNTIME_DIR/bus || echo "NO SESSION BUS"
  setsid nohup ~/neueos/target/release/neue-comp --tty >/tmp/comp.log 2>&1 </dev/null &
  sleep 3; . ~/neueos/docs/vm-env.sh; export WGPU_BACKEND=gl RUST_LOG=info
  setsid nohup ~/neueos/target/release/neue-gallery >/tmp/gallery.log 2>&1 </dev/null &
  sleep 4; neuectl windows; grep -E "WARN|ERROR" /tmp/gallery.log; neuectl screenshot /tmp/d1.png'
```
Expected: the gallery's line ends in `[menu dbusmenu :1.N /org/neueos/MenuBar/0]`; no
`WARN` or `ERROR` (in particular no "no session bus").

**2. Introspect, read the layout, and choose an item:**
```sh
ssh neueos-vm '. ~/neueos/docs/vm-env.sh
  svc=$(neuectl --json windows | jq -r ".[0].menu.service"); p=/org/neueos/MenuBar/0
  gdbus introspect --session --dest $svc --object-path $p | grep -A3 "interface com.canonical.dbusmenu"
  gdbus call --session --dest $svc --object-path $p --method com.canonical.dbusmenu.GetLayout -- 0 -1 "[]" | cut -c1-400
  gdbus call --session --dest $svc --object-path $p --method com.canonical.dbusmenu.Event -- 2 clicked "<0>" 0
  sleep 1; neuectl screenshot /tmp/d2.png
  gdbus call --session --dest $svc --object-path $p --method com.canonical.dbusmenu.GetLayout -- 1 1 "[\"label\",\"toggle-state\"]"'
scp 'neueos-vm:/tmp/d*.png' ~/Desktop/neueos-shots/
```
Expected: the introspection shows `com.canonical.dbusmenu` with `GetLayout`; the first
layout starts `(uint32 1, (0, …` with `'View'` and `'Pages'` and the page items' `radio`
toggles; `d2.png` shows the gallery in the dark appearance; the last call starts
`(uint32 2,` and Dark Appearance has `'toggle-state': <1>`.

## 11. Checking the table (task 2.19a)
Run by the owner on 2026-11-04: steps 1–4 pass (VM and macOS). `t1.png` showed both tables
as expected; the page's blurb runs past the window's right edge (cosmetic). Steps 1–3 run
in the VM, set up as in
`docs/dev-setup.md` §1.3; step 4 is on the Mac. The gallery's "Tables" page is page 8.
The helpers are those of §8.

**1. Build and open the page:** as §8 step 1 with `NEUE_GALLERY_PAGE=8`, screenshot
`/tmp/t1.png`. Expected: no `WARN` or `ERROR`; `t1.png` shows two bordered tables: files
(Name, Size, Kind; striped; a triangle in the Name title) and tasks (#, Task, Done with
checkboxes).

**2. Selection and keys:** click "Field Notes.txt" (`click_at X Y`, positions from
`t1.png`), then Shift-click "Mixtape.flac" (`ydotool key 42:1`, click, `ydotool key 42:0`)
and take `t2.png`. Expected: the rows between them blue with white text, a focus ring
round the table, the status line `5 rows selected`. Press Down: one row. Type "re": "Recipes.md".
Press Return: `Opened “Recipes.md”`. Double-click a row: the same for that row. Click the
other table: the first one's selection turns grey.

**3. Sort and resize:** click the "Size" title: the triangle moves to it and the rows go
smallest first; click it again: largest first. Drag the divider right of "Name" to the
right (`ydotool mousemove`, press, move, release; the pointer is a column-resize cursor over
it): the column widens, the table scrolls sideways and the titles follow. Wheel the tasks
table to the bottom (10,000 rows): no lag.

**4. macOS (owner):** `cargo xtask gallery`, open "Tables" and repeat steps 2 and 3 with
Cmd-click (adds or removes a row) and Cmd-A. With VoiceOver on (Cmd-F5), Tab to the files
table: it is announced as a table of 10 rows and 3 columns; moving with Down reads the row;
the titles read as column headers, the sorted one with its order.

## 12. Checking the outline view (task 2.19b)
Not run yet. Steps 1–3 run in the VM, set up as in `docs/dev-setup.md` §1.3; step 4 is on
the Mac. The outline is the last section of the gallery's "Tables" page (page 8). The
helpers are those of §8.

**1. Build and open the page:** as §8 step 1 with `NEUE_GALLERY_PAGE=8`, screenshot
`/tmp/o1.png`; scroll the page down (wheel) to the "Outline (folders)" section and take
`/tmp/o2.png`. Expected: no `WARN` or `ERROR`; the outline shows Documents open (Work,
Letters under it), then Pictures, Music, Field Notes.txt and Recipes.md, with triangles on
the folders only and the files' names lined up with the folders'.

**2. Mouse:** click the triangle of Pictures: it opens (Beach Photo.jpg, Screenshot.png),
nothing is selected and the status line doesn't change. Click it again: it closes.
Option-click the triangle of Documents (`ydotool key 56:1`, click, `ydotool key 56:0`):
it closes with everything below; click it plain: only Work and Letters show, both closed.
Option-click again: every level is open. Click "Budget 2026.ods": `1 row selected`, the
row blue with white text. Close Documents by its triangle: Documents turns blue instead
(`1 row selected`).

**3. Keys:** Tab to the outline (or click a row). Press Right on a closed folder: it
opens; Right again: the selection moves to its first child; Left: back to the folder; Left
again: it closes. Option-Right opens every level, Option-Left closes them. Type "mu": Music.
Expected: the focus ring round the table throughout, and the status line counting the
selected rows (it doesn't change when a row only moves because a folder above it opened).

**4. macOS (owner):** `cargo xtask gallery`, open "Tables" and repeat steps 2 and 3. With
VoiceOver on (Cmd-F5), Tab to the outline: it is announced as an outline (a tree grid) with
its rows; folders read as collapsed or expanded with their level, VO-Right/Left (or
VO-Shift-Down) opens and closes a folder.

## 13. Checking the source list (task 2.19c)
Not run yet. Steps 1–3 run in the VM, set up as in `docs/dev-setup.md` §1.3; step 4 is on
the Mac. The page is "Sidebar" (page 9). The helpers are those of §8.

**1. Build and open the page:** as §8 step 1 with `NEUE_GALLERY_PAGE=9`, screenshot
`/tmp/s1.png`. Expected: no `WARN` or `ERROR`; two sidebars, light blue-grey with a line
down the right edge. The left: FAVORITES (Documents selected in grey, Pictures, Music with
a triangle, Starred with the badge 12) and DEVICES (Disk, Recent with the badge 3). The
right: Music open (Albums, Artists), Devices closed. Icons are the same grey as the text;
no row has a stripe, a header row or a border round the list.

**2. Mouse:** click Pictures: the blue gradient with white text, status `Selected
“Pictures”`. Click the FAVORITES title: nothing changes. Move the pointer over the
FAVORITES row: Hide appears at the right end in blue; click it: Documents to Starred
disappear, the selection is gone (`Nothing selected`); click Show: they are back. Move the
pointer to a row: Hide goes away. Click the triangle of Music: Albums and Artists open and
nothing is selected. Double-click Disk: `Opened “Disk”`. Click Starred: its badge turns
white with blue digits.

**3. Keys:** Tab to the list (or click an item). Down and Up step from item to item and
skip DEVICES; Home and End go to the first and last item; Right on Music opens it, Right
again moves to Albums, Left moves back, Left again closes it, and Left on Pictures does
nothing. Type "st": Starred. Switch the window's focus away (Alt-Tab) and the selection
turns grey with dark text; switch back and it is blue again. Expected: no focus ring
anywhere.

**4. macOS (owner):** `cargo xtask gallery`, open "Sidebar" and repeat steps 2 and 3. With
VoiceOver on (Cmd-F5), Tab to the list: it is announced as an outline with its items; items
read their label (and the badge as a description) with their level, groups read as
collapsed or expanded, and VO-Right/Left or VO-Shift-Down opens and closes them.

## 14. Checking the icons (task 2.20a)
Not run yet. Steps 1 and 2 run in the VM, set up as in `docs/dev-setup.md` §1.3; step 3 is
on the Mac. The page is "Icons" (page 10). The helpers are those of §8.

**1. Build and open the page:** as §8 step 1 with `NEUE_GALLERY_PAGE=10`, screenshot
`/tmp/i1.png`. Expected: no `WARN` or `ERROR`; a row of six dark-grey glyphs (folder,
page, picture, note, star, clock) and, under it, a blue disc with a light tick at three
sizes (16, 32, 48 px).

**2. Appearance and scale:** switch to dark (the gallery's appearance switch), screenshot
`/tmp/i2.png`. Expected: the glyphs and ticks turn light, the disc a lighter blue; nothing
else changes. Set the scale to 2x: all of it stays sharp, with no blur at the edges.

**3. macOS (owner):** `cargo xtask gallery`, open "Icons" and repeat step 2. With VoiceOver
on (Cmd-F5), move over the glyphs: each is read as an image with its name ("Folder", ...);
the vector icons are read as "Done".

## 15. Checking the virtual grid (task 2.20b)
Not run yet. Runs in the VM, set up as in `docs/dev-setup.md` §1.3; the helpers are those
of §8. The page is "Scrolling" (the gallery's page for scroll views).

**1. Build and open the page:** as §8 step 1 with the "Scrolling" page's number in
`NEUE_GALLERY_PAGE`, screenshot `/tmp/g1.png`. Expected: no `WARN` or `ERROR`; a fourth
section "Virtual grid (1,000,000 items)" with 4 columns of numbers (1 to 4 in the top
row), 160 px wide.

**2. Scroll far:** wheel over the grid, screenshot `/tmp/g2.png`. Expected: the numbers
climb in steps of 4 per row; the grid stays responsive; the scroller's knob is at its
minimum length.

**3. Resize (owner, macOS or VM):** make the gallery window narrower and wider; the grid
is a fixed 160 px, so check the column count only if a later task makes it fill its width
(2.20c's collection view does).

## 16. Checking the collection view (task 2.20c)
Not run yet. Steps 1 to 3 run in the VM, set up as in `docs/dev-setup.md` §1.3; step 4 is
on the Mac. The page is "Icons" (page 10). The helpers are those of §8.

**1. Build and open the page:** as §8 step 1 with `NEUE_GALLERY_PAGE=10`, screenshot
`/tmp/c1.png`. Expected: no `WARN` or `ERROR`; under the icons, two collections side by
side: 32 px icons (1,000 items, labels like "Documents", "Pictures 7") and 64 px icons
(12 items), labels under the icons, long labels cut with an ellipsis.

**2. Selection and keys:** click an item, press Right, Down, End, Home; Shift-click
another item; Cmd- or Ctrl-click one; type "pic"; press Return; double-click an item;
screenshot after each as `/tmp/c2-N.png`. Expected: the selected item's icon gets a
rounded grey backdrop and its label a blue pill with white text; arrows move over the grid
and scroll the selection into view; Shift selects a range in order, Cmd/Ctrl toggles; the
status line says "N items selected" and `Opened "…"` for Return and a double-click. Click
on the empty area of a short last row: the selection clears.

**3. Focus, size and scale:** Tab away from the collection: the blue pill turns grey with
dark text. Narrow and widen the window: the columns change and the items stay centred in
their cells. Switch to dark and to 2x: all stays sharp.

**4. macOS (owner):** `cargo xtask gallery`, open "Icons" and repeat steps 2 and 3. With
VoiceOver on (Cmd-F5), move into the collection: it is read as a grid with its row and
column counts, and each item as a cell with its label and whether it is selected.

## 17. Checking the split view (task 2.21a)
Not run yet. Steps 1 to 3 run in the VM, set up as in `docs/dev-setup.md` §1.3; step 4 is
on the Mac. The page is "Panes" (page 11). The helpers are those of §8.

**1. Build and open the page:** as §8 step 1 with `NEUE_GALLERY_PAGE=11`, screenshot
`/tmp/s1.png`. Expected: no `WARN` or `ERROR`; three 200 × 80 split views (thin, thick
and stacked) with a "Sidebar" and a "Content" pane, and below them a 420 × 120 split of
three panes with thick dividers.

**2. Drag and collapse:** drag a divider right and left, far past both ends; double-click
it twice; screenshot after each as `/tmp/s2-N.png`. Expected: the sidebar stops at 50
(thin and thick) and at 120; dragged well below 50 it collapses (its label disappears and
the divider sits at the edge); a double-click brings it back at its old length; the
status line reads "Panes: 70 · 129" and so on; the cursor is a resize cursor over a
divider.

**3. Keys:** Tab to a split view (a blue ring around a divider); press Right and Left,
Home, End, Return; in the three-pane split press Down and Up to pick the other divider.
Expected: 10 px steps; Home collapses a collapsible pane; Return restores; the ring
follows the picked divider. Switch to dark and to 2x: all stays sharp.

**4. macOS (owner):** `cargo xtask gallery`, open "Panes" and repeat steps 2 and 3. With
VoiceOver on (Cmd-F5), move to a divider: it is read as a splitter with its value, and
Up and Down (VoiceOver's adjust) move it.


## 18. Checking the tab view (task 2.21b)
Not run yet. Steps 1 to 3 run in the VM, set up as in `docs/dev-setup.md` §1.3; step 4 is
on the Mac. The tab view is the last section of the "Panes" page (page 11). The helpers
are those of §8.

**1. Build and open the page:** as §8 step 1 with `NEUE_GALLERY_PAGE=11`, screenshot
`/tmp/s1.png`. Expected: no `WARN` or `ERROR`; below the split views, two 260 × 110 tab
views: "General", "Advanced" and a greyed "Hidden" tab, and "Fields" and "About".

**2. Click:** click "Advanced", "Hidden", "General"; screenshot after each as
`/tmp/s2-N.png`. Expected: the content follows the selected tab and the selected tab is
blue gel with a white label; "Hidden" does nothing; the status line reads "Tab 2", "Tab 1".

**3. Keys:** Tab to a strip (a blue ring around it); press Right, Left, Home, End. In the
second view click into the "Name" field, then press Ctrl-Tab and Ctrl-Shift-Tab.
Expected: the arrows skip "Hidden"; Ctrl-Tab switches tabs from inside the field, the
focus moves to the strip, and the second tab's content shows. Alt-tab away from the
window: the selected tab turns grey. Switch to dark and to 2x: all stays sharp.

**4. macOS (owner):** `cargo xtask gallery`, open "Panes" and repeat steps 2 and 3. With
VoiceOver on (Cmd-F5), move into a tab view: the tabs are read as a tab group with the
selected one and its position ("2 of 3"), and the adjust gesture (Up and Down) changes tab.
Clicking a tab through VoiceOver is not supported yet (ADR 0054).

## 19. Checking the kit window chrome (task 2.22b)
Not run yet. Steps 1 to 4 run in the VM, set up as in `docs/dev-setup.md` §1.3; step 5 is
on the Mac. The gallery window now draws its own chrome, with the theme switches in a
unified toolbar. Resize edges for client-side windows come with 2.22d; until then the
gallery can't be resized from its edges in the VM. The helpers are those of §8.

**1. Build and open the gallery:** as §8 step 1 with `NEUE_GALLERY_PAGE=0`, screenshot
`/tmp/c1.png`. Expected: no `WARN` or `ERROR`; a single title bar (no compositor bar
above it) of about 60 px with rounded top corners: the traffic lights and "Widget
Gallery" in its top 22 px, the switch buttons under them on the same gradient, a dark
line between the chrome and the content. `grep -i decoration /tmp/comp.log` shows the
gallery asked for client-side decorations.

**2. Traffic lights:** move the pointer over the lights, screenshot `/tmp/c2.png`; hold
the button down on the yellow one, screenshot `/tmp/c3.png`, release on it.
```sh
ssh neueos-vm '. ~/neueos/docs/vm-env.sh; export YDOTOOL_SOCKET=/tmp/.ydotool_socket
  move_to() { ydotool mousemove --absolute -x $(($1 / 2)) -y $(($2 / 2)); sleep 0.2; ydotool mousemove -x 1 -y 0; }
  move_to X Y; sleep 0.3; neuectl screenshot /tmp/c2.png
  move_to X2 Y; ydotool click 0x40; sleep 0.3; neuectl screenshot /tmp/c3.png; ydotool click 0x80
  sleep 0.5; neuectl windows'
```
(X, Y: the close light's centre in `c1.png`, X2 the minimise light's, in screenshot
pixels.) Expected: `c2.png` shows ×, − and + on all three lights; `c3.png` shows the
yellow one darker; after release `neuectl windows --json` shows the gallery with
`"minimized": true`. Bring it back with `neuectl action unminimize-window --window ID`
(ID from `neuectl windows`), click the green light: the window fills the work area
(`"zoomed": true`); click it again: it returns to its size.

**3. Drag and double-click:** drag the bar's empty middle 200 px to the right
(`move_to`, `ydotool click 0x40`, `ydotool mousemove -x 100 -y 0`, `ydotool click 0x80`),
then double-click an empty part of the toolbar row (`ydotool click 0xC0` twice, quickly).
Expected: the window follows the drag (`neuectl windows --json`: x grows by about 200),
the double-click maximises it; a click on a switch button changes the theme and does not
move the window.

**4. Inactive look and close:** open a second window (`foot &`), screenshot
`/tmp/c4.png`; then click the gallery's red light. Expected: in `c4.png` the gallery's
bar, title and lights are pale grey; the red light closes the gallery and
`/tmp/gallery.log` has no `ERROR`. Switch to dark and to 2x first: all stays sharp.

**5. macOS (owner):** `cargo xtask gallery`. Expected: no native title or buttons; our
bar and toolbar under the native shadow and corners. Check that the lights hover, press
and act, that dragging the bar moves the window and a double-click zooms it, and that
the bar turns pale when another app is in front. Note whether the native title-bar area
(the top 28 px) takes the clicks for itself: winit's full-size content view may handle
drag and double-click there natively. With VoiceOver on (Cmd-F5), the three lights are
read as buttons "Close", "Minimise" and "Zoom", and VO-Space on them acts.

## 20. Checking the toolbar items (task 2.22c)
Not run yet. Steps 1 to 3 run in the VM, set up as in `docs/dev-setup.md` §1.3; step 4 is
on the Mac. The page is "Toolbar" (page 12). Its toolbars sit on the page background, not
on the chrome gradient; the same items in a real unified toolbar are in the goldens
`toolbar-items-*`. The helpers are those of §8 and §19 (`move_to`).

**1. Build and open the page:** as §8 step 1 with `NEUE_GALLERY_PAGE=12`, screenshot
`/tmp/t1.png`. Expected: no `WARN` or `ERROR`; a "Display mode" segmented control and
three toolbars: Open, Favourite and a greyed Share (icon over label), a separator, a
List/Icons segmented control, then a search field at the far right. The second toolbar
shows icons only, the third labels only.

**2. Press and keys:** hold the button down on "Favourite" in the first toolbar,
screenshot `/tmp/t2.png`, release; click greyed "Share". Click "Open" (it takes focus),
press Tab once (`ydotool key 15:1 15:0`), screenshot `/tmp/t3.png`; Tab again,
`/tmp/t4.png`. The buttons send no message here, so this checks looks only. Expected:
`t2.png` shows a dark pill behind "Favourite" while held, and nothing for "Share"; `t3.png`
has the focus ring on "Favourite" (the greyed "Share" and the separator take no focus);
`t4.png` has it on the List/Icons control.

**3. Display mode:** click "Icon only" in the Display mode control, screenshot
`/tmp/t5.png`, then "Text only", `/tmp/t6.png`. Expected: only the first toolbar changes,
and the page below it moves up with it: icons alone (the bar is shorter), then labels
alone (shorter still).

**4. macOS (owner):** `cargo xtask gallery`, page "Toolbar" (click it in the list: the
Cmd shortcuts reach pages 1 to 9 only). With VoiceOver on (Cmd-F5), each toolbar is
read as a toolbar, its buttons by their label (also in the icon-only toolbar), and
VO-Space on one presses it.
