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
# SHELL is "morf" (examples/impasto from the repo, or MORF_REPO), "upstream"
# (impasto on Quickshell, from UPSTREAM: a clone of
# github.com/andreumassanet/impasto) or "caelestia" (caelestia-dots/shell, from
# CAELESTIA: a clone, on the Quickshell its flake builds, CAELESTIA_PKG: the
# output of `nix build CAELESTIA#caelestia-shell` -- see README.md). STEPS is
# a file sourced inside, with:
#   open PANEL   the same panel word for either impasto; for caelestia one of
#                its global shortcuts (launcher dashboard sidebar utilities
#                session showall lock nexus ...)
#   close [D]    close whatever is open (Escape; caelestia: drawer D, else
#                every open drawer)
#   point X Y    move the nested pointer to X,Y (hover)
#   click X Y [B] move it there and click B (left, right, middle)
#   scroll N     N wheel steps where the pointer is (negative: up)
#   ipc ARGS     caelestia: `qs ipc call ARGS`, e.g. `ipc drawers toggle osd`
#   notify ARGS  caelestia: a real notify-send, on the private bus only
#   shot LABEL   a screenshot of the nested output
#   film LABEL N N screenshots back to back (timestamps in film-LABEL.times)
#   hc ARGS      hyprctl on the nested instance only (refuses otherwise)
#   k ARGS       wtype on the nested display          wait S   sleep
#
# Environment: WORK (scratch root, default ${TMPDIR:-/tmp}/morf-sandbox),
# UPSTREAM, MORF_REPO, MORF_CONFIG (the configuration the morf kind runs,
# default examples/impasto/init.lua; MORF_ENV adds NAME=value pairs), BOOT (s before the steps), TIMEOUT, WALLPAPER,
# RENDER_NODE (cage renders with GL here; pixman screenshots can be stale),
# AWWW_BIN, INTER_DIR, WTYPE (tools taken from these when not on PATH),
# CAELESTIA, CAELESTIA_PKG, CAEL_CONFIG (a shell.json to seed), CAEL_SCHEME (a
# scheme.json to seed; without one caelestia keeps its built-in palette),
# NIXGL (GL wrapper for the nix-built Quickshell, default nixGLIntel).
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
CAELESTIA=${CAELESTIA:-$WORK/caelestia}
CAELESTIA_PKG=${CAELESTIA_PKG:-$WORK/caelestia-pkg}

# Both shells stand on the same ground: upstream's wallpapers and fonts, and
# the person's own fonts read in place (their icons live in Nerd Fonts).
mkdir -p "$H/.config" "$H/.local/share/fonts" "$H/.local/state" "$H/.cache" "$H/Pictures" "$H/Videos"
if [ -d "$UPHOME/.local/share/wallpapers" ]; then
  cp -r "$UPHOME/.local/share/wallpapers" "$UPHOME/.local/share/impasto" "$H/.local/share/"
  cp -r "$UPHOME/.local/share/fonts/." "$H/.local/share/fonts/"
# caelestia, and a morf configuration other than impasto, bring their own.
elif [ "$KIND" = upstream ] || { [ "$KIND" = morf ] && [ -z "${MORF_CONFIG:-}" ]; }; then
  echo "no upstream impasto clone at $UPSTREAM" >&2; exit 1
fi
[ -d "$HOME/.fonts" ] && ln -s "$HOME/.fonts" "$H/.fonts"
[ -d "$HOME/.local/share/fonts" ] && ln -s "$HOME/.local/share/fonts" "$H/.local/share/fonts/own"
[ -n "${INTER_DIR:-}" ] && cp -r "$INTER_DIR/share/fonts" "$H/.local/share/fonts/inter"
[ -f "$HOME/.config/fontconfig/fonts.conf" ] && mkdir -p "$H/.config/fontconfig" && cp "$HOME/.config/fontconfig/fonts.conf" "$H/.config/fontconfig/"
[ "$KIND" = upstream ] && cp -r "$UPHOME/.config/quickshell" "$H/.config/"
if [ "$KIND" = caelestia ]; then WALLPAPER=${WALLPAPER:-}
else WALLPAPER=${WALLPAPER:-$H/.local/share/wallpapers/japanese-castle-full-moon.jpeg}; fi

