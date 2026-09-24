-- A needle gauge: a 240 degree arc with eleven marks, the top fifth in
-- warning red, and an accent needle. A label under the hub and a small
-- value under that.
--
-- Port of Gauge.qml. `warns = false` removes the red, `low_is_bad` moves it
-- to the empty end, and `ends` labels the two ends (E and F on a fuel gauge).
-- The needle is its own document on a node whose rotation follows the
-- fraction, animated as the original's Behavior did.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local common = require("desktop.faces.common")
local svg = require("desktop.faces.analogue.svg")

local C = theme.color
local M = {}
local n = svg.n
local START, SWEEP = 150, 240

local function arc_doc(size, ink, warns, low_is_bad)
  local cx, r = size / 2, size / 2 - 6
  local dim, text, muted = svg.hex(ink.dim()), svg.hex(ink.text()), svg.hex(ink.muted())
  return svg.cached(table.concat({ "gauge", size, dim, text, muted, tostring(warns), tostring(low_is_bad) }, ":"), function()
    local parts = {
      string.format('<path d="%s" fill="none" stroke="%s" stroke-width="2"/>', svg.arc(cx, cx, r, START, SWEEP), dim),
    }
    if warns then
      local from = low_is_bad and START or START + SWEEP * 0.82
      parts[#parts + 1] = string.format('<path d="%s" fill="none" stroke="%s" stroke-width="3"/>',
        svg.arc(cx, cx, r, from, SWEEP * 0.18), svg.hex(C.indicatorBad))
    end
    for i = 0, 10 do
      local major = i % 5 == 0
      local width, len = major and 2.5 or 1.5, major and 11 or 6
      parts[#parts + 1] = svg.bar(cx, cx - r + 5, width, len, START + SWEEP * i / 10 + 90, cx, cx,
        string.format('fill="%s"', major and text or muted))
    end
    return svg.doc(size, size, table.concat(parts))
  end)
end

local function needle_doc(size, colour)
  local cx, r = size / 2, size / 2 - 6
  local hex = svg.hex(colour)
  return svg.cached(table.concat({ "needle", size, hex }, ":"), function()
    local len = r - 14 + r * 0.2
    return svg.doc(size, size, string.format('<rect x="%s" y="%s" width="3" height="%s" rx="1.5" fill="%s"/>',
      n(cx - 1.5), n(cx - (r - 14)), n(len), hex))
  end)
end

--- `values`: `size`, `ink`, `fraction` (function), `label`, `value`
--- (strings or functions), `warns` (default true), `low_is_bad`, `ends`,
--- and `hub`, a node drawn over the gauge (a mark above the hub).
function M.build(values)
  local size, ink = values.size, values.ink
  local cx, r = size / 2, size / 2 - 6
  local warns = values.warns ~= false
  local children = {
    ui.Image { width = size, height = size,
      source = function() return arc_doc(size, ink, warns, values.low_is_bad) end },
  }
  for i, label in ipairs(values.ends or {}) do
    local a = math.rad(START + (i == 1 and 0 or SWEEP))
    children[#children + 1] = kit.text {
      x = cx + (r + 12) * math.cos(a) - 10, y = cx + (r + 12) * math.sin(a) - 9, width = 20, height = 18,
      horizontal_alignment = "center", vertical_alignment = "center",
      text = label, size = theme.size.small, weight = 600, color = ink.muted,
    }
  end
  children[#children + 1] = ui.Image {
    width = size, height = size,
    source = function() return needle_doc(size, ink.accent()) end,
    rotation = function()
      local f = math.max(0, math.min(1, common.read(values.fraction) or 0))
      return SWEEP * f - 120
    end,
    behavior = { rotation = theme.behave("medium") },
  }
  children[#children + 1] = ui.Rect { x = cx - 4.5, y = cx - 4.5, width = 9, height = 9, radius = 4.5, color = ink.text }
  if values.hub then children[#children + 1] = values.hub end
  children[#children + 1] = kit.text {
    x = 0, y = cx + r * 0.5, width = size, horizontal_alignment = "center",
    text = values.label or "", size = theme.size.label, letter_spacing = 1, color = ink.muted,
  }
  children[#children + 1] = kit.text {
    x = 0, y = cx + r * 0.5 + 15, width = size, horizontal_alignment = "center", elide = "right",
    text = function() return tostring(common.read(values.value) or "") end,
    size = theme.size.regular, weight = 600, color = ink.text,
  }
  return ui.Item { x = values.x, y = values.y, width = size, height = size, table.unpack(children) }
end

return M
