-- A button on the bar that opens an island panel: a glyph, lit while the
-- pointer is on it or its panel is open.
--
-- The button half of impasto's BarChip.qml: as wide as the bar is tall, a
-- pill behind the glyph that fades in on hover and stays while the panel
-- it opens is showing.

local ui = require("morf.ui")
local theme = require("theme")
local island = require("bar.island")
local kit = require("components.kit")

local C = theme.color

local door_button = {}

--- `{ glyph, panel }`, or `on_click` instead of a panel.
function door_button.new(options)
  local hovered = kit.hover_signal("door")
  local open = function() return options.panel ~= nil and island.state.open_panel() == options.panel end
  local size = function() return theme.capsule_height() end
  return ui.Item {
    width = size, height = size,
    ui.Rect {
      anchors = { center_in = true },
      width = function() return size() - 4 end,
      height = function() return size() - 8 end,
      radius = function() return (size() - 8) / 2 end,
      color = C.islandSurfaceHover,
      opacity = function() return (hovered:get() or open()) and 1 or 0 end,
      behavior = { opacity = theme.behave("fast") },
    },
    kit.glyph {
      anchors = { center_in = true },
      glyph = options.glyph,
      size = function() return math.floor(size() * 0.44 + 0.5) end,
      color = C.text,
    },
    ui.MouseArea {
      anchors = { fill = true },
      cursor = "pointer",
      on_entered = function() hovered:set(true) end,
      on_exited = function() hovered:set(false) end,
      on_clicked = function()
        if options.on_click then options.on_click()
        elseif options.panel then island.toggle(options.panel) end
      end,
    },
  }
end

return door_button
