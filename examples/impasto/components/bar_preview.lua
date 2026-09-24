-- A thumbnail of a screen with the bar on it (BarPreview), showing only what
-- the bar settings change: whether the island touches the top edge, where
-- the sides sit, and whether it is one capsule. The real bar is a layer
-- surface of its own and cannot be put inside a window.

local ui = require("morf.ui")
local theme = require("theme")

local C = theme.color

local function val(v) if type(v) == "function" then return v() end return v end

--- `attached()` and `style()` (grouped | spread | capsule), values or
--- bindings; `anchors` passes through.
return function(values)
  local W, H = 108, 62
  local SW, SH = W - 12, H - 14
  local band, inset, island_w, gap = 7, 4, 22, 3
  local attached = function() return val(values.attached) and true or false end
  local style = function() return val(values.style) or "grouped" end
  local unified = function() return style() == "capsule" end
  local grouped = function() return style() ~= "spread" and style() ~= "capsule" end
  local motion = theme.behave("medium")

  -- A capsule that loses its top corners when it meets the edge.
  local function notched(fields)
    return ui.Rect {
      x = fields.x, width = fields.width,
      y = function() return attached() and 0 or inset end,
      height = function() return band + (attached() and inset or 0) end,
      radius = function() return (band + (attached() and inset or 0)) / 2 end,
      top_left_radius = function() return attached() and 0 or -1 end,
      top_right_radius = function() return attached() and 0 or -1 end,
      color = fields.color, visible = fields.visible,
      behavior = { y = motion, height = motion },
    }
  end

  return ui.Item {
    anchors = values.anchors,
    width = W, height = H,
    ui.ClipRect {
      x = 6, y = 7, width = SW, height = SH, radius = 5,
      color = C.island, border_width = 1, border_color = C.islandBorder,
      notched { x = (SW - 66) / 2, width = 66, color = C.islandSurface, visible = unified },
      ui.Rect {
        visible = function() return not unified() end,
        x = function() return grouped() and (SW - island_w) / 2 - gap - 12 or 6 end,
        y = inset, width = 12, height = band, radius = band / 2, color = C.islandSurface,
        behavior = { x = motion },
      },
      ui.Rect {
        visible = function() return not unified() end,
        x = function() return grouped() and (SW + island_w) / 2 + gap or SW - 6 - 18 end,
        y = inset, width = 18, height = band, radius = band / 2, color = C.islandSurface,
        behavior = { x = motion },
      },
      notched { x = (SW - island_w) / 2, width = island_w, color = C.accent },
    },
  }
end
