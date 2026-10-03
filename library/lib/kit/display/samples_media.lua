-- Gallery samples for the media display widgets: name -> function(kit) returning a
-- node that fits a 260x190 cell (examples/shells/caelestia/tests/kit_gallery_spec.lua).
local morf = require("morf")
local ui = require("morf.ui")

-- A picture in the repository (impasto's palette board), and none at all.
local function picture() return morf.config_path("../../impasto/shell/art/palette-board.png") end
local MISSING = nil -- no picture: the placeholder

return {
  image = function(kit)
    return ui.Column { gap = 10,
      kit.image { source = picture(), width = 260, height = 110, fit = "cover", alt = "Palette board" },
      ui.Row { gap = 10,
        kit.image { source = picture(), width = 125, height = 70, fit = "contain", alt = "Palette board, whole" },
        kit.image { source = MISSING, width = 125, height = 70, alt = "No picture" } } }
  end,
  avatar = function(kit)
    return ui.Column { gap = 18,
      ui.Row { gap = 12, align = "center",
        kit.avatar { name = "Ada Lovelace", size = 56 }, kit.avatar { name = "Linus", size = 44 },
        kit.avatar { name = "Grace Hopper", source = picture(), size = 44 }, kit.avatar { name = "Ken Thompson", size = 32 } },
      kit.avatar { group = { "Ada Lovelace", "Alan Turing", "Grace Hopper", "Dennis Ritchie", "Barbara Liskov", "Ken" },
        size = 44 } }
  end,
  thumbnail = function(kit)
    return ui.Row { gap = 12,
      kit.thumbnail { source = picture(), width = 124, height = 150, caption = "Palette", index = 1 },
      kit.thumbnail { source = MISSING, width = 124, height = 150, caption = "No picture yet", index = 2 } }
  end,
  video = function(kit)
    return kit.video { source = picture(), width = 260, height = 170, title = "Palette timelapse",
      duration = "3:42", played = function() return 0.35 end }
  end,
}