mkdir -p "$H/shim"
for c in systemctl loginctl pkexec sudo pkill killall kill kitten reboot poweroff shutdown \
         xdg-open gtk-launch hyprsunset ddcutil brightnessctl nmcli bluetoothctl wpctl pactl \
         playerctl cava notify-send swaync-client makoctl; do
  printf '#!/bin/sh\necho "%s $*" >> "%s/stubbed.log"\nexit 0\n' "$c" "$OUT" > "$H/shim/$c"
  chmod +x "$H/shim/$c"
done

if [ "$KIND" = caelestia ]; then
  [ -f "$CAELESTIA/shell.qml" ] || { echo "no caelestia clone at $CAELESTIA" >&2; exit 1; }
  [ -x "$CAELESTIA_PKG/bin/caelestia-shell" ] || { echo "no caelestia build at $CAELESTIA_PKG" >&2; exit 1; }
  # caelestia's own reach into the machine: its CLI (wallpaper, schemes,
  # recording, screenshots), GPU probes, VPN clients, fingerprint and face
  # unlock probe (it answers "nothing enrolled"), ping, the session commands.
  for c in swappy gpu-screen-recorder nvidia-smi lspci glxinfo warp-cli tailscale \
           netbird wg-quick ping asdbctl fprintd-list logout hibernate suspend app2unit; do
    code=0; [ "$c" = fprintd-list ] && code=1
    printf '#!/bin/sh\necho "%s $*" >> "%s/stubbed.log"\nexit %s\n' "$c" "$OUT" "$code" > "$H/shim/$c"
    chmod +x "$H/shim/$c"
  done
  # Only logged, except that `caelestia wallpaper -f P` records P where the
  # shell reads the current wallpaper, as the real CLI would.
  cat > "$H/shim/caelestia" <<CAEL
#!/bin/sh
echo "caelestia \$*" >> "$OUT/stubbed.log"
[ "\$1 \$2" = "wallpaper -f" ] && printf '%s' "\$3" > "$H/.local/state/caelestia/wallpaper/path.txt"
exit 0
CAEL
  chmod +x "$H/shim/caelestia"
  # Notifications stay real: the shell under test is the notification
  # server, and notify-send only reaches the private bus.
  rm "$H/shim/notify-send"
  mkdir -p "$H/.config/caelestia" "$H/.local/state/caelestia/wallpaper" "$H/Pictures/Wallpapers"
  if [ -n "${CAEL_CONFIG:-}" ]; then cp "$CAEL_CONFIG" "$H/.config/caelestia/shell.json"
  # A fixed weather place, so it never looks the machine up by its address.
  else echo '{ "services": { "weatherLocation": "Reykjavik" } }' > "$H/.config/caelestia/shell.json"; fi
  [ -n "${CAEL_SCHEME:-}" ] && cp "$CAEL_SCHEME" "$H/.local/state/caelestia/scheme.json"
  # Its own wallpaper unless WALLPAPER says otherwise; the switcher lists
  # ~/Pictures/Wallpapers.
  cp "$CAELESTIA/assets/wallpaper.webp" "$H/Pictures/Wallpapers/"
  [ -d "$H/.local/share/wallpapers" ] && cp "$H/.local/share/wallpapers/"* "$H/Pictures/Wallpapers/"
  printf '%s' "${WALLPAPER:-$H/Pictures/Wallpapers/wallpaper.webp}" > "$H/.local/state/caelestia/wallpaper/path.txt"
  # Never the flake's launcher (it puts the real ddcutil, brightnessctl,
  # nmcli first on PATH): its environment minus PATH, and qs under it.
  "$HERE/caelestia-env.sh" "$CAELESTIA_PKG" > "$H/caelestia-env"
