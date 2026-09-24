-- A thermometer: a tube and bulb, accent mercury and five marks. Takes a
-- fraction; the scale is the face's business.
--
-- Port of Thermometer.qml: the glass and marks are one document, the
-- mercury a node whose top follows the fraction.

local ui = require("morf.ui")
local theme = require("theme")
local common = require("desktop.faces.common")
local svg = require("desktop.faces.analogue.svg")

local M = {}
local n = svg.n
local WIDTH, TUBE, BULB, STROKE = 44, 14, 12, 2.5

local function glass_doc(size, ink)
  local text, muted, accent = svg.hex(ink.text()), svg.hex(ink.muted()), svg.hex(ink.accent())
  return svg.cached(table.concat({ "thermo", size, text, muted, accent }, ":"), function()
    local c = WIDTH / 2
    local high, low = 8, size - 2 * BULB - 2
    local parts = {
      string.format('<rect x="%s" y="%s" width="%s" height="%s" rx="%s" fill="none" stroke="%s" stroke-width="%s"/>',
        n((WIDTH - TUBE) / 2 + STROKE / 2), n(STROKE / 2), n(TUBE - STROKE), n(size - 2 * BULB + 6 - STROKE), n(TUBE / 2), text, n(STROKE)),
      string.format('<circle cx="%s" cy="%s" r="%s" fill="none" stroke="%s" stroke-width="%s"/>',
        n(c), n(size - BULB), n(BULB - STROKE / 2), text, n(STROKE)),
      string.format('<circle cx="%s" cy="%s" r="%s" fill="%s"/>', n(c), n(size - BULB), n(BULB - 4), accent),
    }
    for i = 0, 4 do
      parts[#parts + 1] = string.format('<rect x="%s" y="%s" width="%s" height="1.5" rx="1" fill="%s"/>',
        n(c + TUBE / 2 + 5), n(high + (low - high) * i / 4), i % 2 == 0 and 8 or 5, muted)
    end
    return svg.doc(WIDTH, size, table.concat(parts))
  end)
end

--- `values`: `size`, `ink`, `fraction` (function).
function M.build(values)
  local size, ink = values.size, values.ink
  local high, low = 8, size - 2 * BULB - 2
  local function level()
    local f = math.max(0, math.min(1, common.read(values.fraction) or 0))
    return low - (low - high) * f
  end
  return ui.Item {
    x = values.x, y = values.y, width = WIDTH, height = size,
    ui.Rect { x = WIDTH / 2 - 3, width = 6, color = ink.accent,
      y = level, height = function() return size - BULB - level() end,
      behavior = { y = theme.behave("medium"), height = theme.behave("medium") } },
    ui.Image { width = WIDTH, height = size, source = function() return glass_doc(size, ink) end },
  }
end

M.WIDTH = WIDTH

return M
