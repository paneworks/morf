-- The parts of a settings page as pills, all visible, the current one filled
-- (TabStrip). A page with a single part gets no strip.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local controls = require("components.controls")
local setting = require("components.setting")

local C = theme.color
local fast = function() return theme.behave("fast") end

--- `tabs` a list of `{ id, label }`, `current()` the shown id,
--- `on_picked(id)`.
return function(values)
  local children = { gap = 4, align = "center" }
  for _, tab in ipairs(values.tabs or {}) do
    local hovered = controls.signal("tab", false)
    local active = function() return values.current() == tab.id end
    local caption = kit.text {
      anchors = { center_in = true }, text = tab.label, size = theme.size.small,
      weight = function() return active() and 600 or 400 end,
      color = function() return active() and C.accentText() or C.textMuted() end,
      behavior = { color = fast() },
    }
    children[#children + 1] = ui.Rect {
      width = function() return (caption.layout_width or 0) + 24 end,
      height = 26, radius = 13,
      color = function()
        if active() then return C.accent() end
        return hovered:get() and C.islandSurfaceHover or C.islandSurface
      end,
      border_width = 1,
      border_color = function() return active() and C.accent() or C.islandBorder end,
      behavior = { color = fast(), border_color = fast() },
      caption,
      setting.hit { hovered = hovered, on_click = function() values.on_picked(tab.id) end },
    }
  end
  local row = ui.Row(children)
  return ui.Item {
    width = values.width,
    height = 30,
    visible = #(values.tabs or {}) > 1,
    ui.Item { x = 2, anchors = { vertical_center = true },
      width = function() return row.layout_width or 0 end, height = 26, row },
  }
end
