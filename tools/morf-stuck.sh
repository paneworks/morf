#!/bin/sh
# morf-stuck.sh: run when the shell freezes (from another terminal, or a TTY
# with ctrl+alt+F3), as `sudo tools/morf-stuck.sh`: reading another
# process's stacks needs root where ptrace_scope is 1. Writes
# ~/morf-stuck-TIME.txt: where every thread of every morf is, and which
# drawers it thinks are open. Changes nothing.
home=$HOME
[ -n "${SUDO_USER:-}" ] && home=$(getent passwd "$SUDO_USER" | cut -d: -f6)
out="$home/morf-stuck-$(date +%Y%m%d-%H%M%S).txt"
for pid in $(pgrep -x morf); do
  {
    echo "=== morf $pid: $(tr '\0' ' ' < /proc/$pid/cmdline)"
    echo "--- threads (state, cpu ticks, name)"
    for t in /proc/$pid/task/*; do
      echo "$(awk '{print $3, $14+$15}' "$t/stat") $(cat "$t/comm") $(cat "$t/wchan" 2>/dev/null)"
    done
    echo "--- stacks"
    timeout 20 eu-stack -p "$pid" 2>&1 || timeout 20 gdb -p "$pid" -batch -ex "thread apply all bt" 2>&1
  } >> "$out"
done
{
  echo "=== drawers open"
  if [ -n "${SUDO_USER:-}" ]; then
    uid=$(id -u "$SUDO_USER")
    sudo -u "$SUDO_USER" XDG_RUNTIME_DIR=/run/user/$uid WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-wayland-1} \
      timeout 3 "$home/.local/bin/morf" ipc call drawers 2>&1
  else
    timeout 3 morf ipc call drawers 2>&1
  fi
} >> "$out"
[ -n "${SUDO_USER:-}" ] && chown "$SUDO_USER" "$out"
echo "wrote $out"
