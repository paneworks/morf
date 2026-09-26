-- The brightness chip on the bar: its mark and figure, and a click that opens
-- its detail in the island (bar/modules/brightness.lua).
local bar = require("bar.bar")
local chip = require("bar.modules.chip")

bar.register("brightness", { build = function(_, look) return chip.piece("brightness", look) end })
