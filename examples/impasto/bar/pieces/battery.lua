-- The battery chip on the bar: its mark and figure, and a click that opens
-- its detail in the island (bar/modules/battery.lua).
local bar = require("bar.bar")
local chip = require("bar.modules.chip")

bar.register("battery", { build = function(_, look) return chip.piece("battery", look) end })
