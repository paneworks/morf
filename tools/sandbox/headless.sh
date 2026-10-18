#!/bin/sh
# headless.sh NAME STEPS -- morf straight under a headless sway on the GPU,
# sealed off from the person's session as nested.sh is:
#
#   * a private runtime dir (0700) with its own Wayland and sway sockets;
#   * a private session bus that activates nothing, and no system bus;
#   * a scratch HOME with the XDG dirs inside it.
#
# It stubs no commands, so it is for configurations that do not reach the
# machine (the frame-stall rehearsal). STEPS is a file sourced inside, with
#   ipc ARGS   `morf ipc call ARGS` on the shell under test
#   sw ARGS    swaymsg on the private instance only
#   wait S     sleep
# and OUT, the output folder. Output lands in $WORK/out/headless-NAME/.
#
# Environment: MORF_REPO, MORF_CONFIG (default the frame-stall rehearsal),
# MORF_ENV (NAME=value pairs), SWAY (default `sway` on PATH), VULKAN (the
# wrapper morf runs under, default nixVulkanIntel), RENDER_NODE (default
# the first render node), MODE (default 1920x1080@60Hz), BOOT (s before
# the steps), TIMEOUT, WORK, and HOST_GL=1: the compositor, built against
# its own GL stack, is given the host's NVIDIA EGL and GBM libraries from
# HOST_LIBDIR (default /usr/lib/x86_64-linux-gnu) through a folder of links
# in the private runtime dir, and nothing else of the host's.
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=${MORF_REPO:-$(cd "$HERE/../.." && pwd)}
NAME=$1; STEPS=$2
case "$STEPS" in /*) ;; *) STEPS=$PWD/$STEPS;; esac
WORK=${WORK:-${TMPDIR:-/tmp}/morf-sandbox}
OUT=$WORK/out/headless-$NAME; rm -rf "$OUT"; mkdir -p "$OUT"
SWAY=${SWAY:-$(command -v sway)}
SWAYMSG=$(dirname "$SWAY")/swaymsg
RENDER_NODE=${RENDER_NODE:-$(ls /dev/dri/renderD* | head -1)}
# Always /tmp: a long TMPDIR pushes the socket paths past the unix limit.
RUN=$(mktemp -d /tmp/mhXXXX); chmod 700 "$RUN"
H=$WORK/home-headless; rm -rf "$H"
mkdir -p "$H/.config" "$H/.local/share" "$H/.local/state" "$H/.cache"
printf 'output * bg #101010 solid_color\n' > "$H/sway.conf"

# The host's NVIDIA EGL and GBM libraries, linked one by one.
GL_ENV=""
if [ -n "${HOST_GL:-}" ]; then
  LIBS=${HOST_LIBDIR:-/usr/lib/x86_64-linux-gnu}
  mkdir -p "$RUN/gl/lib" "$RUN/gl/gbm"
  for f in "$LIBS"/libEGL_nvidia.so* "$LIBS"/libGLESv2_nvidia.so* "$LIBS"/libnvidia-*.so*; do
    [ -e "$f" ] && ln -s "$f" "$RUN/gl/lib/"
  done
  ln -s "$LIBS/gbm/nvidia-drm_gbm.so" "$RUN/gl/gbm/"
  GL_ENV="LD_LIBRARY_PATH=$RUN/gl/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH} GBM_BACKENDS_PATH=$RUN/gl/gbm"
  GL_ENV="$GL_ENV __EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/10_nvidia.json"
  GL_ENV="$GL_ENV __EGL_EXTERNAL_PLATFORM_CONFIG_DIRS=/usr/share/egl/egl_external_platform.d"
fi

cat > "$OUT/inner.sh" <<INNER
#!/bin/sh
set -u
OUT=$OUT
[ "\$XDG_RUNTIME_DIR" = "$RUN" ] || { echo "REFUSING: runtime \$XDG_RUNTIME_DIR" > \$OUT/refused; exit 1; }
[ "\$DBUS_SYSTEM_BUS_ADDRESS" = "unix:path=$RUN/no-system-bus" ] || { echo "REFUSING: system bus" > \$OUT/refused; exit 1; }
env $GL_ENV WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDER_DRM_DEVICE=$RENDER_NODE \
  "$SWAY" --unsupported-gpu -c "$H/sway.conf" > \$OUT/sway.log 2>&1 &
SP=\$!
for i in \$(seq 100); do ls "$RUN"/sway-ipc.*.sock >/dev/null 2>&1 && break; sleep 0.1; done
export SWAYSOCK=\$(ls "$RUN"/sway-ipc.*.sock | head -1)
export WAYLAND_DISPLAY=\$(cd "$RUN" && ls -d wayland-[0-9] | head -1)
sw() { "$SWAYMSG" "\$@" >> \$OUT/sway-ipc.log 2>&1; }
sw output HEADLESS-1 mode ${MODE:-1920x1080@60Hz}
cd "$REPO"
env MORF_TEST_HOOKS=1 ${MORF_ENV:-} ${VULKAN:-nixVulkanIntel} "$REPO/target/release/morf" \
  ${MORF_CONFIG:-examples/demos/motion/frame-stall.lua} > \$OUT/shell.log 2>&1 &
MP=\$!
ipc() { timeout 10 "$REPO/target/release/morf" ipc call "\$@" >> \$OUT/hc.log 2>&1; }
wait() { sleep "\$1"; }
sleep ${BOOT:-3}
. "$STEPS"
kill \$MP 2>/dev/null; sleep 1; kill \$SP 2>/dev/null
INNER
chmod +x "$OUT/inner.sh"
env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET -u DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE -u SWAYSOCK -u I3SOCK -u NIRI_SOCKET \
  HOME="$H" XDG_CONFIG_HOME="$H/.config" XDG_DATA_HOME="$H/.local/share" XDG_STATE_HOME="$H/.local/state" XDG_CACHE_HOME="$H/.cache" \
  XDG_RUNTIME_DIR="$RUN" DBUS_SYSTEM_BUS_ADDRESS="unix:path=$RUN/no-system-bus" \
  timeout "${TIMEOUT:-240}" dbus-run-session --config-file="$HERE/dbus-session.conf" -- "$OUT/inner.sh" > "$OUT/run.log" 2>&1 || true
rm -rf "$RUN"
cat "$OUT/refused" 2>/dev/null || true
