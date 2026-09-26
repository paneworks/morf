-- The launcher button on the bar: the magnifier that opens the launcher.
--
-- One of the buttons of ModuleService.qml; it replaces the placeholder
-- init.lua registers under the same id.

local bar = require("bar.bar")
local door_button = require("components.door_button")

bar.register("launcher", {
  build = function() return door_button.new { glyph = "󰍉", panel = "launcher" } end,
})
