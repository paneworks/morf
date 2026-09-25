#!/bin/sh
# nested.sh SHELL NAME STEPS -- a shell under test inside a NESTED Hyprland,
# itself a client of a headless cage, sealed off from the person's session:
#
#   * a private runtime dir (0700) -- its own Wayland and Hyprland sockets;
#   * a private session bus that activates nothing, and no system bus;
#   * a scratch HOME with the XDG dirs inside it;
#   * stub commands, first on PATH, for anything that reaches the machine
#     (power, root, other people's programs, the network and audio stacks):
#     each call is logged in OUT/stubbed.log and does nothing.
#
# SHELL is "morf" (examples/impasto from the repo, or MORF_REPO) or "upstream"
# (impasto on Quickshell, from UPSTREAM: a clone of
# github.com/andreumassanet/impasto). STEPS is a file sourced inside, with:
#   open PANEL   the same panel word for either shell
#   close        close whatever is open
#   shot LABEL   a screenshot of the nested output
#   film LABEL N N screenshots back to back (timestamps in film-LABEL.times)
#   hc ARGS      hyprctl on the nested instance only (refuses otherwise)
#   k ARGS       wtype on the nested display          wait S   sleep
#
# Environment: WORK (scratch root, default ${TMPDIR:-/tmp}/morf-sandbox),
# UPSTREAM, MORF_REPO, BOOT (s before the steps), TIMEOUT, WALLPAPER,
# RENDER_NODE (cage renders with GL here; pixman screenshots can be stale),
# AWWW_BIN, INTER_DIR, WTYPE (tools taken from these when not on PATH).
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=${MORF_REPO:-$(cd "$HERE/../.." && pwd)}
KIND=$1; NAME=$2; STEPS=$3
case "$STEPS" in /*) ;; *) STEPS=$PWD/$STEPS;; esac
WORK=${WORK:-${TMPDIR:-/tmp}/morf-sandbox}
UPSTREAM=${UPSTREAM:-$WORK/impasto}
OUT=$WORK/out/$KIND-$NAME; rm -rf "$OUT"; mkdir -p "$OUT"
H=$WORK/home-$KIND; chmod -R u+w "$H" 2>/dev/null || true; rm -rf "$H"; mkdir -p "$H"
UPHOME=$UPSTREAM/home
WTYPE=${WTYPE:-$(command -v wtype || true)}

# Both shells stand on the same ground: upstream's wallpapers and fonts, and
# the person's own fonts read in place (their icons live in Nerd Fonts).
mkdir -p "$H/.config" "$H/.local/share/fonts" "$H/.local/state" "$H/.cache" "$H/Pictures" "$H/Videos"
cp -r "$UPHOME/.local/share/wallpapers" "$UPHOME/.local/share/impasto" "$H/.local/share/"
cp -r "$UPHOME/.local/share/fonts/." "$H/.local/share/fonts/"
[ -d "$HOME/.fonts" ] && ln -s "$HOME/.fonts" "$H/.fonts"
[ -d "$HOME/.local/share/fonts" ] && ln -s "$HOME/.local/share/fonts" "$H/.local/share/fonts/own"
[ -n "${INTER_DIR:-}" ] && cp -r "$INTER_DIR/share/fonts" "$H/.local/share/fonts/inter"
[ -f "$HOME/.config/fontconfig/fonts.conf" ] && mkdir -p "$H/.config/fontconfig" && cp "$HOME/.config/fontconfig/fonts.conf" "$H/.config/fontconfig/"
[ "$KIND" = upstream ] && cp -r "$UPHOME/.config/quickshell" "$H/.config/"
WALLPAPER=${WALLPAPER:-$H/.local/share/wallpapers/japanese-castle-full-moon.jpeg}

mkdir -p "$H/shim"
for c in systemctl loginctl pkexec sudo pkill killall kill kitten reboot poweroff shutdown \
         xdg-open gtk-launch hyprsunset ddcutil brightnessctl nmcli bluetoothctl wpctl pactl \
         playerctl cava notify-send swaync-client makoctl; do
  printf '#!/bin/sh\necho "%s $*" >> "%s/stubbed.log"\nexit 0\n' "$c" "$OUT" > "$H/shim/$c"
  chmod +x "$H/shim/$c"
done

# A nested Hyprland config of our own -- never the person's, and not
# upstream's either (its autostart starts daemons and a polkit agent) --
# with upstream's look and its desktop blur rule, so both shells get the
# same compositor effects.
cat > "$H/hyprland.lua" <<'HYPR'
hl.monitor({ output = "", mode = "preferred", position = "auto", scale = 1 })
hl.monitor({ output = "HEADLESS-A", mode = "1920x1080@60", position = "0x0", scale = 1 })
hl.config({
  general = { gaps_in = 5, gaps_out = 10, border_size = 2, layout = "dwindle" },
  decoration = { rounding = 22, shadow = { enabled = false },
    blur = { enabled = true, size = 6, passes = 2, xray = true } },
  animations = { enabled = true },
  misc = { force_default_wallpaper = 0, disable_hyprland_logo = true, disable_splash_rendering = true },
  input = { kb_layout = "us", follow_mouse = 1, resolve_binds_by_sym = true },
  ecosystem = { no_update_news = true, no_donation_nag = true },
})
hl.layer_rule({ name = "impasto-desktop-blur", match = { namespace = "^(impasto-desktop)$" },
  blur = true, ignore_alpha = 0.15 })
HYPR

# cage renders with GL on a non-NVIDIA node (its EGL fails on NVIDIA and
# falls back to pixman, whose screenshots can be a frame stale).
if [ -z "${RENDER_NODE:-}" ]; then
  for d in /sys/class/drm/renderD*; do
    [ "$(cat "$d/device/vendor" 2>/dev/null)" = 0x10de ] && continue
    RENDER_NODE=/dev/dri/$(basename "$d"); break
  done
fi
RUN=$(mktemp -d "${TMPDIR:-/tmp}/msXXXX"); chmod 700 "$RUN"
cat > "$OUT/inner.sh" <<INNER
#!/bin/sh
RUN=$RUN; OUT=$OUT
[ "\$XDG_RUNTIME_DIR" = "\$RUN" ] || { echo "REFUSING: runtime \$XDG_RUNTIME_DIR" > \$OUT/refused; exit 1; }
CAGE_DISPLAY=\$WAYLAND_DISPLAY
python3 "$HERE/wlproxy.py" \$RUN/wayland-parent \$RUN/\$CAGE_DISPLAY > \$OUT/proxy.log 2>&1 &
PP=\$!
sleep 0.5
env -u HYPRLAND_INSTANCE_SIGNATURE -u DISPLAY WAYLAND_DISPLAY=wayland-parent \
  LIBSEAT_BACKEND=seatd SEATD_SOCK=\$RUN/no-seatd.sock DBUS_SYSTEM_BUS_ADDRESS=unix:path=\$RUN/no-system-bus \
  Hyprland --config "$H/hyprland.lua" > \$OUT/hyprland.log 2>&1 &
HP=\$!
i=0; SIG=""
while [ \$i -lt 60 ]; do
  for d in \$RUN/hypr/*/; do [ -S "\$d.socket2.sock" ] && SIG=\$(basename \$d); done
  [ -n "\$SIG" ] && break; sleep 0.5; i=\$((i+1))
