-- The screens as they sit in the compositor's space, scaled to fit
-- (MonitorCanvas). Read-only here: the original dragged screens into place
-- and wrote the arrangement into Hyprland, and this port never writes the
-- compositor. A click picks a screen.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local controls = require("components.controls")
local setting = require("components.setting")

local C = theme.color
local fast = function() return theme.behave("fast") end

local M = {}

--- `width`, `height`, `monitors` (`{ key, name, x, y, width, height,
--- disabled }`), `selected()` and `on_picked(key)`.
function M.new(values)
  local W, H = values.width, values.height or 220
  local PAD = 26
  local SLACK = 1.4
  local lit = {}
  for _, m in ipairs(values.monitors) do if not m.disabled and m.width > 0 then lit[#lit + 1] = m end end
  local min_x, min_y, span_x, span_y = math.huge, math.huge, 1, 1
  for _, m in ipairs(lit) do
    min_x, min_y = math.min(min_x, m.x), math.min(min_y, m.y)
  end
  if min_x == math.huge then min_x, min_y = 0, 0 end
  for _, m in ipairs(lit) do
    span_x = math.max(span_x, m.x - min_x + m.width)
    span_y = math.max(span_y, m.y - min_y + m.height)
  end
  local factor = math.min((W - 2 * PAD) / (span_x * SLACK), (H - 2 * PAD) / (span_y * SLACK))
  local origin_x = (W - span_x * factor) / 2
  local origin_y = (H - span_y * factor) / 2

  local children = { width = W, height = H, setting.wheel_area() }
  -- A faint grid, so the plate reads as a space rather than a card.
  for i = 1, 7 do
    children[#children + 1] = ui.Rect { x = math.floor(W * i / 8), y = 0, width = 1, height = H,
      color = C.hairline, opacity = 0.4 }
  end
  for _, m in ipairs(lit) do
    local hovered = controls.signal("canvas.screen", false)
    local chosen = function() return values.selected() == m.key end
    children[#children + 1] = ui.Rect {
      x = origin_x + (m.x - min_x) * factor, y = origin_y + (m.y - min_y) * factor,
      width = math.max(8, m.width * factor), height = math.max(8, m.height * factor),
      radius = theme.radius_small,
      color = function() return chosen() and C.islandSurfaceHover or C.island end,
      border_width = function() return chosen() and 2 or 1 end,
      border_color = function()
        if chosen() then return C.accent() end
        return hovered:get() and C.textMuted() or C.islandBorder
      end,
      behavior = { color = fast(), border_color = fast() },
      ui.Column {
        anchors = { center_in = true }, gap = 2, align = "center",
        kit.text { text = m.name, size = theme.size.small, weight = 600,
          color = function() return chosen() and C.accent() or C.text() end },
        kit.text { text = string.format("%d × %d", m.width, m.height), size = theme.size.label,
          color = C.textMuted },
      },
      setting.hit { hovered = hovered, on_click = function() values.on_picked(m.key) end },
    }
  end
  if #lit == 0 then
    children[#children + 1] = kit.text { anchors = { center_in = true }, text = "No screen to show",
      size = theme.size.small, color = C.textMuted }
  end
  return ui.ClipRect {
    width = W, height = H, radius = theme.radius_medium, color = C.island,
    border_width = 1, border_color = C.islandBorder,
    ui.Item(children),
  }
end

return M
