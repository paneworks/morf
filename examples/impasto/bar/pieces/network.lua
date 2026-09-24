-- The network chip on the bar: its mark and figure, and a click that opens
-- its detail in the island (bar/modules/network.lua).
local bar = require("bar.bar")
local chip = require("bar.modules.chip")

bar.register("network", { build = function() return chip.piece("network") end })
