-- The system load chip on the bar: its ring and figure, and a click that
-- opens its detail in the island (bar/modules/stats.lua).
local bar = require("bar.bar")
local chip = require("bar.modules.chip")

bar.register("stats", { build = function() return chip.piece("stats") end })
