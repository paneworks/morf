-- The bluetooth chip on the bar: its mark and figure, and a click that opens
-- its detail in the island (bar/modules/bluetooth.lua).
local bar = require("bar.bar")
local chip = require("bar.modules.chip")

bar.register("bluetooth", { build = function() return chip.piece("bluetooth") end })
