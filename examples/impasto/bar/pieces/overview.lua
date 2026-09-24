-- The overview button on the bar: every workspace side by side.
--
-- One of the buttons of ModuleService.qml; it replaces the placeholder
-- init.lua registers under the same id.

local bar = require("bar.bar")
local door_button = require("components.door_button")

bar.register("overview", {
  build = function() return door_button.new { glyph = "󰕰", panel = "overview" } end,
})
