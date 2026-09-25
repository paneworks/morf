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
      -- "wallpaper", or a colour to build the scheme from.
      source = "wallpaper",
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
    },
    dashboard = {
      -- Opens when the pointer reaches the top edge over it.
      hover = true,
    },
    launcher = {
      max_shown = 7,
      action_prefix = ">",
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