done
[ -n "\$SIG" ] || { echo "no nested hyprland" > \$OUT/refused; kill \$HP \$PP; exit 1; }
HWL=""; i=0
while [ \$i -lt 40 ] && [ -z "\$HWL" ]; do
  for s in \$RUN/wayland-*; do n=\$(basename \$s); case \$n in *.lock) ;; \$CAGE_DISPLAY|wayland-parent) ;; *) [ -S \$s ] && HWL=\$n;; esac; done
  sleep 0.25; i=\$((i+1))
done
export HYPRLAND_INSTANCE_SIGNATURE=\$SIG WAYLAND_DISPLAY=\$HWL
echo "cage=\$CAGE_DISPLAY nested=\$HWL sig=\$SIG runtime=\$XDG_RUNTIME_DIR" > \$OUT/env
hc() { [ -S "\$RUN/hypr/\$HYPRLAND_INSTANCE_SIGNATURE/.socket.sock" ] || { echo "REFUSING hc" >> \$OUT/refused; return 1; }; timeout 10 hyprctl --instance \$HYPRLAND_INSTANCE_SIGNATURE "\$@"; }
k() { timeout 20 "$WTYPE" "\$@" >> \$OUT/input.log 2>&1; }
shot() { timeout 20 grim \$OUT/\$1.png 2>>\$OUT/input.log; }
film() {
  n=0; t0=\$(date +%s%N)
  while [ \$n -lt \$2 ]; do
    timeout 5 grim -t ppm \${FILM_GEOMETRY:+-g "\$FILM_GEOMETRY"} \$OUT/film-\$1-\$(printf %03d \$n).ppm 2>/dev/null
    echo "\$n \$(( (\$(date +%s%N) - t0) / 1000000 ))" >> \$OUT/film-\$1.times
    n=\$((n+1))
  done
}
wait() { sleep \$1; }
hc output create headless HEADLESS-A >> \$OUT/hc.log 2>&1
sleep 1
awww-daemon > \$OUT/awww.log 2>&1 &
AW=\$!
sleep 1
awww img "$WALLPAPER" --transition-type none >> \$OUT/awww.log 2>&1
if [ "$KIND" = upstream ]; then
  quickshell -p \$HOME/.config/quickshell > \$OUT/shell.log 2>&1 &
  S=\$!
  open() { hc dispatch "hl.dsp.global(\\"quickshell:\$1\\")" >> \$OUT/hc.log 2>&1; }
  close() { k -k Escape; }
