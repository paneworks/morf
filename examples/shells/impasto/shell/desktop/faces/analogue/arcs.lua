-- Wi-Fi as arcs lit by signal strength, one for weak and three for strong;
-- a plug on a cable when the connection is wired.
--
-- Port of Arcs.qml, as one document keyed on how many arcs are lit.

local ui = require("morf.ui")
local common = require("desktop.faces.common")
local svg = require("desktop.faces.analogue.svg")

local M = {}
local n = svg.n

local function doc(size, ink, lit, connected, wired)
  local text, dim = svg.hex(ink.text()), svg.hex(ink.dim())
  return svg.cached(table.concat({ "arcs", size, text, dim, lit, tostring(connected), tostring(wired) }, ":"), function()
    local cx, dot_y = size / 2, size * 0.72
    local stroke = math.max(4, size * 0.055)
    local parts = {}
    if wired then
      parts[#parts + 1] = string.format('<rect x="%s" y="%s" width="%s" height="%s" rx="6" fill="none" stroke="%s" stroke-width="2.5"/>',
        n(cx - size * 0.16), n(size * 0.34), n(size * 0.32), n(size * 0.28), text)
      for _, off in ipairs { -0.07, 0.07 } do
        parts[#parts + 1] = string.format('<rect x="%s" y="%s" width="4" height="%s" rx="2" fill="%s"/>',
          n(cx + size * off - 2), n(size * 0.2), n(size * 0.15), text)
      end
      parts[#parts + 1] = string.format('<rect x="%s" y="%s" width="4" height="%s" rx="2" fill="%s"/>',
        n(cx - 2), n(size * 0.62), n(size * 0.2), text)
    else
      parts[#parts + 1] = string.format('<circle cx="%s" cy="%s" r="%s" fill="%s"/>',
        n(cx), n(dot_y), n(stroke * 0.6), connected and text or dim)
      for i, radius in ipairs { 0.16, 0.31, 0.46 } do
        local on = connected and i <= lit
        parts[#parts + 1] = string.format('<path d="%s" fill="none" stroke="%s" stroke-width="%s" stroke-linecap="round"/>',
          svg.arc(cx, dot_y, size * radius, 225, 90), on and text or dim, n(stroke))
      end
    end
    return svg.doc(size, size, table.concat(parts))
  end)
end

--- `values`: `size`, `ink`, `strength` (0..1), `connected`, `wired` (functions).
function M.build(values)
  local size = values.size
  return ui.Image {
    x = values.x, y = values.y, width = size, height = size,
    source = function()
      local s = common.read(values.strength) or 0
      local lit = 1 + (s > 1 / 3 and 1 or 0) + (s > 2 / 3 and 1 or 0)
      return doc(size, values.ink, lit, common.read(values.connected) and true or false,
        common.read(values.wired) and true or false)
    end,
  }
end

return M
