-- A rotary knob: 21 marks over three quarters of a turn, lit up to the
-- level, with an accent pointer. Muted, a bar crosses it in the muted colour
-- rather than red: silence is not a warning.
--
-- Port of Knob.qml. The marks are one document (it changes only when a mark
-- lights or goes out); the pointer turns on its own node. Unlike the
-- original, which only showed the level, `set` makes it a control: the
-- wheel turns it by 5%, and a drag up or down by a percent per two pixels.

local ui = require("morf.ui")
local theme = require("theme")
local common = require("desktop.faces.common")
local svg = require("desktop.faces.analogue.svg")

local M = {}
local n = svg.n

local function marks_doc(size, ink, lit, muted)
  local c = size / 2
  local r = size * 0.34
  local text, dim, raised, border = svg.hex(ink.text()), svg.hex(ink.dim()), ink.raised(), ink.border()
  return svg.cached(table.concat({ "knob", size, text, dim, svg.hex(raised), svg.hex(border), lit, tostring(muted) }, ":"), function()
    local parts = {}
    for i = 0, 20 do
      local major = i % 5 == 0
      local on = i <= lit and not muted
      parts[#parts + 1] = svg.bar(c, c - size * 0.48, major and 2.5 or 1.5, major and 8 or 5, 135 + 270 * i / 20 + 90, c, c,
        string.format('fill="%s"', on and text or dim))
    end
    parts[#parts + 1] = string.format('<circle cx="%s" cy="%s" r="%s" %s %s stroke-width="2"/>',
      n(c), n(c), n(r - 1), svg.paint("fill", raised), svg.paint("stroke", border))
    parts[#parts + 1] = string.format('<circle cx="%s" cy="%s" r="%s" fill="none" stroke="%s" stroke-width="1"/>',
      n(c), n(c), n(r - 8), dim)
    if muted then
      parts[#parts + 1] = string.format('<rect x="%s" y="%s" width="%s" height="4" rx="2" fill="%s" transform="rotate(-45 %s %s)"/>',
        n(c - r * 1.15), n(c - 2), n(r * 2.3), svg.hex(ink.muted()), n(c), n(c))
    end
    return svg.doc(size, size, table.concat(parts))
  end)
end

local function pointer_doc(size, colour)
  local c, r = size / 2, size * 0.34
  local hex = svg.hex(colour)
  return svg.cached("pointer:" .. size .. ":" .. hex, function()
    return svg.doc(size, size, string.format('<rect x="%s" y="%s" width="4" height="%s" rx="2" fill="%s"/>',
      n(c - 2), n(c - (r - 10)), n(r * 0.55), hex))
  end)
end

--- `values`: `size`, `ink`, `fraction` and `muted` (functions), `set`
--- (called with a fraction), `toggle` (a click).
function M.build(values)
  local size, ink = values.size, values.ink
  local function fraction() return math.max(0, math.min(1, common.read(values.fraction) or 0)) end
  local function muted() return common.read(values.muted) and true or false end
  local start_fraction, moved = 0, false
  local children = {
    ui.Image { width = size, height = size, source = function()
      return marks_doc(size, ink, math.floor(fraction() * 20 + 0.001), muted())
    end },
    ui.Image { width = size, height = size,
      source = function() return pointer_doc(size, muted() and ink.muted() or ink.accent()) end,
      rotation = function() return -135 + 270 * fraction() end,
      behavior = { rotation = theme.behave("medium") } },
  }
  if values.set then
    children[#children + 1] = ui.MouseArea {
      width = size, height = size, cursor = "pointer",
      on_pressed = function() start_fraction, moved = fraction(), false end,
      on_dragged = function(_, _, _, dy)
        if math.abs(dy) < 3 and not moved then return end
        moved = true
        values.set(math.max(0, math.min(1, start_fraction - dy / 200)))
      end,
      on_clicked = function() if not moved and values.toggle then values.toggle() end end,
      on_wheel = function(_, _, _, py, _, steps)
        local step = steps ~= 0 and -steps or (py > 0 and -1 or 1)
        values.set(math.max(0, math.min(1, fraction() + step * 0.05)))
      end,
    }
  end
  return ui.Item { x = values.x, y = values.y, width = size, height = size, table.unpack(children) }
end

return M