else
  cd "$REPO"
  env IMPASTO_LIVE_COMPOSITOR=1 IMPASTO_DRY_RUN=1 nixVulkanIntel "$REPO/target/release/morf" examples/impasto/init.lua > \$OUT/shell.log 2>&1 &
  S=\$!
  open() { timeout 10 "$REPO/target/release/morf" ipc call "\$@" >> \$OUT/hc.log 2>&1; }
  close() { timeout 10 "$REPO/target/release/morf" ipc call close >> \$OUT/hc.log 2>&1; }
fi
sleep \${BOOT:-15}
[ -n "$WTYPE" ] && { timeout \${TIMEOUT:-240} "$WTYPE" -s 400000 > /dev/null 2>&1 & KP=\$!; }
. "$STEPS"
kill \$S \$AW 2>/dev/null
sleep 1
kill \$HP 2>/dev/null
sleep 1
kill \$PP \${KP:-} 2>/dev/null
INNER
chmod +x "$OUT/inner.sh"
env -u HYPRLAND_INSTANCE_SIGNATURE -u NIRI_SOCKET -u SWAYSOCK -u I3SOCK -u WAYLAND_DISPLAY -u WAYLAND_SOCKET -u DISPLAY \
  HOME="$H" XDG_CONFIG_HOME="$H/.config" XDG_DATA_HOME="$H/.local/share" XDG_STATE_HOME="$H/.local/state" XDG_CACHE_HOME="$H/.cache" \
  PATH="$H/shim:${AWWW_BIN:+$AWWW_BIN:}$PATH" XDG_RUNTIME_DIR="$RUN" \
  WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDER_DRM_DEVICE=${RENDER_NODE:-/dev/dri/renderD128} \
  timeout "${TIMEOUT:-240}" dbus-run-session --config-file="$HERE/dbus-session.conf" -- cage -- "$OUT/inner.sh" > "$OUT/cage.log" 2>&1 || true
rm -rf "$RUN"
cat "$OUT/refused" 2>/dev/null || true
cat "$OUT/env" 2>/dev/null || true
