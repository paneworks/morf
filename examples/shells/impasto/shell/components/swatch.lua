-- Two small pictures of a look: a desktop widget style (StyleSwatch) and a
-- palette as a two by two grid (ColorSwatch).

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")

local C = theme.color
local M = {}

local function val(v) if type(v) == "function" then return v() end return v end

--- One of the four desktop widget styles: the capsule in the given ink,
--- with a two-letter type sample. `style` capsule | bare | outline |
--- accent, `ink()` `{ text, ground, border }` (default: the palette's),
--- `factor` the scale (34 by 26 at one).
function M.style(values)
  local style = values.style or "capsule"
  local factor = values.factor or 1
  local on_picture = style == "bare" or style == "outline"
  local ink = function()
    local given = val(values.ink)
    if given then return given end
    if style == "accent" then
      return { text = C.accentText(), ground = C.accent(), border = C.accent() }
    end
    return { text = C.text(), ground = C.island, border = C.islandBorder }
  end
  return ui.Rect {
    anchors = values.anchors,
    width = math.floor(34 * factor + 0.5), height = math.floor(26 * factor + 0.5),
    radius = 7 * factor,
    color = function() return on_picture and "#00000000" or ink().ground end,
    border_width = (style == "accent" or style == "bare") and 0 or 1,
    border_color = function()
      local i = ink()
      if style == "outline" then
        local text = type(i.text) == "string" and morf.color(i.text) or i.text
        return text:alpha(0.55)
      end
      if style == "capsule" then return i.border end
      return "#00000000"
    end,
    kit.text {
      anchors = { center_in = true }, text = "Aa",
      size = math.floor(11 * factor + 0.5), weight = 600,
      color = function() return ink().text end,
    },
  }
end

--- A palette as a two by two grid of squares. `colors()` a list of colours.
function M.colors(values)
  local size = values.size or 12
  local cells = { columns = 2, gap = 3, anchors = values.anchors }
  for i = 1, 4 do
    cells[i] = ui.Rect {
      width = size, height = size, radius = values.radius or 3,
      color = function()
        local list = val(values.colors) or {}
        return list[i] or C.surface()
      end,
      border_width = 1, border_color = C.hairline,
      behavior = { color = { duration = 260 } },
    }
  end
  return ui.Grid(cells)
end

return M
