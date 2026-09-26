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
    launcher = {
      -- Opens when the pointer reaches the bottom edge under it.
      hover = true,
      max_shown = 7,
      action_prefix = ">",
    },
    rail = {
      -- The workspaces down the right edge.
      enabled = true,
      -- How long the numbered bud stays out after a switch, in ms.
      hold = 800,
    },
    services = {
      weather_location = "",
      imperial = true,
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