fi

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
# No system bus for anyone in here: BlueZ, UPower, NetworkManager and logind
# are the machine's, and a shell under test would read and drive them.
[ "\$DBUS_SYSTEM_BUS_ADDRESS" = "unix:path=\$RUN/no-system-bus" ] || { echo "REFUSING: system bus \$DBUS_SYSTEM_BUS_ADDRESS" > \$OUT/refused; exit 1; }
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
# The pointer is the nested compositor's alone: one virtual pointer on its
# socket for the whole run (vpointer.py), fed through a FIFO.
mkfifo \$RUN/pointer
python3 "$HERE/vpointer.py" \$RUN/pointer 1920 1080 >> \$OUT/input.log 2>&1 &
VP=\$!
pointer() { timeout 5 sh -c 'echo "\$1" > \$2' _ "\$1" \$RUN/pointer; }
point() { asked point; pointer "to \$1 \$2"; }
click() { point \$1 \$2; sleep 0.2; asked click; pointer "click \${3:-left}"; }
scroll() { asked scroll; pointer "scroll \$1"; }
shot() { timeout 20 grim \$OUT/\$1.png 2>>\$OUT/input.log; }
film() {
  # The two clocks side by side once, so morf's log lines (stamped on the
  # monotonic clock) line up with the frames (timed on the wall clock).
  echo "clocks \$(python3 -c 'import time; print(time.clock_gettime_ns(time.CLOCK_MONOTONIC) // 1000000, time.time_ns() // 1000000)')" >> \$OUT/marks
  n=0; t0=\$(date +%s%N)
  echo "film \$1 wall \$((t0 / 1000000))" >> \$OUT/marks
  while [ \$n -lt \$2 ]; do
    timeout 5 grim -t ppm \${FILM_GEOMETRY:+-g "\$FILM_GEOMETRY"} \$OUT/film-\$1-\$(printf %03d \$n).ppm 2>/dev/null
    now=\$(date +%s%N)
    # Since the film began, and since the last open or close was asked for.
    echo "\$n \$(( (now - t0) / 1000000 )) \$(( (now - \${REQ:-t0}) / 1000000 ))" >> \$OUT/film-\$1.times
    n=\$((n+1))
  done
}
wait() { sleep \$1; }
# A media player that plays nothing (fakeplayer.py), on the private bus.
player() { asked player; /usr/bin/python3 "$HERE/fakeplayer.py" "\${1:-\$HOME/.local/share/wallpapers/japanese-castle-full-moon.jpeg}" > \$OUT/player.log 2>&1 & }
# When a request was made: films are timed from it as well.
asked() { REQ=\$(date +%s%N); echo "\$1 wall \$((REQ / 1000000))" >> \$OUT/marks; }
hc output create headless HEADLESS-A >> \$OUT/hc.log 2>&1
sleep 1
if [ "$KIND" != caelestia ]; then
  awww-daemon > \$OUT/awww.log 2>&1 &
  AW=\$!
  sleep 1
  awww img "$WALLPAPER" --transition-type none >> \$OUT/awww.log 2>&1
