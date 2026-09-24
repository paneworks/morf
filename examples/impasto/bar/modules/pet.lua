-- The pet module: the chip on the bar, and the detail it opens.
--
-- Port of bar/modules/PetModule.qml and the pet's part of ChipFace.qml. The
-- chip is the pet itself at glyph size with its level beside it, or, in the
-- ring shape, the pet inside a ring that is its progress to the next level
-- (white, because it warns about nothing). A click opens the detail in the
-- island, 380 x 172 as ModuleService's catalogue has it: the pet with its
-- bob and hop, feed and play, and the shelf of the family below.
--
-- It is a module like any other (`modules.define`), so a piece's own
-- shape, figure and when apply to it, and the chip is `bar.modules.chip`'s.

local ui = require("morf.ui")
local theme = require("theme")
local modules = require("services.modules")
local pets = require("services.pets")
local kit = require("components.kit")
local pill = require("components.pill_button")
local usage_bar = require("components.usage_bar")
local ring = require("components.ring_indicator")
local draw = require("pets.draw")
local face = require("pets.face")
local shelf = require("pets.shelf")

local C = theme.color

local module = {}

-- The detail used to be a panel of its own; it is the module panel now.
module.DETAIL = "module"

--- What the chip says beside the pet.
function module.figure()
  return pets.hatched() and ("Lv " .. pets.level()) or "Egg"
end

-- --------------------------------------------------------------- detail --

local function detail()
  local big = face.new { size = 60, lively = true }
  local inner = 380 - 8 - 28
  return ui.Column {
    width = inner,
    gap = 10,
    ui.Flex {
      width = inner,
      direction = "row", gap = 14, align = "center",
      ui.Item {
        width = 68, height = 68,
        -- Idle bob; a hop pauses it for its own length.
        ui.Item { x = 4, y = 4, face.lively(big, 2, 10) },
      },
      ui.Column {
        layout = { grow = 1, minimum_width = 0 },
        gap = 6, align = "stretch",
        ui.Flex {
          direction = "row", gap = 6, align = "center",
          -- Hatched pets show their name, then their species.
          kit.text {
            layout = { grow = 1, minimum_width = 0 },
            text = function() return pets.title_of(pets.pet()) end,
            elide = "right",
            size = theme.size.medium, weight = 600,
          },
          kit.text {
            visible = pets.hatched,
            text = function() return "Lv " .. pets.level() end,
            mono = true, size = theme.size.small, color = C.textMuted,
          },
        },
        kit.text {
          text = pets.mood_line,
          elide = "right",
          size = theme.size.small, color = C.textMuted,
        },
        ui.Flex {
          height = 14,
          direction = "row", gap = 8, align = "center",
          usage_bar {
            layout = { grow = 1, minimum_width = 0 },
            progress = pets.progress,
            fill_color = function() return draw.coat(pets.species_info()) end,
          },
          kit.text {
            text = function() return pets.xp() .. "/" .. pets.threshold() end,
            mono = true, size = theme.size.label, color = C.textMuted,
          },
        },
        ui.Flex {
          height = 28,
          direction = "row", gap = 9, align = "center",
          pill {
            layout = { grow = 1, basis = 0 },
            text = function() return pets.can_feed() and "Feed" or "Fed" end,
            enabled = pets.can_feed, dim = 0.45,
            on_click = pets.feed,
          },
          pill {
            layout = { grow = 1, basis = 0 },
            text = function() return pets.can_play() and "Play" or "Played" end,
            enabled = pets.can_play, dim = 0.45,
            on_click = pets.play,
          },
        },
      },
    },
    shelf.new { width = inner },
  }
end

-- ----------------------------------------------------------------- chip --

--- The ring face: the pet inside its progress to the next level, white
--- because it warns about nothing.
local function ring_face()
  local capsule = theme.capsule_height
  return ring {
    size = capsule,
    thickness = 2.5,
    progress = pets.progress,
    track_color = C.indicatorDim,
    fill_color = C.indicator,
    ui.Item {
      anchors = { center_in = true },
      width = function() return math.floor(capsule() * 0.56 + 0.5) end,
      height = function() return math.floor(capsule() * 0.56 + 0.5) end,
      face.new { size = function() return math.floor(capsule() * 0.56 + 0.5) end, lively = true },
    },
  }
end

modules.define("pet", {
  value = module.figure,
  -- The service is always there; being on the bar is what keeps it company.
  has = function() return true end,
  -- The pet at glyph size, in place of a font glyph (ChipFace.qml:86-95).
  chip_mark = function()
    local size = function() return math.floor(theme.capsule_height() * 0.44 + 0.5) + 2 end
    return face.new { size = size, lively = true }
  end,
  chip = ring_face,
  detail = function()
    pets.subscribe()
    return ui.Item {
      anchors = { fill = true },
      on_destroyed = function() pets.release() end,
      ui.Item { x = 14, y = 14, width = 380 - 8 - 28, height = 172 - 8 - 28, detail() },
    }
  end,
})

--- The piece bar/pieces/pet.lua registers: the module's chip, with the
--- piece's own shape, figure and when (ModuleService.qml:229).
function module.chip(item)
  item = item or {}
  return require("bar.modules.chip").piece("pet",
    { shape = item.shape, figure = item.figure, when = item.when })
end

-- `morf ipc call pet.detail` opens the detail, as a click on the chip does.
morf.ipc["pet.detail"] = function()
  modules.activate("pet")
  return modules.open_id:get()
end

return module
