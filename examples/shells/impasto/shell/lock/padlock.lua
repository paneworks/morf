-- The padlock: a shackle that lifts and swings open.
--
-- Port of Padlock.qml. Drawn rather than a glyph so the shackle can open: it
-- lifts and swings out on its left leg, with a little overshoot. The shackle
-- is an outlined rounded box whose lower half runs behind the body, which
-- hides it -- the same arc and two legs the original draws as a path.

local ui = require("morf.ui")
local theme = require("theme")

local C = theme.color

--- `opened()` and `tint()` are functions a binding follows. 20 x 24.
return function(values)
  local opened = values.opened or function() return false end
  local tint = values.tint or C.text
  local stroke = 2.6
  -- The legs are 9.6 apart, centre to centre, and rise from y = 12; the arc
  -- tops out at y = 3.2.
  local shackle = ui.Rect {
    x = 5.2 - stroke / 2, y = 3.2 - stroke / 2 - 1,
    width = 9.6 + stroke, height = 21,
    radius = (9.6 + stroke) / 2,
    color = "#00000000",
    border_width = stroke,
    border_color = tint,
    -- About the left leg's foot.
    transform_origin_x = (stroke / 2) / (9.6 + stroke),
    transform_origin_y = (12 - (3.2 - stroke / 2 - 1)) / 21,
    translate_y = function() return opened() and -4 or 0 end,
    rotation = function() return opened() and -14 or 0 end,
    behavior = {
      translate_y = { duration = math.max(1, theme.duration_morph()), easing = "out_back" },
      rotation = { duration = math.max(1, theme.duration_morph()), easing = "out_back" },
      border_color = theme.behave("fast"),
    },
  }
  local body = ui.Rect {
    x = 1, y = 10, width = 18, height = 13, radius = 4,
    color = tint,
    behavior = { color = theme.behave("fast") },
  }
  local node = {
    width = 20, height = 24,
    shackle, body,
  }
  for key, value in pairs(values) do
    if key ~= "opened" and key ~= "tint" then node[key] = value end
  end
  return ui.Item(node)
end
