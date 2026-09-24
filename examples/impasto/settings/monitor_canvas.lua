-- The screens as they sit in the compositor's space, scaled to fit
-- (MonitorCanvas). A screen is dragged into place and, on release, snaps to
-- the nearest edge of another within reach, so a free drag never leaves a
-- one-pixel gap or an overlap; snapping only on release keeps the plate from
-- jumping under the pointer. A click picks a screen.
--
-- Sizes are the layout's: a screen's pixels divided by its scale, turned a
-- quarter for a rotation, which is what positions are counted in.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local controls = require("components.controls")
local setting = require("components.setting")

local C = theme.color
local fast = function() return theme.behave("fast") end

local M = {}

--- A screen's size in the layout.
function M.logical(m)
  local scale = (tonumber(m.scale) or 1) > 0 and tonumber(m.scale) or 1
  local w, h = (m.width or 0) / scale, (m.height or 0) / scale
  if (tonumber(m.transform) or 0) % 2 == 1 then w, h = h, w end
  return math.floor(w + 0.5), math.floor(h + 0.5)
end

--- Where a screen dropped at (x, y) snaps to. Each axis on its own,
--- against four candidates per other screen: butted against either side,
--- or aligned with either edge; the nearest within `reach` wins.
function M.settle(lit, key, wanted_x, wanted_y, reach)
  local me
  for _, m in ipairs(lit) do if m.key == key then me = m end end
  if not me then return { x = math.floor(wanted_x + 0.5), y = math.floor(wanted_y + 0.5) } end
  local x, y = math.floor(wanted_x + 0.5), math.floor(wanted_y + 0.5)
  local near_x, near_y = reach, reach
  for _, o in ipairs(lit) do
    if o.key ~= key then
      for _, c in ipairs { o.x - me.w, o.x + o.w, o.x, o.x + o.w - me.w } do
        local gap = math.abs(c - wanted_x)
        if gap < near_x then near_x, x = gap, c end
      end
      for _, c in ipairs { o.y - me.h, o.y + o.h, o.y, o.y + o.h - me.h } do
        local gap = math.abs(c - wanted_y)
        if gap < near_y then near_y, y = gap, c end
      end
    end
  end
  return { x = x, y = y }
end

--- Every screen's place after the drop, shifted so the layout's top-left
--- is (0, 0): the same arrangement keeps the same numbers.
function M.arrangement_with(lit, key, at)
  local places = {}
  local left, top = math.huge, math.huge
  for _, m in ipairs(lit) do
    local p = m.key == key and { x = at.x, y = at.y } or { x = m.x, y = m.y }
    places[m.key] = p
    left, top = math.min(left, p.x), math.min(top, p.y)
  end
  for _, p in pairs(places) do p.x, p.y = p.x - left, p.y - top end
  return places
end

