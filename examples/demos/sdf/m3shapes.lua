-- Material 3's expressive shapes from lib/m3shapes.lua: every one, and one
-- that morphs through them all.
--
--   morf render examples/demos/sdf/m3shapes.lua -o shapes.png

local ui = require("morf.ui")
local shapes = require("lib.util.m3shapes")

local COLS, CELL = 8, 110
local rows = math.ceil(#shapes.NAMES / COLS) + 1
morf.surface.width = COLS * CELL + 20
morf.surface.height = rows * CELL + 20
morf.surface.anchors = { top = true, left = true }

local accent, ink = "#b6c9ff", "#c4c6d0"

local cells = {}
for i, name in ipairs(shapes.NAMES) do
  local col, row = (i - 1) % COLS, (i - 1) // COLS
  cells[#cells + 1] = ui.Item {
    x = 10 + col * CELL, y = 10 + row * CELL, width = CELL, height = CELL,
    ui.Path {
      x = 25, y = 8, width = 60, height = 60, view_box = { 0, 0, 100, 100 },
      d = shapes.path(name), fill_color = accent,
    },
    ui.Text { x = 0, y = 76, width = CELL, text = name, color = ink, font_size = 12, horizontal_alignment = "center" },
  }
end

-- One shape stepping through the library, a second at a time.
local step = morf.signal("m3shapes.step", 1)
morf.timer(1000, function() step:set(step:get() % #shapes.NAMES + 1) end, true)
cells[#cells + 1] = shapes.Shape {
  x = 10, y = 10 + (rows - 1) * CELL, width = 90, height = 90,
  id = "morphing", shape = function() return shapes.NAMES[step:get()] end,
  color = "#ffb4a8", duration = 500, easing = "out_back",
}

ui.Rect { width = morf.surface.width, height = morf.surface.height, color = "#1a1b21", table.unpack(cells) }
