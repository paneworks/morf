-- The logo at any size: a painter's board with five daubs in the palette's
-- colours (PaletteBoard).
--
-- The original laid the daubs over a painted picture of a board that ships
-- with impasto. That picture is not part of this port, so the board is
-- drawn: an oval with a thumb hole, the daubs where the original's
-- geometry puts them, each in a palette token so the logo repaints with the
-- wallpaper.

local ui = require("morf.ui")
local theme = require("theme")

local C = theme.color

-- Daub centres and turns on a 100-unit board.
local DAUBS = {
  { key = "accent", x = 30, y = 30, turn = -18 },
  { key = "green", x = 55, y = 22, turn = 8 },
  { key = "yellow", x = 75, y = 36, turn = 24 },
  { key = "red", x = 74, y = 62, turn = -12 },
  { key = "blue", x = 50, y = 72, turn = 14 },
}

--- `size` in pixels (64).
return function(values)
  local size = values.size or 64
  local f = size / 100
  local children = {
    anchors = values.anchors,
    width = size, height = size,
    ui.Path {
      anchors = { fill = true }, view_box = { 0, 0, 100, 100 },
      d = "M50 6 C80 6 96 26 96 50 C96 76 76 94 50 94 C30 94 20 84 22 74 "
        .. "C24 64 34 66 34 58 C34 50 18 54 10 46 C4 40 6 28 14 20 C22 12 36 6 50 6 Z "
        .. "M24 38 C24 33 28 30 32 32 C36 34 35 40 31 42 C27 44 24 42 24 38 Z",
      fill_rule = "evenodd",
      fill_color = C.islandSurfaceHover,
      stroke_color = C.islandBorder, stroke_width = 1.5,
    },
  }
  for _, daub in ipairs(DAUBS) do
    local w, h = 16 * f, 10 * f
    children[#children + 1] = ui.Rect {
      x = daub.x * f - w / 2, y = daub.y * f - h / 2, width = w, height = h,
      radius = h / 2, rotation = daub.turn,
      color = C[daub.key],
      behavior = { color = { duration = 260 } },
    }
  end
  return ui.Item(children)
end
