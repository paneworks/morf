-- A clock face: twelve marks (every third one long), two hands, and an accent
-- seconds hand when seconds are on. The large size adds minute marks, four
-- numerals and a date window at three o'clock.
--
-- Port of Dial.qml. The face is one document; each hand is a document of its
-- own, drawn pointing at twelve on a dial-sized node whose `rotation`
-- follows the clock, so the pivot is always the centre and a minute passing
-- turns a picture rather than drawing one. The seconds hand exists only
-- with `clockShowsSeconds`, as in the original.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local settings = require("services.settings")
local svg = require("desktop.faces.analogue.svg")
local S = require("desktop.sources")

local C = theme.color
local M = {}

local n = svg.n

local function face_doc(size, ink, minute_ticks)
  local r = size / 2
  local dim, text, muted = svg.hex(ink.dim()), svg.hex(ink.text()), svg.hex(ink.muted())
  return svg.cached(table.concat({ "dial", size, dim, text, muted, tostring(minute_ticks) }, ":"), function()
    local parts = {
      string.format('<circle cx="%s" cy="%s" r="%s" fill="none" stroke="%s" stroke-width="1"/>', n(r), n(r), n(r - 0.5), dim),
    }
    local count = minute_ticks and 60 or 12
    for i = 0, count - 1 do
      local hour = not minute_ticks or i % 5 == 0
      local quarter = minute_ticks and i % 15 == 0 or (not minute_ticks and i % 3 == 0)
      local width = quarter and 3 or (hour and 2 or 1)
      local len = quarter and r * 0.16 or (hour and r * 0.09 or r * 0.045)
      parts[#parts + 1] = svg.bar(r, r * 0.08, width, len, i * (360 / count), r, r,
        string.format('fill="%s"', quarter and text or muted))
    end
    return svg.doc(size, size, table.concat(parts))
  end)
end

-- A hand pointing at twelve, from the centre: `len` long, `width` wide, and
-- `tail` past the centre.
local function hand_doc(size, width, len, tail, colour)
  local r = size / 2
  local hex = svg.hex(colour)
  return svg.cached(table.concat({ "hand", size, width, len, tail, hex }, ":"), function()
    return svg.doc(size, size, string.format('<rect x="%s" y="%s" width="%s" height="%s" rx="%s" fill="%s"/>',
      n(r - width / 2), n(r - len), n(width), n(len + tail), n(tail > 0 and 0 or width / 2), hex))
  end)
end

--- `values`: `size`, `ink`, `numerals`, `minute_ticks`, `date_window`.
function M.build(values)
  local size = values.size
  local ink = values.ink
  local r = size / 2
  local function now() return S.clock.now() end
  local function hand(width, len, tail, colour, angle, visible)
    return ui.Image {
      x = 0, y = 0, width = size, height = size,
      visible = visible,
      source = function() return hand_doc(size, width, len, tail, colour()) end,
      rotation = angle,
    }
  end
  local children = {
    ui.Image { x = 0, y = 0, width = size, height = size,
      source = function() return face_doc(size, ink, values.minute_ticks) end },
  }
  if values.numerals then
    for _, pair in ipairs { { 12, 0 }, { 3, 90 }, { 6, 180 }, { 9, 270 } } do
      local a = math.rad(pair[2] - 90)
      local cx, cy = r + r * 0.72 * math.cos(a), r + r * 0.72 * math.sin(a)
      children[#children + 1] = kit.text {
        x = cx - 20, y = cy - 12, width = 40, height = 24,
        horizontal_alignment = "center", vertical_alignment = "center",
        text = tostring(pair[1]), size = theme.size.large, weight = 600, color = ink.text,
      }
    end
  end
  if values.date_window then
    children[#children + 1] = ui.Rect {
      x = r + r * 0.5 - 13, y = r - 11, width = 26, height = 22, radius = 3,
      color = function() return svg.paper(ink) end,
      kit.text { anchors = { fill = true }, horizontal_alignment = "center", vertical_alignment = "center",
        text = function() return tostring(now().day) end,
        size = theme.size.medium, weight = 600, color = C.paperInk },
    }
  end
  local seconds = function() return settings.clockShowsSeconds end
  children[#children + 1] = hand(math.max(4, r * 0.07), r * 0.52, 0, ink.text, function()
    local t = now()
    return (t.hour % 12) * 30 + t.minute * 0.5
  end)
  children[#children + 1] = hand(math.max(3, r * 0.045), r * 0.74, 0, ink.text, function()
    local t = now()
    return t.minute * 6 + (seconds() and t.second * 0.1 or 0)
  end)
  children[#children + 1] = hand(1.5, r * 0.82, r * 0.18, ink.accent, function()
    return now().second * 6
  end, seconds)
  local hub = math.max(6, r * 0.1)
  children[#children + 1] = ui.Rect { x = r - hub / 2, y = r - hub / 2, width = hub, height = hub,
    radius = hub / 2, color = ink.text }
  local pin = math.max(2, r * 0.04)
  children[#children + 1] = ui.Rect { x = r - pin / 2, y = r - pin / 2, width = pin, height = pin,
    radius = pin / 2, color = ink.accent, visible = seconds }
  return ui.Item { x = values.x, y = values.y, width = size, height = size, table.unpack(children) }
end

return M
