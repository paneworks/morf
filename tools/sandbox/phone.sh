#!/bin/sh
# phone.sh -- a phone, in a window, to try shells on: a nested Hyprland
# (nested.sh, VISIBLE) the size of a Fairphone 6's screen in logical pixels
# at the scale the shell is meant for there (868 x 1932: its 1116 x 2484
# panel at about 1.29), with whatever morf configuration is under test
# inside it, driven through a FIFO.
#
#   tools/sandbox/phone.sh &             opens it, running the caelestia shell
#   tools/sandbox/phone.sh ctl 'greet'   one command (see phone.steps)
#   tools/sandbox/phone.sh ctl 'shot greet-rest'
#   tools/sandbox/phone.sh ctl quit
#
# Screenshots and logs land in $WORK/out/morf-phone (WORK defaults to
# ~/.cache/morf-phone). Everything inside stays sealed off from the
# person's session, as nested.sh keeps it.
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
export WORK=${WORK:-$HOME/.cache/morf-phone}
CTL=$WORK/ctl
if [ "${1:-}" = ctl ]; then
  [ -p "$CTL" ] || { echo "no bench running (no $CTL)" >&2; exit 1; }
  shift
  printf '%s\n' "$*" > "$CTL"
  exit 0
fi
mkdir -p "$WORK"
rm -f "$CTL"; mkfifo "$CTL"
# BENCH_W x BENCH_H: another size than the phone's (a desk: 1920 1080), in
# a WORK of its own so two benches can run side by side.
BW=${BENCH_W:-868}; BH=${BENCH_H:-1932}
# Its window is floated at that size on the person's compositor: the phone
# by a rule keyed on the nested Hyprland's window class; any other size by
# resizing its own new window once it is there (the rule would catch both).
if command -v hyprctl > /dev/null && [ -n "${HYPRLAND_INSTANCE_SIGNATURE:-}" ]; then
  if [ "$BW" = 868 ] && [ "$BH" = 1932 ]; then
    hyprctl eval 'hl.window_rule({ name = "morf-phone-bench", match = { class = "^(aquamarine)$" }, float = true, size = { "868", "1932" } })' > /dev/null 2>&1 || true
  else
    before=$(hyprctl clients -j | jq -r '.[] | select(.class=="aquamarine") | .address' | sort)
    ( i=0; while [ $i -lt 60 ]; do
        sleep 1; i=$((i+1))
        new=""
        for a in $(hyprctl clients -j | jq -r '.[] | select(.class=="aquamarine") | .address'); do
          echo "$before" | grep -qx "$a" || new=$a
        done
        [ -n "$new" ] || continue
        hyprctl dispatch setfloating "address:$new" > /dev/null
        hyprctl dispatch resizewindowpixel "exact $BW $BH,address:$new" > /dev/null
        break
      done ) &
  fi
fi
export BENCH_CTL=$CTL BENCH_REPO=$(cd "$HERE/../.." && pwd)
VISIBLE=1 NESTED_SCALE=${NESTED_SCALE:-1} POINTER_SIZE="$BW $BH" TIMEOUT=${TIMEOUT:-36000} BOOT=${BOOT:-4} \
  MORF_CONFIG=${MORF_CONFIG:-examples/shells/caelestia/shell/init.lua} \
  MORF_ENV="CAELESTIA_STYLE=${STYLE:-tsugumori} CAELESTIA_DRY_RUN=1" \
  "$HERE/nested.sh" morf phone "$HERE/phone.steps"
rm -f "$CTL"
