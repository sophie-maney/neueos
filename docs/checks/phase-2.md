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
