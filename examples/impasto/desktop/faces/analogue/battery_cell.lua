-- A battery on its side: the case, the terminal and a fill from the left,
-- with a bolt in the accent while charging. The fill colour is passed in,
-- since a low charge is red in every palette.
--
-- Port of BatteryCell.qml: case, terminal and bolt as one document, the
-- fill a node whose width follows the charge.

local ui = require("morf.ui")
local theme = require("theme")
local common = require("desktop.faces.common")
local svg = require("desktop.faces.analogue.svg")

local M = {}
local n = svg.n

local function case_doc(size, ink, charging)
  local text, accent = svg.hex(ink.text()), svg.hex(ink.accent())
  return svg.cached(table.concat({ "cell", size, text, accent, tostring(charging) }, ":"), function()
    local h = size * 0.48
    local body = size - 8
    local parts = {
      string.format('<rect x="1.25" y="1.25" width="%s" height="%s" rx="10" fill="none" stroke="%s" stroke-width="2.5"/>',
        n(body - 2.5), n(h - 2.5), text),
      string.format('<rect x="%s" y="%s" width="6" height="20" rx="2" fill="%s"/>', n(body + 2), n(h / 2 - 10), text),
    }
    if charging then
      local cx, cy, u = body / 2, h / 2, h * 0.06
      local pts = {
        { 1.2, -5 }, { -2.6, 0.6 }, { 0.4, 0.6 }, { -1.2, 5 }, { 2.6, -0.6 }, { -0.4, -0.6 },
      }
      local out = {}
      for _, p in ipairs(pts) do out[#out + 1] = n(cx + u * p[1]) .. "," .. n(cy + u * p[2]) end
      parts[#parts + 1] = string.format('<polygon points="%s" fill="%s"/>', table.concat(out, " "), accent)
    end
    return svg.doc(size, h, table.concat(parts))
  end)
end

--- `values`: `size`, `ink`, `fraction`, `charging`, `fill` (functions).
function M.build(values)
  local size, ink = values.size, values.ink
  local h = size * 0.48
  local body = size - 8
  return ui.Item {
    x = values.x, y = values.y, width = size, height = h,
    ui.Rect { x = 6, y = 6, height = h - 12, radius = 5,
      width = function()
        local f = math.max(0, math.min(1, common.read(values.fraction) or 0))
        return math.max(0, (body - 12) * f)
      end,
      color = function() return common.read(values.fill) or ink.text() end,
      behavior = { width = theme.behave("medium") } },
    ui.Image { width = size, height = h,
      source = function() return case_doc(size, ink, common.read(values.charging) and true or false) end },
  }
end

return M
