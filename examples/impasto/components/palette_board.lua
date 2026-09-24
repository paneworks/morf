-- The logo at any size: a painter's board with five daubs in the palette's
-- colours (PaletteBoard.qml).
--
-- The board is the painted picture impasto installs beside its wallpapers
-- (`art/palette-board.png` here, cut down from 1254 px to 512, which keeps
-- the brush texture at every size the shell draws it), and the daubs are laid
-- over it where its geometry file (Board.qml's `palette-board.json`) puts
-- them, each in a palette token so the logo repaints with the wallpaper.
-- Without the picture it is one daub in the accent, as the original falls
-- back to.

local ui = require("morf.ui")
local theme = require("theme")

local C = theme.color

-- palette-board.json, in board coordinates (a 1254 square).
local CANVAS = 1254
local DAUB = { width = 144, height = 90, radius = 45 }
local PAINT = {
  { key = "accent", x = 280, y = 650, rotation = -72 },
  { key = "green", x = 340, y = 480, rotation = -52 },
  { key = "yellow", x = 450, y = 340, rotation = -33 },
  { key = "red", x = 620, y = 250, rotation = -12 },
  { key = "blue", x = 800, y = 225, rotation = 6 },
}

local PICTURE = morf.fs.join(morf.shell_dir(), "art", "palette-board.png")
local have_picture = morf.fs.is_file(PICTURE)

--- `size` in pixels (64).
return function(values)
  local size = values.size or 64
  local f = size / CANVAS
  local fade = { duration = theme.duration_medium() }
  local children = {
    anchors = values.anchors, x = values.x, y = values.y,
    width = size, height = size,
  }
  if not have_picture then
    children[#children + 1] = ui.Rect {
      anchors = { center_in = true },
      width = size * 0.42, height = size * 0.27, radius = size * 0.135, rotation = -16,
      color = C.accent, behavior = { color = fade },
    }
    return ui.Item(children)
  end
  children[#children + 1] = ui.Image {
    anchors = { fill = true }, source = PICTURE, fill_mode = "preserve_aspect_fit",
    source_width = math.ceil(size * 2), source_height = math.ceil(size * 2),
  }
  for _, daub in ipairs(PAINT) do
    local w, h = DAUB.width * f, DAUB.height * f
    children[#children + 1] = ui.Rect {
      x = daub.x * f - w / 2, y = daub.y * f - h / 2, width = w, height = h,
      radius = DAUB.radius * f, rotation = daub.rotation,
      color = C[daub.key],
      behavior = { color = fade },
    }
  end
  return ui.Item(children)
end
