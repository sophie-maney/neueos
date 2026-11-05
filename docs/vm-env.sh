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
        | "\(.title): top-aligned=\(.y == $p.y) centred=\(($d < 1.01) and ($d > -1.01))"'
}

# D-Bus name the gallery exports its menu bar under.
gallery_menu() {
    neuectl --json windows |
        jq -r 'first(.[] | select(.title == "Widget Gallery")) | .menu.service // empty'
}
