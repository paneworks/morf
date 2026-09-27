#!/bin/sh
# lockbox.sh [NAME] -- a nested Hyprland in a window on your own desktop,
# for trying a shell's lock and greeter by hand without locking yourself out.
#
# The lock inside holds only the nested session: close the window and it is
# gone. It asks the real PAM, so a wrong password counts toward faillock,
# as any other would. Inside the window:
#
#   F9    lock again          (morf -c NAME/lock)
#   F10   the greeter         (morf -c NAME/greet; no greetd here, so it
#                              draws and says there is nothing to log in to)
#
# SCREENS=2 (or more) opens that many windows, one output each, to see a
# lock over several screens: the whole of it on one, the rest quiet.
#
# The nested Hyprland reads a config of its own, never ~/.config/hypr.

set -eu
NAME=${1:-caelestia}
MORF=${MORF:-$HOME/.local/bin/morf}
DIR=$(mktemp -d "${TMPDIR:-/tmp}/lockboxXXXX")
# The machine's own libraries for morf, as outside a nix shell: the shell's
# library path and data dirs hide the Vulkan driver.
RUN_MORF="env -u LD_LIBRARY_PATH -u XDG_DATA_DIRS $MORF -c $NAME"

cat > "$DIR/hyprland.lua" <<HYPR
hl.monitor({ output = "", mode = "1920x1080", position = "auto", scale = 1 })
hl.config({
  input = { kb_layout = "us" },
  misc = { force_default_wallpaper = 0, disable_hyprland_logo = true, disable_splash_rendering = true },
  ecosystem = { no_update_news = true, no_donation_nag = true },
})
hl.on("hyprland.start", function()
  hl.exec_cmd("for i in \$(seq 2 ${SCREENS:-1}); do hyprctl output create wayland; done; sleep 1; $RUN_MORF/lock")
end)
hl.bind("F9", function() hl.exec_cmd("$RUN_MORF/lock") end)
hl.bind("F10", function() hl.exec_cmd("$RUN_MORF/greet") end)
HYPR

echo "lockbox: $DIR (log in $DIR/hyprland.log)"
exec env -u HYPRLAND_INSTANCE_SIGNATURE Hyprland --config "$DIR/hyprland.lua" > "$DIR/hyprland.log" 2>&1
