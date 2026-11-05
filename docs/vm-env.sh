# Sourced on the VM over ssh (docs/dev-setup.md §1.4; the VM's shell is bash, so the
# Mac's shell, fish or zsh, never has to parse any of this):
#   ssh neueos-vm '. ~/neueos/docs/vm-env.sh; neuectl windows'
export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-1
export PATH="$HOME/neueos/target/release:$PATH"

# "x,y WxH" of the first window in `neuectl windows`, the format `grim -g` takes.
window_geometry() {
    neuectl --json windows | jq -r '.[0] | "\(.x),\(.y) \(.width)x\(.height)"'
}

# Name of the first output, as `neuectl outputs` lists it.
output_name() {
    neuectl --json outputs | jq -r '.[0].name'
}

# Click at (DX, DY) logical pixels inside the first window (`neuectl windows`), with ydotool
# (daemon as in docs/dev-setup.md §1.4). Halves the target for the `--absolute` quirk
# (§1.7), so it is only exact at output scale 1.
click_in_window() {
    local x y
    read -r x y < <(neuectl --json windows | jq -r '.[0] | "\(.x) \(.y)"')
    export YDOTOOL_SOCKET=/tmp/.ydotool_socket
    ydotool mousemove --absolute -x $(((x + $1) / 2)) -y $(((y + $2) / 2))
    sleep 0.2
    ydotool mousemove -x 1 -y 0
    ydotool click 0xC0
}

# --- Helpers for the owner's runbooks (docs/checks/phase-N-owner.md) ---

# (Re)start the widget gallery on page N (0-based, default 0), with debug logs for the
# platform layer in /tmp/gallery.log. Prints the window list and any WARN or ERROR line.
gallery() {
    pkill -x neue-gallery
    sleep 0.5
    WGPU_BACKEND=gl RUST_LOG=info,neue_platform=debug NEUE_GALLERY_PAGE="${1:-0}" \
        setsid nohup neue-gallery >/tmp/gallery.log 2>&1 </dev/null &
    sleep 4
    neuectl windows
    grep -E "WARN|ERROR" /tmp/gallery.log || echo "gallery log: no WARN or ERROR"
}

# Start a foot terminal (a second window to move focus to).
term() {
    setsid nohup foot "$@" >/dev/null 2>&1 </dev/null &
    sleep 1.5
}

# Id of the first window titled TITLE (default "Widget Gallery").
win_id() {
    neuectl --json windows |
        jq -r --arg t "${1:-Widget Gallery}" 'first(.[] | select(.title == $t) | .id) // empty'
}

# One line per window: id, title, geometry, parent and state.
wins() {
    neuectl --json windows |
        jq -c '.[] | {id, title, x, y, width, height, parent, focused, minimized, zoomed}'
}

# For each child of the gallery (sheet, HUD): whether it hangs from the gallery's top edge
# and is centred on it (within 1 px).
child_check() {
    neuectl --json windows | jq -r '
        (first(.[] | select(.title == "Widget Gallery"))) as $p
        | .[] | select(.parent == $p.id)
        | (.x - ($p.x + ($p.width - .width) / 2)) as $d
        | "\(.title): below-top=\(.y - $p.y) centred=\(($d < 1.01) and ($d > -1.01))"'
}

# D-Bus name the gallery exports its menu bar under.
gallery_menu() {
    neuectl --json windows |
        jq -r 'first(.[] | select(.title == "Widget Gallery")) | .menu.service // empty'
}

# Task 2.30 check: open a menu (`sweep menu`) or a popover (`sweep popover`) in the gallery,
# sweep the pointer over it for SECONDS (default 30) while taking screenshots in a loop,
# then print how many shots show the window see-through (mean luma of a patch of the
# window the popup never covers below 150; the window is about 220, the desktop about 50).
# Needs ydotoold (docs/dev-setup.md §1.4). Output scale 1.
sweep() {
    local kind="${1:-menu}" secs="${2:-30}" x0 x1 y0 y1 end i=0 y shots
    export YDOTOOL_SOCKET=/tmp/.ydotool_socket
    _mv() { ydotool mousemove --absolute -x $(($1 / 2)) -y $(($2 / 2)); ydotool mousemove -x 1 -y 0; }
    case "$kind" in
        menu) gallery 6 >/dev/null; _mv 300 238; ydotool click 0xC1; x0=310 x1=410 y0=245 y1=365 ;;
        popover) gallery 13 >/dev/null; _mv 288 240; ydotool click 0xC0; x0=200 x1=380 y0=262 y1=360 ;;
        *) echo "usage: sweep menu|popover [seconds]"; return 1 ;;
    esac
    rm -rf /tmp/sweep; mkdir -p /tmp/sweep
    end=$((SECONDS + secs))
    ( n=0; while [ $SECONDS -lt $end ]; do n=$((n + 1))
        timeout 5 neuectl screenshot /tmp/sweep/$(printf %05d $n).png >/dev/null 2>&1; done ) &
    shots=$!
    while [ $SECONDS -lt $end ]; do
        for y in $(seq $y0 8 $y1); do
            _mv $((i % 2 ? x0 : x1)) $y; sleep 0.02
        done
        i=$((i + 1))
    done
    wait $shots # not a bare `wait`: the gallery is a background job of this shell too
    ydotool key 1:1 1:0
    ffmpeg -v error -framerate 1 -i /tmp/sweep/%05d.png \
        -vf "crop=300:280:620:360,signalstats,metadata=print:key=lavfi.signalstats.YAVG:file=/tmp/sweep/yavg.txt" \
        -f null - 2>/dev/null
    echo "$kind: $(ls /tmp/sweep/*.png | wc -l) shots, $(grep -o 'YAVG=[0-9.]*' /tmp/sweep/yavg.txt | awk -F= '$2 < 150' | wc -l) see-through"
}
