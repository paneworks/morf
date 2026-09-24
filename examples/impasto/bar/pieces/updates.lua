-- The pending updates chip on the bar: a count, and a click that opens its
-- detail in the island (bar/modules/updates.lua).
local bar = require("bar.bar")
local chip = require("bar.modules.chip")

bar.register("updates", { build = function() return chip.piece("updates") end })
