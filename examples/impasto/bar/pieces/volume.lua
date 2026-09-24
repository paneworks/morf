-- The volume chip on the bar: its mark and figure, and a click that opens
-- its detail in the island (bar/modules/volume.lua).
local bar = require("bar.bar")
local chip = require("bar.modules.chip")

bar.register("volume", { build = function() return chip.piece("volume") end })
