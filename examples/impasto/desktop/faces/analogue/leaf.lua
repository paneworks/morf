-- One leaf of a wall calendar: two rings, the month on an accent ribbon,
-- the day in the notes' ink and the weekday under it. `band` is the
-- ribbon's height; with no day it is a blank sheet, which the month is
-- drawn on.
--
-- Port of Leaf.qml and Paper.qml (the paper is the text colour under the
-- notes' wash, written on in the notes' ink).

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local common = require("desktop.faces.common")
local svg = require("desktop.faces.analogue.svg")

local C = theme.color
local read = common.read
local M = {}

--- `values`: `x`, `y`, `width`, `height`, `ink`, `month`, `day`,
--- `weekday` (strings or functions), `rings` (default true), `band`
--- (default 30), `today` (a function; default true), and children drawn on
--- the sheet.
function M.build(values)
  local w, h, ink = values.width, values.height, values.ink
  local band = values.band or 30
  local pr = theme.paper_radius
  local today = values.today or function() return true end
  local paper = function() return svg.paper(ink) end
  local text = function(v) return function() return tostring(read(v) or "") end end
  local children = {
    ui.Rect { width = w, height = h, radius = pr, color = paper },
    ui.Rect { width = w, height = band + pr, radius = pr,
      color = function() return today() and ink.accent() or ink.muted() end },
    ui.Rect { y = band, width = w, height = pr, color = paper },
    kit.text { x = 6, y = 0, width = w - 12, height = band, horizontal_alignment = "center",
      vertical_alignment = "center", elide = "right", text = text(values.month),
      size = theme.size.label, weight = 600, letter_spacing = 1, color = ink.accentText },
  }
  if values.rings ~= false then
    for _, at in ipairs { 0.3, 0.7 } do
      children[#children + 1] = ui.Rect { x = w * at - 3, y = 0, width = 6, height = 6, radius = 3,
        color = C.paperInk, opacity = 0.5 }
      children[#children + 1] = ui.Rect { x = w * at - 6, y = -7, width = 12, height = 12, radius = 6,
        color = morf.color("transparent"), border_color = ink.text, border_width = 2.5 }
    end
  end
  if values.day then
    local has_weekday = values.weekday ~= nil
    local size = math.floor(math.min(w * 0.5, (h - band) * 0.5) + 0.5)
    children[#children + 1] = kit.text {
      x = 0, width = w, horizontal_alignment = "center",
      y = band + (h - band) / 2 - size * 0.62 - (has_weekday and 6 or 0),
      text = text(values.day), size = size, weight = 700, color = C.paperInk,
    }
    if has_weekday then
      children[#children + 1] = kit.text {
        x = 6, y = h - 10 - 15, width = w - 12, horizontal_alignment = "center", elide = "right",
        text = text(values.weekday), size = theme.size.small, weight = 500, color = C.paperInkMuted,
      }
    end
  end
  for _, child in ipairs(values) do children[#children + 1] = child end
  return ui.Item { x = values.x, y = values.y, width = w, height = h, table.unpack(children) }
end

return M
