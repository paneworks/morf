-- A value nudged with arrows, wrapping at both ends (Stepper). Compact
-- enough to fit two across in the island.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local controls = require("components.controls")

local C = theme.color

--- `value()`, `maximum` (59), `unit`, `on_changed(value)`.
return function(values)
  local value = values.value
  local maximum = values.maximum or 59
  local function step(delta)
    local v = tonumber(controls.val(value)) or 0
    if values.on_changed then values.on_changed((v + delta + maximum + 1) % (maximum + 1)) end
  end
  local function arrow(glyph, delta)
    local hovered = controls.signal("stepper", false)
    return ui.Item {
      width = 14, height = 12,
      kit.glyph {
        anchors = { center_in = true }, glyph = glyph, size = 10,
        color = function() return hovered:get() and C.accent() or C.textMuted() end,
        behavior = { color = theme.behave("fast") },
      },
      controls.hit { hovered = hovered, on_click = function() step(delta) end },
    }
  end
  return ui.Rect {
    width = values.width or 62, height = 26, radius = theme.radius_small,
    color = C.islandSurface, border_width = 1, border_color = C.islandBorder,
    kit.text {
      anchors = { left = true, left_margin = 9, vertical_center = true },
      text = function()
        local v = tonumber(controls.val(value)) or 0
        return string.format("%02d", v) .. ((values.unit or "") ~= "" and (" " .. values.unit) or "")
      end,
      mono = true, size = theme.size.small,
    },
    ui.Column {
      anchors = { right = true, right_margin = 4, vertical_center = true },
      arrow("󰅃", 1), arrow("󰅀", -1),
    },
  }
end
