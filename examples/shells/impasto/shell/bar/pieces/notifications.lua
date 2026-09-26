-- The notifications chip on the bar: its mark and figure, and a click that opens
-- its detail in the island (bar/modules/notifications.lua).
local bar = require("bar.bar")
local chip = require("bar.modules.chip")

bar.register("notifications", { build = function(_, look) return chip.piece("notifications", look) end })
