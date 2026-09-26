-- impasto with no screen at all.
--
-- Every screen switched off (from the Displays page, a keybind, a script)
-- leaves Hyprland with no output to offer -- its fallback output is not a
-- wl_output -- and a shell with nothing to draw on. morf still runs this
-- file once, in a runtime with no surface (`morf.surface.outputless`, set in
-- init.lua), and this is all of impasto that starts there: what can bring a
-- screen back. The displays service sees no lit monitor and pushes its
-- recovery rules (services/displays.lua `recover`); the lid lights the panel
-- when it opens; `morf ipc call display <name> on` and `shell reload` work.
-- Once an output is back, the whole shell starts on it and this runtime
-- goes.

require("services.auto.displays")

do
  local lid = require("services.lid")
  lid.start {
    login = require("services.brightness").lib,
    on_change = function(closed) lid.apply(closed) end,
  }
end

morf.ipc.shell = function(verb)
  if verb ~= "reload" then return "usage: morf ipc call shell reload" end
  morf.timer(1, function() morf.reload() end, false)
  return "reloading"
end

morf.ipc.outputless = function() return true end

morf.log("info", "impasto: no screen is offered; waiting for one, and lighting one if none is lit")
