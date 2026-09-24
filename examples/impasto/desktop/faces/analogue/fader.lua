-- A fader: a track with marks, lit below the cap, and an accent line across
-- the cap. A slider, so it is not the same drawing as the volume knob.
--
-- Port of Fader.qml. The track and marks are one document; the lit part
-- and the cap are nodes whose height and y follow the level, animated as
-- the original's cap was. With `set` it is a control: drag the cap or use
-- the wheel.

local ui = require("morf.ui")
local theme = require("theme")
local common = require("desktop.faces.common")
local svg = require("desktop.faces.analogue.svg")

local M = {}
local n = svg.n
local WIDTH = 48

local function track_doc(size, ink)
  local travel = size - 24
  local c = WIDTH / 2
  local dim, muted = svg.hex(ink.dim()), svg.hex(ink.muted())
  return svg.cached(table.concat({ "fader", size, dim, muted }, ":"), function()
    local parts = { string.format('<rect x="%s" y="12" width="4" height="%s" rx="2" fill="%s"/>', n(c - 2), n(travel), dim) }
    for i = 0, 10 do
      local y = 12 + travel * i / 10
      local len = i % 5 == 0 and 7 or 4
      parts[#parts + 1] = string.format('<rect x="%s" y="%s" width="%s" height="1.5" fill="%s"/><rect x="%s" y="%s" width="%s" height="1.5" fill="%s"/>',
        n(c - 8 - len), n(y - 0.75), n(len), muted, n(c + 8), n(y - 0.75), n(len), muted)
    end
    return svg.doc(WIDTH, size, table.concat(parts))
  end)
end

--- `values`: `size`, `ink`, `fraction` (function), `set`.
function M.build(values)
  local size, ink = values.size, values.ink
  local travel = size - 24
  local c = WIDTH / 2
  local function fraction() return math.max(0, math.min(1, common.read(values.fraction) or 0)) end
  local function cap_y() return 12 + travel * (1 - fraction()) end
  local start = 0
  local children = {
    ui.Image { width = WIDTH, height = size, source = function() return track_doc(size, ink) end },
    ui.Rect { x = c - 2, width = 4, radius = 2, color = ink.text,
      y = cap_y, height = function() return 12 + travel - cap_y() end,
      behavior = { y = theme.behave("medium"), height = theme.behave("medium") } },
    ui.Rect {
      x = c - 17, width = 34, height = 16, radius = 4,
      y = function() return cap_y() - 8 end,
      behavior = { y = theme.behave("medium") },
      color = ink.raised, border_color = ink.border, border_width = 1,
      ui.Rect { x = 5, y = 7, width = 24, height = 2, radius = 1, color = ink.accent },
    },
  }
  if values.set then
    children[#children + 1] = ui.MouseArea {
      width = WIDTH, height = size, cursor = "pointer",
      on_pressed = function() start = fraction() end,
      on_dragged = function(_, _, _, dy) values.set(math.max(0, math.min(1, start - dy / travel))) end,
      on_wheel = function(_, _, _, py, _, steps)
        local step = steps ~= 0 and -steps or (py > 0 and -1 or 1)
        values.set(math.max(0, math.min(1, fraction() + step * 0.05)))
      end,
    }
  end
  return ui.Item { x = values.x, y = values.y, width = WIDTH, height = size, table.unpack(children) }
end

M.WIDTH = WIDTH

return M
