-- The calendar chip on the bar: its mark and figure, and a click that opens
-- its detail in the island (bar/modules/calendar.lua).
local bar = require("bar.bar")
local chip = require("bar.modules.chip")

bar.register("calendar", { build = function() return chip.piece("calendar") end })
