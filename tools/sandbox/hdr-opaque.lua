-- Opaque surface: SDR white, sRGB mid-grey, a 2x-bright white, black, and a
-- grey gradient, for the PQ path.
local morf = require("morf")
local ui = require("morf.ui")

morf.surface.width = 800
morf.surface.height = 400
morf.surface.layer = "overlay"
morf.surface.keyboard_focus = "none"
morf.surface.exclusive_zone = -1
morf.surface.opaque = true

ui.Rect {
  width = 800, height = 400, color = "#000000",
  ui.Rect { x = 0, y = 0, width = 200, height = 200, color = "#ffffff" },
  ui.Rect { x = 200, y = 0, width = 200, height = 200, color = "#808080" },
  ui.Rect { x = 400, y = 0, width = 200, height = 200, color = "#ffffff", hdr = 2 },
  ui.Rect { x = 600, y = 0, width = 200, height = 200, color = "#ff0000" },
  ui.Rect {
    x = 0, y = 200, width = 800, height = 200,
    gradient = { angle = 90, space = "srgb", stops = { "#000000", "#ffffff" } },
  },
}
