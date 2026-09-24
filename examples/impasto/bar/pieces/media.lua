-- The media chip on the bar: its mark and figure, and a click that opens
-- its detail in the island (bar/modules/media.lua).
local bar = require("bar.bar")
local chip = require("bar.modules.chip")

bar.register("media", { build = function() return chip.piece("media") end })
