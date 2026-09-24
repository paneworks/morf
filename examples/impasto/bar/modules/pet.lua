-- The pet module: the chip on the bar, and the detail it opens.
--
-- Port of bar/modules/PetModule.qml and the pet's part of ChipFace.qml. The
-- chip is the pet itself at glyph size with its level beside it, or, in the
-- ring shape, the pet inside a ring that is its progress to the next level
-- (white, because it warns about nothing). A click opens the detail in the
-- island, 380 x 172 as ModuleService's catalogue has it: the pet with its
-- bob and hop, feed and play, and the shelf of the family below.

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local island = require("bar.island")
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

module.DETAIL = "pet.detail"

--- What the chip says beside the pet.
function module.figure()
  return pets.hatched() and ("Lv " .. pets.level()) or "Egg"
end

-- --------------------------------------------------------------- detail --

local function detail()
  local big = face.new { size = 60, lively = true }
  local inner = 380 - 2 * theme.panel_padding
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

island.register(module.DETAIL, {
  size = function() return 380, 172 end,
  build = detail,
})

morf.ipc["pet.detail"] = function()
  island.toggle(module.DETAIL)
  return island.state.open_panel()
end

-- ----------------------------------------------------------------- chip --

--- The chip, `theme.capsule_height()` tall, in the shape and with the
--- figure the settings ask for (`chipShape`: icon | ring, `chipFigure`:
--- on | off | hover).
function module.chip()
  local hovered = kit.hover_signal("pet.chip")
  local capsule = theme.capsule_height
  local ringed = function() return settings.chipShape == "ring" end
  local open = function() return island.state.open_panel() == module.DETAIL end
  -- How far the figure is out: always, never, or while the pointer is on
  -- the chip.
  local reveal = function()
    local mode = settings.chipFigure
    if mode == "off" then return 0 end
    if mode == "hover" then return (hovered:get() or open()) and 1 or 0 end
    return 1
  end
  local glyph = function() return math.floor(capsule() * 0.44 + 0.5) end
  local PAD, SPACING, GAP = 8, 5, 7
  local inset = function() return capsule() * 0.075 end

  local figure = kit.text {
    text = module.figure,
    -- Centred on the line box, which is 1.2 of the size.
    y = function() return (capsule() - theme.size.small * 1.2) / 2 end,
    size = theme.size.small, weight = 600,
  }
  local figure_w = function() return figure.layout_width or 0 end
  local mark_w = function() return ringed() and capsule() or glyph() + 2 end

  local width = function()
    local shown = figure_w() * reveal()
    if ringed() then
      return capsule() + (figure_w() + 2 * GAP - inset()) * reveal()
    end
    return PAD + mark_w() + (shown > 0 and (SPACING + figure_w()) * reveal() or 0) + PAD
  end

  return ui.Item {
    width = width,
    height = capsule,
    behavior = { width = theme.behave("fast") },
    -- Highlight on hover and while the detail is open.
    ui.Rect {
      anchors = { center_in = true },
      width = function() return width() - 4 end,
      height = function() return capsule() - 8 end,
      radius = function() return (capsule() - 8) / 2 end,
      color = C.islandSurfaceHover,
      opacity = function() return (hovered:get() or open()) and 1 or 0 end,
      behavior = { opacity = theme.behave("fast") },
    },
    -- The icon shape: the pet at glyph size.
    ui.Item {
      x = PAD,
      y = function() return (capsule() - glyph() - 2) / 2 end,
      width = function() return glyph() + 2 end,
      height = function() return glyph() + 2 end,
      visible = function() return not ringed() end,
      face.new { size = function() return glyph() + 2 end, lively = true },
    },
    -- The ring shape: the pet inside its progress, drawn at 0.85 so it
    -- does not touch the capsule's outline.
    ring {
      size = capsule,
      scale = 0.85,
      visible = ringed,
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
    },
    -- The figure, clipped to the width opened so far and faded in past a
    -- fifth of the reveal, so a half-open chip never shows half a word.
    ui.ClipRect {
      color = "#00000000",
      x = function()
        return ringed() and (mark_w() - inset() + GAP) or (PAD + mark_w() + SPACING)
      end,
      width = function() return math.max(1, figure_w() * reveal()) end,
      height = capsule,
      visible = function() return reveal() > 0 end,
      ui.Item {
        width = figure_w, height = capsule,
        opacity = function() return math.max(0, (reveal() - 0.2) / 0.8) end,
        figure,
      },
    },
    ui.MouseArea {
      anchors = { fill = true },
      cursor = "pointer",
      on_entered = function() hovered:set(true) end,
      on_exited = function() hovered:set(false) end,
      on_clicked = function() island.toggle(module.DETAIL) end,
    },
  }
end

return module
