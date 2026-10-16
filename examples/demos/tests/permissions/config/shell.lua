-- A configuration that imports fixture_probe from the spec's runtime path
-- and says what became of it.
local morf = require("morf")
local ui = require("morf.ui")

local probe = require("fixture_probe")

morf.ipc.probe = function() return probe.ran, probe.why end
morf.ipc.alive = function() return "alive" end

ui.Rect { width = 40, height = 40, color = "#336699" }
