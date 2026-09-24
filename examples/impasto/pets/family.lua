-- The pet family: one row per creature in the order found, and one for the
-- next egg.
--
-- Port of PetFamily.qml. Each row has the face, an editable name (an egg has
-- none yet and says so), the level and a pill that brings it out; the one
-- that is out is outlined in the accent. The last row says how far the next
-- egg is, or that all five are found.

local ui = require("morf.ui")
local theme = require("theme")
local pets = require("services.pets")
local kit = require("components.kit")
local pill = require("components.pill_button")
local usage_bar = require("components.usage_bar")
local face = require("pets.face")
local name_field = require("pets.name_field")

local C = theme.color

local family = {}

-- One row per pet, keyed by its place in the family: a pet never leaves, so
-- a new egg is a row added and nothing else is rebuilt.
local rows = morf.list_model({})
morf.effect("impasto.pets.family.rows", function()
  local out = {}
  for i = 1, pets.count() do out[i] = { id = i } end
  rows:replace(out, "id")
end)

local function member(row)
  local index = row.id
  local function record() return pets.record_at(index) or {} end
  local function out() return index == pets.active_index() end
  local function egg() return (record().hatchedAt or 0) <= 0 end

  return ui.Rect {
    height = 52,
    radius = theme.radius_medium,
    color = C.islandSurface,
    border_width = 1,
    border_color = function() return out() and C.accent() or C.islandBorder end,
    behavior = { border_color = theme.behave("fast") },
    ui.Flex {
      anchors = { fill = true, left_margin = 12, right_margin = 12 },
      direction = "row", gap = 12, align = "center",
      face.new {
        size = 28,
        record = function() return pets.record_at(index) end,
        mood = function() return pets.mood_at(index) end,
      },
      -- An egg has no name field yet.
      kit.text {
        layout = { grow = 1, minimum_width = 0 },
        visible = egg,
        text = "Unhatched — care for it and see",
        elide = "right",
        size = theme.size.small,
        color = C.textMuted,
      },
      ui.Rect {
        layout = { grow = 1, minimum_width = 0 },
        visible = function() return not egg() end,
        height = 28,
        radius = theme.radius_small,
        color = C.island,
        border_width = 1,
        border_color = C.islandBorder,
        name_field.new {
          anchors = { fill = true, left_margin = 10, right_margin = 10 },
          height = 28,
          index = function() return index end,
          size = theme.size.small,
        },
      },
      kit.text {
        text = function() return "Lv " .. (record().level or 1) end,
        mono = true,
        size = theme.size.small,
        color = C.textMuted,
      },
      pill {
        text = function() return out() and "Out" or "Bring out" end,
        active = out,
        enabled = function() return not out() end,
        -- The active pill says where it is; it is not a dimmed control.
        dim = 1,
        on_click = function() pets.bring_out(index) end,
      },
    },
  }
end

-- The next egg's slot: how many remain and what earns the next one.
local function next_egg()
  return ui.Rect {
    height = 52,
    radius = theme.radius_medium,
    color = "#00000000",
    border_width = 1,
    border_color = C.hairline,
    ui.Flex {
      anchors = { fill = true, left_margin = 12, right_margin = 12 },
      direction = "row", gap = 12, align = "center",
      ui.Rect {
        width = 28, height = 28, radius = 14,
        color = "#00000000",
        border_width = 1,
        border_color = C.hairline,
        kit.text {
          anchors = { center_in = true },
          visible = pets.complete,
          text = "★",
          font_size = 13,
          color = C.indicatorWarn,
        },
      },
      ui.Column {
        layout = { grow = 1, minimum_width = 0 },
        gap = 2, align = "stretch",
        kit.text {
          text = function() return pets.complete() and "All five found" or "The next egg" end,
          size = theme.size.small,
          weight = 600,
        },
        kit.text {
          text = function()
            if pets.complete() then
              return "Nothing left to find — there is a star waiting at level fifteen for each of them."
            end
            return string.format("Levels across the family: %d/%d", pets.total_level(), pets.next_egg_at())
          end,
          wrap = true,
          size = theme.size.label,
          color = C.textMuted,
        },
      },
      usage_bar {
        width = 90,
        visible = function() return not pets.complete() end,
        progress = pets.egg_progress,
        fill_color = C.accent,
      },
    },
  }
end

--- The family, `width` wide.
function family.new(options)
  return ui.Column {
    width = options.width,
    gap = 10,
    align = "stretch",
    ui.Repeater {
      as = "column", gap = 10, align = "stretch",
      width = options.width,
      model = rows,
      delegate = member,
    },
    next_egg(),
  }
end

return family
