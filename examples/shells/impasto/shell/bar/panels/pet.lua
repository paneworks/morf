-- The pet panel: the creature that is out (feed it, play with it, name it)
-- and, below, the family, where the one that is out is swapped.
--
-- Port of bar/island/PetPanel.qml, at the size DynamicIsland.qml gives it:
-- 560 wide, 205 tall plus 62 for every member of the family, so the island
-- grows by one row when an egg is laid. `morf ipc call pet` toggles it, as
-- impasto's "pet" shortcut does.

local ui = require("morf.ui")
local theme = require("theme")
local island = require("bar.island")
local pets = require("services.pets")
local kit = require("components.kit")
local pill = require("components.pill_button")
local usage_bar = require("components.usage_bar")
local draw = require("pets.draw")
local face = require("pets.face")
local family = require("pets.family")
local name_field = require("pets.name_field")

local C = theme.color

local WIDTH = 560
local INNER = WIDTH - 2 * theme.panel_padding

-- The coat of the pet that is out: the level bar is filled with it.
local function coat() return draw.coat(pets.species_info()) end

local function current()
  local big = face.new { size = 76, lively = true }
  return ui.Flex {
    width = INNER, height = 84,
    direction = "row", gap = 18, align = "center",
    ui.Item {
      width = 84, height = 84,
      -- An idle bob, and a hop when played with that pauses the bob for
      -- its duration.
      ui.Item { x = 4, y = 4, face.lively(big, 3, 12) },
    },
    ui.Column {
      layout = { grow = 1, minimum_width = 0 },
      gap = 5, align = "stretch",
      ui.Flex {
        height = 24,
        direction = "row", gap = 10, align = "center",
        ui.Item {
          layout = { grow = 1, minimum_width = 0 },
          height = 24,
          -- The name is edited in place. An egg has none and says so.
          kit.text {
            anchors = { left = true, top = true, top_margin = 2 },
            visible = function() return not pets.hatched() end,
            text = "Egg",
            size = theme.size.large, weight = 600,
          },
          ui.Item {
            anchors = { fill = true },
            visible = pets.hatched,
            name_field.new {
              anchors = { fill = true },
              height = 24,
              index = pets.active_index,
              size = theme.size.large, weight = 600,
            },
          },
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
          fill_color = coat,
        },
        kit.text {
          text = function() return pets.xp() .. "/" .. pets.threshold() end,
          mono = true, size = theme.size.label, color = C.textMuted,
        },
      },
    },
    ui.Column {
      gap = 8,
      pill {
        width = 88,
        text = function() return pets.can_feed() and "Feed" or "Fed" end,
        enabled = pets.can_feed,
        on_click = pets.feed,
      },
      pill {
        width = 88,
        text = function() return pets.can_play() and "Play" or "Played" end,
        enabled = pets.can_play,
        on_click = pets.play,
      },
    },
  }
end

island.register("pet", {
  size = function() return WIDTH, 205 + 62 * pets.count() end,
  build = function()
    return ui.Column {
      gap = 14,
      current(),
      ui.Rect { width = INNER, height = 1, color = C.hairline },
      family.new { width = INNER },
    }
  end,
})

morf.ipc.pet = function()
  island.toggle("pet")
  return island.state.open_panel()
end
