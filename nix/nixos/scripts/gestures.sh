set -euo pipefail

child=
previous=
device=
stop_reader() {
  if [[ -n "$child" ]]; then
    kill "$child" 2>/dev/null || true
    wait "$child" 2>/dev/null || true
    child=
  fi
}
trap stop_reader EXIT
trap 'exit 0' INT TERM

touchscreen() {
  for node in /sys/class/input/event*; do
    # A virtual device used by an application must not replace the phone panel.
    [[ $(readlink -f "$node") != */devices/virtual/* ]] || continue
    local device="/dev/input/${node##*/}"
    [[ -r "$device" ]] || continue
    if udevadm info --query=property --name="$device" | grep -qx ID_INPUT_TOUCHSCREEN=1; then
      printf '%s\n' "$device"
      return
    fi
  done
  return 1
}

while true; do
  if [[ -z "$device" || ! -r "$device" ]]; then
    device=$(touchscreen) || device=
  fi
  geometry=$(hyprctl -j monitors 2>/dev/null | jq -er '
    ([.[] | select(.disabled != true)] | sort_by(.name | startswith("DSI") | not) | .[0])
    | select(. != null)
    | [.width, .height, .transform, .scale] | @tsv') || geometry=
  identity=$(stat -c '%i:%Y' "$device" 2>/dev/null) || identity=
  current="$device:$identity:$geometry"
  if [[ -z "$device" || -z "$geometry" ]]; then
    stop_reader
    previous=
  elif [[ "$current" != "$previous" ]] || ! kill -0 "${child:-0}" 2>/dev/null; then
    stop_reader
    read -r width height transform scale <<< "$geometry"
    # Match lisgd's Wayland transform convention and Morf's 20 logical-pixel edges.
    case "$transform" in 1) orientation=3 ;; 3) orientation=1 ;; *) orientation=$transform ;; esac
    edge_scale=$(jq -n --argjson scale "$scale" '$scale * 20 / 50')
    threshold=$(jq -n --argjson scale "$scale" '80 * $scale | round')
    lisgd -d "$device" -w "$width" -h "$height" -o "$orientation" \
      -s "$edge_scale" -t "$threshold" -r 30 -m 1800 \
      -g "1,RL,B,*,R,$PHONE_GESTURE_ACTION workspace-next" \
      -g "1,LR,B,*,R,$PHONE_GESTURE_ACTION workspace-previous" \
      -g "1,DU,B,*,R,$PHONE_GESTURE_ACTION dashboard" \
      -g "1,UD,T,*,R,$PHONE_GESTURE_ACTION top" \
      -g "2,DU,B,*,R,$PHONE_GESTURE_ACTION keyboard" &
    child=$!
    previous=$current
    printf 'lisgd: %s, %sx%s, scale %s\n' "$device" "$width" "$height" "$scale"
  fi
  # Reopen after hardware resets and follow compositor scale/rotation changes.
  sleep 3
done