--- `width`, `height`, `monitors` (`{ key, name, x, y, width, height,
--- scale, transform, disabled }`), `selected()`, `primary()` (a name),
--- `editable()`, `on_picked(key)`, `on_arranged(places)`.
function M.new(values)
  local W, H = values.width, values.height or 220
  local PAD = 26
  -- Room around the arrangement, so a screen can be dropped left of or
  -- above all the others.
  local SLACK = 1.6
  local editable = values.editable or function() return false end
  local lit = {}
  for _, m in ipairs(values.monitors) do
    -- A screen mirroring another shows its picture, not a place of its own.
    if not m.disabled and (m.width or 0) > 0 and (m.mirror or "none") == "none" then
      local w, h = M.logical(m)
      lit[#lit + 1] = { key = m.key, name = m.name, x = m.x or 0, y = m.y or 0, w = w, h = h,
        pw = m.width, ph = m.height }
    end
  end
  local min_x, min_y, span_x, span_y = math.huge, math.huge, 1, 1
  for _, m in ipairs(lit) do min_x, min_y = math.min(min_x, m.x), math.min(min_y, m.y) end
  if min_x == math.huge then min_x, min_y = 0, 0 end
  for _, m in ipairs(lit) do
    span_x = math.max(span_x, m.x - min_x + m.w)
    span_y = math.max(span_y, m.y - min_y + m.h)
  end
  local factor = math.min((W - 2 * PAD) / (span_x * SLACK), (H - 2 * PAD) / (span_y * SLACK))
  local origin_x = (W - span_x * factor) / 2
  local origin_y = (H - span_y * factor) / 2
  local reach = math.floor(40 / math.max(factor, 0.0001) + 0.5)

  local children = { width = W, height = H, setting.wheel_area() }
  for i = 1, 7 do
    children[#children + 1] = ui.Rect { x = math.floor(W * i / 8), y = 0, width = 1, height = H,
      color = C.hairline, opacity = 0.4 }
  end
  for _, m in ipairs(lit) do
    local hovered = controls.signal("canvas.screen", false)
    local held = controls.signal("canvas.held", false)
    local offset = controls.signal("canvas.offset", { 0, 0 })
    local chosen = function() return values.selected() == m.key end
    local base_x = origin_x + (m.x - min_x) * factor
    local base_y = origin_y + (m.y - min_y) * factor
    local star = kit.text {
      anchors = { top = true, right = true, top_margin = 4, right_margin = 6 },
      text = "★", size = theme.size.small,
      visible = function() return values.primary and values.primary() == m.name end,
      color = function() return chosen() and C.accentText() or C.accent() end,
    }
    children[#children + 1] = ui.Rect {
      x = function() return base_x + offset:get()[1] end,
      y = function() return base_y + offset:get()[2] end,
      z = function() return held:get() and 2 or (chosen() and 1 or 0) end,
      width = math.max(8, m.w * factor), height = math.max(8, m.h * factor),
      radius = theme.radius_small,
      color = function() return chosen() and C.accent() or C.islandSurfaceHover end,
      border_width = 1,
      border_color = function()
        if chosen() then return C.accent() end
        return hovered:get() and C.textMuted() or C.islandBorder
      end,
      behavior = { color = fast(), border_color = fast() },
      ui.Column {
        anchors = { center_in = true }, gap = 2, align = "center",
        kit.text { text = m.name, size = theme.size.small, weight = 600,
          color = function() return chosen() and C.accentText() or C.text() end },
        kit.text { text = string.format("%d×%d", m.pw, m.ph), size = theme.size.label, mono = true,
          color = function() return chosen() and C.accentText() or C.textMuted() end },
      },
      star,
      ui.MouseArea {
        anchors = { fill = true },
        cursor = function()
          if not editable() then return "pointer" end
          return held:get() and "grabbing" or "grab"
        end,
        on_entered = function() hovered:set(true) end,
        on_exited = function() hovered:set(false) end,
        on_pressed = function() values.on_picked(m.key) end,
        on_dragged = function(_, _, dx, dy)
          if not editable() then return end
          if not held:get() and math.abs(dx) + math.abs(dy) < 3 then return end
          held:set(true)
          offset:set({ dx, dy })
        end,
        on_released = function()
          if not held:get() then return end
          held:set(false)
          local d = offset:get()
          local wanted_x = m.x + d[1] / factor
          local wanted_y = m.y + d[2] / factor
          local at = M.settle(lit, m.key, wanted_x, wanted_y, reach)
          -- Drawn where it snapped until the compositor reports it there.
          offset:set({ (at.x - m.x) * factor, (at.y - m.y) * factor })
          if values.on_arranged then values.on_arranged(M.arrangement_with(lit, m.key, at)) end
        end,
        on_wheel = setting.wheel,
      },
    }
  end
  if #lit == 0 then
    children[#children + 1] = kit.text { anchors = { center_in = true }, text = "No screen to show",
      size = theme.size.small, color = C.textMuted }
  end
  return ui.ClipRect {
    width = W, height = H, radius = theme.radius_small, color = C.island,
    border_width = 1, border_color = C.islandBorder,
    ui.Item(children),
  }
end

return M
