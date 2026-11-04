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