fi
if [ "$KIND" = caelestia ]; then
  # caelestia paints its own wallpaper. Its Quickshell and Qt come from nix,
  # so it needs nix's GL driver too.
  # The system's data dirs, as a login would have them -- not whatever a
  # dev shell left in XDG_DATA_DIRS -- so the launcher lists the
  # installed applications.
  export XDG_DATA_DIRS=/usr/local/share:/usr/share
  . "$H/caelestia-env"
  \${NIXGL:-nixGLIntel} "\$QS" -p "$CAELESTIA" > \$OUT/shell.log 2>&1 &
  S=\$!
  qsipc() { timeout 10 "\$QS" -p "$CAELESTIA" ipc call "\$@" 2>> \$OUT/hc.log; }
  # Its global shortcuts, as its keybinds send them -- except the launcher,
  # which toggles on the key's release, and a dispatched global only
  # presses: that one goes through its IPC toggle instead.
  open() {
    asked open
    if [ "\$1" = launcher ]; then qsipc drawers toggle launcher >> \$OUT/hc.log
    else hc dispatch "hl.dsp.global(\\"caelestia:\$1\\")" >> \$OUT/hc.log 2>&1; fi
  }
  # Not every drawer closes on Escape (the dashboard closes on leave), so
  # drawers are toggled shut over IPC: the one named (one call, for films),
  # or every open one (found first; the request is stamped after).
  close() {
    if [ -n "\${1:-}" ]; then asked close; qsipc drawers toggle \$1 >> \$OUT/hc.log; return; fi
    open_ones=""
    for d in \$(qsipc drawers list); do
      [ "\$(qsipc drawers isOpen \$d)" = 1 ] && open_ones="\$open_ones \$d"
    done
    asked close
    for d in \$open_ones; do qsipc drawers toggle \$d >> \$OUT/hc.log; done
  }
  ipc() { asked ipc; qsipc "\$@" >> \$OUT/hc.log; }
  notify() { asked notify; timeout 10 /usr/bin/notify-send "\$@" >> \$OUT/hc.log 2>&1; }
elif [ "$KIND" = upstream ]; then
  quickshell -p \$HOME/.config/quickshell > \$OUT/shell.log 2>&1 &
  S=\$!
  open() { asked open; hc dispatch "hl.dsp.global(\\"quickshell:\$1\\")" >> \$OUT/hc.log 2>&1; }
  close() { asked close; k -k Escape; }
else
  cd "$REPO"
  env IMPASTO_LIVE_COMPOSITOR=1 IMPASTO_DRY_RUN=1 ${MORF_ENV:-} nixVulkanIntel "$REPO/target/release/morf" ${MORF_CONFIG:-examples/impasto/init.lua} > \$OUT/shell.log 2>&1 &
  S=\$!
  open() { asked open; timeout 10 "$REPO/target/release/morf" ipc call "\$@" >> \$OUT/hc.log 2>&1; }
  close() { asked close; timeout 10 "$REPO/target/release/morf" ipc call close >> \$OUT/hc.log 2>&1; }
  ipc() { asked ipc; timeout 10 "$REPO/target/release/morf" ipc call "\$@" >> \$OUT/hc.log 2>&1; }
  notify() { asked notify; timeout 10 /usr/bin/notify-send "\$@" >> \$OUT/hc.log 2>&1; }
fi
sleep \${BOOT:-15}
[ -n "$WTYPE" ] && { timeout \${TIMEOUT:-240} "$WTYPE" -s 400000 > /dev/null 2>&1 & KP=\$!; }
. "$STEPS"
kill \$S \${AW:-} 2>/dev/null
sleep 1
kill \$HP 2>/dev/null
sleep 1
kill \$PP \${KP:-} \$VP 2>/dev/null
INNER
chmod +x "$OUT/inner.sh"
env -u HYPRLAND_INSTANCE_SIGNATURE -u NIRI_SOCKET -u SWAYSOCK -u I3SOCK -u WAYLAND_DISPLAY -u WAYLAND_SOCKET -u DISPLAY \
  HOME="$H" XDG_CONFIG_HOME="$H/.config" XDG_DATA_HOME="$H/.local/share" XDG_STATE_HOME="$H/.local/state" XDG_CACHE_HOME="$H/.cache" \
  PATH="$H/shim:${AWWW_BIN:+$AWWW_BIN:}$PATH" XDG_RUNTIME_DIR="$RUN" DBUS_SYSTEM_BUS_ADDRESS="unix:path=$RUN/no-system-bus" \
  WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDER_DRM_DEVICE=${RENDER_NODE:-/dev/dri/renderD128} \
  timeout "${TIMEOUT:-240}" dbus-run-session --config-file="$HERE/dbus-session.conf" -- cage -- "$OUT/inner.sh" > "$OUT/cage.log" 2>&1 || true
rm -rf "$RUN"
cat "$OUT/refused" 2>/dev/null || true
cat "$OUT/env" 2>/dev/null || true
