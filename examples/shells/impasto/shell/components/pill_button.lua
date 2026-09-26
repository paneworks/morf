-- A capsule action: fills with the accent when active.
--
-- Port of PillButton.qml, which is how the Quick Controls show a radio or a
-- profile that is on. `text`, `icon` (a glyph), `active`, `enabled` and
-- `on_click`; the first four may be functions, so a binding follows them.
-- A disabled pill is dimmed and takes no clicks.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")

local C = theme.color

local function read(v)
  if type(v) == "function" then return v() end
  return v
end

--- `width` fixes the width (else it fits the label, 10 either side);
--- `height` defaults to 28; `dim` is the opacity while disabled.
return function(values)
  local hovered = kit.hover_signal("pill")
  local active = function() return read(values.active) and true or false end
  local enabled = function()
    if values.enabled == nil then return true end
    return read(values.enabled) and true or false
  end
  local children = {}
  if values.icon then
    children[#children + 1] = kit.glyph {
      glyph = values.icon, size = 12,
      color = function() return active() and C.accentText() or C.accent() end,
    }
  end
  children[#children + 1] = kit.text {
    text = function() return read(values.text) or "" end,
    size = theme.size.small,
    font_weight = function() return active() and 600 or 400 end,
    color = function() return active() and C.accentText() or C.text() end,
  }
  local row = ui.Row { gap = 6, align = "center", table.unpack(children) }
  local padding = values.padding or 10
  return ui.Rect {
    width = values.width or function() return (row.layout_width or 0) + padding * 2 end,
    height = values.height or 28,
    layout = values.layout,
    radius = theme.radius_pill,
    opacity = function() return enabled() and 1 or (values.dim or 0.4) end,
    color = function()
      if active() then return hovered:get() and C.accentHover() or C.accent() end
      return hovered:get() and C.islandSurfaceHover or C.islandSurface
    end,
    border_width = 1,
    border_color = function()
      if active() or hovered:get() then return C.accent() end
      return C.islandBorder
    end,
    behavior = { color = theme.behave("fast"), border_color = theme.behave("fast"), opacity = theme.behave("fast") },
    ui.Item {
      anchors = { center_in = true },
      width = function() return row.layout_width or 0 end,
      height = function() return row.layout_height or 0 end,
      row,
    },
    ui.MouseArea {
      anchors = { fill = true },
      enabled = enabled,
      cursor = "pointer",
      on_entered = function() hovered:set(true) end,
      on_exited = function() hovered:set(false) end,
      on_clicked = function() if enabled() and values.on_click then values.on_click() end end,
    },
  }
end
