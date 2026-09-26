-- The shell's settings: defaults here, what differs in a JSON file under the
-- state directory, every value a signal (lib/settings.lua).
--
-- CAELESTIA_SETTINGS names another file (the tests point it at a scratch
-- one).

local morf = require("morf")
local settings = require("lib.settings")

local path = (morf.env and morf.env("CAELESTIA_SETTINGS")) or morf.state_path("caelestia.json")

return settings.open {
  path = path,
  defaults = {
    theme = {
      -- What the Material scheme is built from: "lule" (the colour
      -- tool's accent -- lule, pywal), "wallpaper", a colour, or "auto"
      -- (lule when it has set anything, else the wallpaper). The tool's
      -- own colours are theme.lule either way.
      source = "auto",
      variant = "tonal_spot",
      mode = "dark",
    },
    appearance = {
      -- A font file every label is set in ("" for the installed faces).
      font_file = "",
    },
    wallpaper = {
      -- A picture to paint under everything; "" reads the path the
      -- caelestia tools keep in ~/.local/state/caelestia/wallpaper/path.txt.
      path = "",
    },
    bar = {
      workspaces = { shown = 5 },
      clock = { twelve_hour = true },
      -- What the Bluetooth popout's "Open settings" starts.
      bluetooth_settings = { "blueman-manager" },
    },
    dashboard = {
      -- Opens when the pointer reaches the top edge over it.
      hover = true,
    },
    sidebar = {
      -- Opens when the pointer reaches the middle of the right edge.
      hover = true,
    },
    leftbar = {
      -- Opens when the pointer reaches the left edge above the rail.
      hover = true,
    },
    capture = {
      -- Opens when the pointer reaches the bottom edge under it.
      hover = true,
      -- Where screenshots and recordings go, both.
      folder = "~/Pictures/Captures",
      -- What each capture runs: `$FILE` is the file to write (in the
      -- folder, named for the time, with its extension), `~/` and `$HOME`
      -- expanded. An empty list runs nothing.
      commands = {
        screenshot_region = { "sh", "-c", "grim -g \"$(slurp)\" \"$0\" && wl-copy < \"$0\"", "$FILE.png" },
        screenshot_window = { "sh", "-c", "grim -g \"$(hyprctl -j activewindow | jq -r '\"\\(.at[0]),\\(.at[1]) \\(.size[0])x\\(.size[1])\"')\" \"$0\" && wl-copy < \"$0\"", "$FILE.png" },
        screenshot_screen = { "sh", "-c", "grim -o \"$(hyprctl -j monitors | jq -r '.[] | select(.focused) | .name')\" \"$0\" && wl-copy < \"$0\"", "$FILE.png" },
        record_region = { "sh", "-c", "gpu-screen-recorder -w region -region \"$(slurp -f '%wx%h+%x+%y')\" -f 60 -o \"$0\"", "$FILE.mp4" },
        record_window = { "gpu-screen-recorder", "-w", "focused", "-f", "60", "-o", "$FILE.mp4" },
        record_screen = { "gpu-screen-recorder", "-w", "screen", "-f", "60", "-o", "$FILE.mp4" },
        record_stop = { "pkill", "-INT", "-f", "gpu-screen-recorder" },
        open = { "xdg-open" },
      },
    },
    launcher = {
      max_shown = 7,
      action_prefix = ">",
    },
    rail = {
      -- The workspaces down the left edge.
      enabled = true,
      -- How long the numbered bud stays out after a switch, in ms.
      hold = 800,
    },
    services = {
      weather_location = "",
      imperial = true,
    },
    utilities = {
      commands = {
        mic_on = { "wpctl", "set-mute", "@DEFAULT_AUDIO_SOURCE@", "0" },
        mic_off = { "wpctl", "set-mute", "@DEFAULT_AUDIO_SOURCE@", "1" },
        settings = {},
        -- Game mode on Hyprland: no animations, blur, gaps or rounding.
        gamemode_on = { "hyprctl", "--batch",
          "keyword animations:enabled 0; keyword decoration:blur:enabled 0; keyword general:gaps_in 0; keyword general:gaps_out 0; keyword decoration:rounding 0" },
        gamemode_off = { "hyprctl", "reload" },
      },
    },
    session = {
      -- What each of the session menu's actions runs (`$USER` is the
      -- user's name).
      commands = {
        logout = { "loginctl", "terminate-user", "$USER" },
        shutdown = { "systemctl", "poweroff" },
        hibernate = { "systemctl", "hibernate" },
        reboot = { "systemctl", "reboot" },
      },
    },
  },
}
