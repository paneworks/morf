-- A desktop theme at tile size: the live clock face on the island's black.
--
-- Port of ThemeSwatch.qml, shared by the inspector (and a settings page when
-- that port lands) so both previews match. The face is built at the 2x2
-- size and scaled down, so it is the very face the desk draws.

local ui = require("morf.ui")
local theme = require("theme")
local desk = require("services.desktop")

local C = theme.color
local M = {}

--- `theme_id` "modern" or "analogue"; `factor` the scale (0.2 by default).
--- `values` may place it (`x`, `y`, `anchors`).
function M.build(theme_id, factor, values)
  values = values or {}
  factor = factor or 0.2
  local side = desk.size_for("2x2").width
  local shown = math.floor(side * factor + 0.5)
  local face = require("desktop.face").build {
    id = "clock", key = "", family = "2x2", theme = theme_id,
    ink = desk.ink_for(nil), row = nil, width = side, height = side,
  }
  return ui.Item {
    x = values.x, y = values.y, anchors = values.anchors,
    width = shown, height = shown,
    ui.Item {
      width = side, height = side, scale = factor,
      transform_origin_x = 0, transform_origin_y = 0,
      ui.Rect {
        anchors = { fill = true }, radius = theme.desktop_radius,
        color = C.island, border_color = C.islandBorder, border_width = 2,
      },
      face,
    },
  }
end

--- The side a swatch at `factor` takes.
function M.side(factor)
  return math.floor(desk.size_for("2x2").width * (factor or 0.2) + 0.5)
end

return M
