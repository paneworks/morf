-- A series over time: oldest on the left, newest on the right, as a line
-- with a dot on the current reading. A trend, not a quantity, so no fill.
--
-- Port of Sparkline.qml, drawn with ui.Path. `values()` is the series,
-- `maximum` a fixed ceiling or 0 to scale to the series' own highest,
-- `width` and `height` numbers or bindings (a path needs a size), `stroke`
-- a colour or a binding, `thickness` the line.

local ui = require("morf.ui")
local theme = require("theme")

local function val(v) if type(v) == "function" then return v() end return v end

return function(values)
  local series = values.values
  local maximum = values.maximum or 1
  local thickness = values.thickness or 1.6
  local stroke = values.stroke or theme.color.accent

  local function ceiling(list)
    if maximum > 0 then return maximum end
    local highest = 0
    for _, v in ipairs(list) do if v > highest then highest = v end end
    return highest > 0 and highest or 1
  end

  local function point(list, index, w, h, top)
    local count = math.max(2, #list)
    local x = (index - 1) / (count - 1) * w
    local fraction = math.max(0, math.min(1, (list[index] or 0) / top))
    -- Half the line's width in from the edges, so the stroke is never cut.
    local inset = thickness / 2 + 1
    return x, inset + (h - 2 * inset) * (1 - fraction)
  end

  local head = function()
    local list = series()
    local w, h = val(values.width) or 0, val(values.height) or 0
    if #list < 2 then return 0, 0 end
    return point(list, #list, w, h, ceiling(list))
  end

  return ui.Item {
    x = values.x, y = values.y, anchors = values.anchors,
    width = values.width, height = values.height,
    visible = values.visible,
    ui.Path {
      x = 0, y = 0, width = values.width, height = values.height,
      visible = function() return #series() > 1 end,
      d = function()
        local list = series()
        local w, h = val(values.width) or 0, val(values.height) or 0
        if #list < 2 or w <= 0 or h <= 0 then return "M0 0" end
        local top = ceiling(list)
        local parts = {}
        for index = 1, #list do
          local x, y = point(list, index, w, h, top)
          parts[#parts + 1] = (index == 1 and "M" or "L") .. ("%.1f %.1f"):format(x, y)
        end
        return table.concat(parts, " ")
      end,
      fill_color = "#00000000",
      stroke_color = stroke, stroke_width = thickness,
      stroke_cap = "round", stroke_join = "round",
    },
    ui.Rect {
      visible = function() return values.dot ~= false and #series() > 1 end,
      x = function() local x = head() return x - 2.5 end,
      y = function() local _, y = head() return y - 2.5 end,
      width = 5, height = 5, radius = 2.5, color = stroke,
    },
  }
end
