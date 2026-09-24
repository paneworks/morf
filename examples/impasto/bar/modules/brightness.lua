-- Brightness: hidden when no screen can be dimmed. The ring is white:
-- brightness is a choice, not a warning. The detail has one row per screen
-- that can be dimmed; with only one, the row is "Brightness".
--
-- Port of BrightnessModule.qml.

local ui = require("morf.ui")
local theme = require("theme")
local modules = require("services.modules")
local brightness = require("services.brightness")
local kit = require("components.kit")
local controls = require("components.controls")

local C = theme.color
local ROW = 48

-- One row per display slot: the backlight, then the DDC monitors. A slot
-- with nothing behind it takes no room.
local function row(index, column_w)
  local display = function() return brightness.dimmable()[index] end
  local percent = function() local d = display() return d and d.percent() or 0 end
  local hovered = controls.signal("brightness.slider", false)
  return ui.Row {
    gap = 14, align = "center",
    visible = function() return display() ~= nil end,
    ui.Item {
      width = 48, height = function() return display() and ROW or 0 end,
      controls.ring_glyph {
        size = 48, thickness = 3,
        progress = function() return percent() / 100 end,
        track_color = C.indicatorDim, fill_color = C.indicator,
        glyph = function() return brightness.icon_for(percent()) end, glyph_size = 17,
      },
    },
    ui.Column {
      gap = 8, width = column_w,
      ui.Item {
        width = column_w, height = 16,
        kit.text { anchors = { left = true, vertical_center = true },
          width = column_w - 50, elide = "right",
          text = function()
            local d = display()
            if not d then return "" end
            if #brightness.dimmable() > 1 then return type(d.title) == "function" and d.title() or d.title end
            return "Brightness"
          end,
          size = theme.size.small, weight = 600 },
        kit.text { anchors = { right = true, vertical_center = true },
          text = function() return percent() .. "%" end, mono = true, size = theme.size.small },
      },
      ui.Item {
        width = column_w, height = 16,
        controls.usage_bar {
          anchors = { vertical_center = true }, width = column_w,
          height = function() return hovered:get() and 6 or 4 end,
          progress = function() return percent() / 100 end,
        },
        controls.drag_area {
          width = column_w, from = 0, to = 100, value = percent, hovered = hovered,
          on_moved = function(value) local d = display() if d then d.set_percent(value) end end,
        },
      },
    },
  }
end

modules.define("brightness", {
  glyph = brightness.icon,
  value = function() return brightness.percent() .. "%" end,
  has = brightness.available,
  size = function()
    local item = modules.entry("brightness")
    return { item.width, item.height + math.max(0, #brightness.dimmable() - 1) * 62 }
  end,
  chip = function()
    return controls.ring_glyph {
      size = theme.capsule_height(), thickness = 2.5,
      progress = function() return brightness.percent() / 100 end,
      track_color = C.indicatorDim, fill_color = C.indicator,
      glyph = brightness.icon, glyph_size = math.floor(theme.capsule_height() * 0.38 + 0.5),
    }
  end,
  detail = function()
    brightness.refresh()
    local column_w = modules.entry("brightness").width - 8 - 28 - 48 - 14
    local rows = { anchors = { left = true, top = true, left_margin = 14, top_margin = 26 }, gap = 14 }
    -- The detail is built as it opens, so it has the screens of that moment.
    for index = 1, math.max(1, #brightness.dimmable()) do rows[#rows + 1] = row(index, column_w) end
    return ui.Item { anchors = { fill = true }, ui.Column(rows) }
  end,
})
