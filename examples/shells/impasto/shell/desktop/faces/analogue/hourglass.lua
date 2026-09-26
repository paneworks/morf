-- An hourglass with the sand in the timer's blue: what is left in the top
-- bulb, a heap below, and a thread while it runs. `fraction` is what is
-- left.
--
-- Port of Hourglass.qml, as one document keyed on the fraction snapped to a
-- hundredth (a five-minute timer redraws every three seconds, not every
-- frame).

local ui = require("morf.ui")
local theme = require("theme")
local common = require("desktop.faces.common")
local svg = require("desktop.faces.analogue.svg")

local C = theme.color
local M = {}
local n = svg.n

local function doc(size, ink, f, running)
  local text = svg.hex(ink.text())
  local sand = svg.hex(C.indicatorTimer)
  return svg.cached(table.concat({ "glass", size, text, f, tostring(running) }, ":"), function()
    local w, h = size * 0.72, size
    local top, bottom = 8, h - 8
    local cx, cy, neck = w / 2, h / 2, 3
    local level = cy - 8 - (cy - top - 12) * f
    local function half_at(y)
      local k = math.max(0, math.min(1, (y - top) / (cy - top)))
      return (w / 2 - 8) * (1 - k ^ 2.2) + neck
    end
    local heap = (cy - top - 12) * (1 - f) * 0.55
    local parts = {
      string.format('<rect x="0" y="%s" width="%s" height="4" rx="2" fill="%s"/>', n(top - 6), n(w), text),
      string.format('<rect x="0" y="%s" width="%s" height="4" rx="2" fill="%s"/>', n(bottom + 2), n(w), text),
    }
    if f > 0.02 then
      parts[#parts + 1] = string.format('<polygon points="%s,%s %s,%s %s,%s %s,%s" fill="%s"/>',
        n(cx - half_at(level)), n(level), n(cx + half_at(level)), n(level),
        n(cx + neck), n(cy - 2), n(cx - neck), n(cy - 2), sand)
    end
    if f < 0.98 then
      parts[#parts + 1] = string.format('<path d="M 10 %s Q %s %s %s %s Z" fill="%s"/>',
        n(bottom - 3), n(cx), n(bottom - 3 - heap * 2.2), n(w - 10), n(bottom - 3), sand)
    end
    if running and f > 0.02 then
      parts[#parts + 1] = string.format('<rect x="%s" y="%s" width="2" height="%s" fill="%s" fill-opacity="0.7"/>',
        n(cx - 1), n(cy), n(math.max(0, bottom - 3 - heap - cy)), sand)
    end
    parts[#parts + 1] = string.format(
      '<path d="M 6 %s L %s %s Q %s %s %s %s Q %s %s %s %s L 6 %s Q 6 %s %s %s Q 6 %s 6 %s" fill="none" stroke="%s" stroke-width="2.5" stroke-linejoin="round"/>',
      n(top), n(w - 6), n(top), n(w - 6), n(cy - 10), n(cx + neck), n(cy), n(w - 6), n(cy + 10), n(w - 6), n(bottom),
      n(bottom), n(cy + 10), n(cx - neck), n(cy), n(cy - 10), n(top), text)
    return svg.doc(w, h, table.concat(parts))
  end)
end

--- `values`: `size`, `ink`, `fraction` and `running` (functions).
function M.build(values)
  local size, ink = values.size, values.ink
  return ui.Image {
    x = values.x, y = values.y, width = size * 0.72, height = size,
    source = function()
      local f = svg.snap(math.max(0, math.min(1, common.read(values.fraction) or 0)), 0.01)
      return doc(size, ink, f, common.read(values.running) and true or false)
    end,
  }
end

return M
