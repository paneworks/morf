-- The collection in one row: a slot per species and a line saying what
-- earns the next.
--
-- Port of PetShelf.qml. Sleeping pets are shown in their slots and
-- undiscovered ones are empty; the one that is out is ringed in white.
-- Clicking a sleeping pet brings it out. Used in the module's detail.

local ui = require("morf.ui")
local theme = require("theme")
local pets = require("services.pets")
local kit = require("components.kit")
local face = require("pets.face")

local C = theme.color

local shelf = {}

--- `slot` is the slot's size (32); `brief` the short caption, for a
--- narrow place.
function shelf.new(options)
  options = options or {}
  local size = options.slot or 32
  local children = {}
  for index = 1, #pets.species do
    local hovered = kit.hover_signal("pet.slot")
    local filled = function() return index <= pets.count() end
    local out = function() return index == pets.active_index() end
    children[#children + 1] = ui.Rect {
      width = size, height = size, radius = size / 2,
      color = function()
        if out() then return C.islandSurface end
        return hovered:get() and C.islandSurfaceHover or "#00000000"
      end,
      border_width = 1,
      -- The active pet in white; sleepers outlined, empty slots faint.
      border_color = function()
        if out() then return C.indicator end
        return filled() and C.islandBorder or C.hairline
      end,
      behavior = { color = theme.behave("fast") },
      ui.Item {
        anchors = { center_in = true },
        width = math.floor(size * 0.625 + 0.5),
        height = math.floor(size * 0.625 + 0.5) + 2,
        visible = filled,
        face.new {
          size = math.floor(size * 0.625 + 0.5),
          record = function() return pets.record_at(index) end,
          mood = function() return pets.mood_at(index) end,
        },
      },
      ui.MouseArea {
        anchors = { fill = true },
        cursor = function() return (filled() and not out()) and "pointer" or "default" end,
        on_entered = function() if filled() and not out() then hovered:set(true) end end,
        on_exited = function() hovered:set(false) end,
        on_clicked = function() hovered:set(false) pets.bring_out(index) end,
      },
    }
  end
  children[#children + 1] = kit.text {
    layout = { grow = 1, minimum_width = 0 },
    horizontal_alignment = "right",
    elide = "right",
    text = function()
      if pets.complete() then return "All five found" end
      local left = pets.levels_to_next_egg()
      if options.brief then return left .. " to the next egg" end
      return left == 1 and "1 level to the next egg" or (left .. " levels to the next egg")
    end,
    size = theme.size.label,
    color = C.textMuted,
  }
  return ui.Flex {
    width = options.width,
    height = size,
    layout = options.layout,
    direction = "row", gap = 6, align = "center",
    table.unpack(children),
  }
end

return shelf
