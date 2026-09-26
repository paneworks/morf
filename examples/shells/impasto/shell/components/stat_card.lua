-- A figure, its recent history and whatever detail belongs under it: the
-- sparkline runs behind the reading as the card's background.
--
-- Port of StatCard.qml. `width`, `height` (numbers or bindings), `icon`,
-- `title`, `reading`, `detail`, `series` (a function returning the list),
-- `maximum` (0 scales to the series), `accent`, `bare` (no card: drawn
-- straight on the island), and `extra`: a node laid under the text, with
-- `extra_height` its height, above which the line stops.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local sparkline = require("components.sparkline")

local C = theme.color

local function val(v) if type(v) == "function" then return v() end return v end
local function bind(v) if type(v) == "function" then return v end return function() return v end end

return function(values)
  local width, height = bind(values.width), bind(values.height)
  local accent = values.accent or C.accent
  local extra_h = values.extra_height or 0
  local pad = values.bare and 4 or 14
  local inner_w = function() return width() - 2 * pad end

  local line_h = function()
    return math.max(28, height() * 0.58 - extra_h)
  end
  local line_bottom = extra_h > 0 and (extra_h + 20) or 1

  local children = {
    x = values.x, y = values.y,
    width = width, height = height,
    visible = values.visible,
    radius = theme.radius_medium,
    color = values.bare and "#00000000" or C.islandSurface,
    border_width = values.bare and 0 or 1,
    border_color = C.islandBorder,
    sparkline {
      x = 1, y = function() return height() - line_bottom - line_h() end,
      width = function() return width() - 2 end, height = line_h,
      values = values.series, maximum = values.maximum or 1, stroke = accent,
    },
    ui.Item {
      x = pad, y = values.bare and 2 or 12, width = inner_w, height = 24,
      ui.Row {
        anchors = { left = true, vertical_center = true }, gap = 9, align = "center",
        kit.glyph { glyph = values.icon, size = 15, color = accent },
        kit.text { text = values.title, size = theme.size.small, weight = 600 },
      },
      kit.text {
        anchors = { right = true, vertical_center = true },
        text = values.reading, size = theme.size.large, weight = 600,
      },
    },
    kit.text {
      x = pad, y = values.bare and 28 or 40, width = inner_w, elide = "right",
      text = values.detail, size = theme.size.label, color = C.textMuted,
    },
  }
  if values.extra then
    children[#children + 1] = ui.Item {
      x = pad, y = function() return height() - pad - extra_h end,
      width = inner_w, height = extra_h,
      values.extra,
    }
  end
  return ui.Rect(children)
end
