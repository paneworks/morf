-- The control centre's pet block: PetBlock.qml.
--
-- The pet's face, its name and level, its mood, the way to the next level,
-- and Feed and Play; three rows tall adds the shelf of the family. The face
-- opens the pet panel. The pet is watched while the block is on the grid.

local ui = require("morf.ui")
local theme = require("theme")
local pets = require("services.pets")
local kit = require("components.kit")
local controls = require("components.controls")
local pill = require("components.pill_button")
local usage_bar = require("components.usage_bar")
local draw = require("pets.draw")
local face = require("pets.face")
local shelf = require("pets.shelf")

local C = theme.color
local M = {}

function M.build(o)
  pets.subscribe()
  local w, h = o.width - 28, o.height - 28
  local hovered = controls.signal("pet.block.face", false)
  local buttons_w = 64
  local text_w = w - 52 - 12 - buttons_w - 12
  local level = kit.text {
    visible = pets.hatched,
    text = function() return "Lv " .. pets.level() end,
    mono = true, size = theme.size.label, color = C.textMuted,
  }
  local xp = kit.text {
    text = function() return pets.xp() .. "/" .. pets.threshold() end,
    mono = true, size = theme.size.label, color = C.textMuted,
  }
  local top = ui.Row {
    gap = 12, align = "center",
    ui.Item {
      width = 52, height = 52,
      ui.Item {
        x = 3, y = 3, width = 46, height = 46,
        scale = function() return hovered:get() and 1.08 or 1 end,
        behavior = { scale = theme.behave("fast") },
        face.new { size = 46, lively = true },
      },
      controls.hit { hovered = hovered, on_click = function() o.on_panel("pet") end },
    },
    ui.Column {
      gap = 3, width = text_w,
      ui.Item {
        width = text_w, height = 14,
        kit.text {
          anchors = { left = true, vertical_center = true },
          width = function() return text_w - (level.layout_width or 0) - 8 end, elide = "right",
          text = function() return pets.title_of(pets.pet()) end,
          size = theme.size.small, weight = 600,
        },
        ui.Item { anchors = { right = true, vertical_center = true }, height = 12,
          visible = pets.hatched, width = function() return level.layout_width or 0 end, level },
      },
      kit.text { width = text_w, elide = "right", text = pets.mood_line,
        size = theme.size.label, color = C.textMuted },
      ui.Item {
        width = text_w, height = 12,
        ui.Item {
          anchors = { left = true, vertical_center = true }, height = 6,
          width = function() return text_w - (xp.layout_width or 0) - 8 end,
          usage_bar {
            width = function() return text_w - (xp.layout_width or 0) - 8 end,
            progress = pets.progress,
            fill_color = function() return draw.coat(pets.species_info()) end,
          },
        },
        ui.Item { anchors = { right = true, vertical_center = true }, height = 12,
          width = function() return xp.layout_width or 0 end, xp },
      },
    },
    ui.Column {
      gap = 6,
      pill {
        width = buttons_w, height = 24,
        text = function() return pets.can_feed() and "Feed" or "Fed" end,
        enabled = pets.can_feed, dim = 0.45, on_click = pets.feed,
      },
      pill {
        width = buttons_w, height = 24,
        text = function() return pets.can_play() and "Play" or "Played" end,
        enabled = pets.can_play, dim = 0.45, on_click = pets.play,
      },
    },
  }
  local children = { width = w, height = h, top }
  if o.rows >= 3 then
    children[#children + 1] = ui.Item {
      x = 0, y = h - 40, width = w, height = 40,
      shelf.new { width = w, slot = 28, brief = true },
    }
  end
  return controls.card {
    width = o.width, height = o.height,
    on_destroyed = function() pets.release() end,
    ui.Item(children),
  }
end

return M
