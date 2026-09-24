-- The quick settings' pieces: a tile, a switch, a slider, a pill, a
-- segmented control, a ring, a square icon button, and the card they sit on.
--
-- Ports of QuickTile, ToggleSwitch, SliderRow, SettingSlider, PillButton,
-- SegmentedControl, RingIndicator, IconButton, Figure, UsageBar and Card.
-- Every value a piece shows may be a plain value or a function, which is
-- then a binding: `active = function() return network.radio_on() end`.
-- Sizes are given by the caller (a number or a binding), because a piece
-- is laid out before it can measure itself.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")

local C = theme.color
local M = {}

local function val(v, ...)
  if type(v) == "function" then return v(...) end
  return v
end
M.val = val

--- A binding for `v`, whatever it is: a function stays one.
local function bind(v)
  if type(v) == "function" then return v end
  return function() return v end
end

local count = 0
local function signal(name, value)
  count = count + 1
  return morf.signal("impasto.controls.piece." .. name .. "." .. count, value)
end
M.signal = signal

local fast = function() return theme.behave("fast") end

-- ------------------------------------------------------------------ card --

--- The rounded surface every control centre section uses. `bare` drops the
--- surface and keeps the contents, for a detail drawn straight on the
--- island. `padding` insets the one child.
function M.card(values)
  local bare = values.bare
  local padding = values.padding or 14
  local out = {
    radius = theme.radius_medium,
    color = bare and "#00000000" or C.islandSurface,
    border_width = bare and 0 or 1,
    border_color = C.islandBorder,
  }
  for k, v in pairs(values) do
    if type(k) ~= "number" and k ~= "bare" and k ~= "padding" then out[k] = v end
  end
  local children = {}
  for index, child in ipairs(values) do children[index] = child end
  -- One Item inside the padding holds them all; a child that anchors fills
  -- the inside, not the card.
  local inner = { x = padding, y = padding, table.unpack(children) }
  if values.width then
    inner.width = function() return val(values.width) - 2 * padding end
  end
  if values.height then
    inner.height = function() return val(values.height) - 2 * padding end
  end
  out[1] = ui.Item(inner)
  return ui.Rect(out)
end

-- ------------------------------------------------------------------ ring --

--- A circular gauge that fills clockwise from the top, with children drawn
--- inside it: RingIndicator. `size`, `thickness`, `progress` (0..1),
--- `track_color`, `fill_color`, `sweep_ms`.
---
--- The arc is a ring intersected with a pie. The pie is centred on straight
--- up, so the field is turned by half its sweep to start at twelve o'clock.
function M.ring(values)
  local size = values.size or theme.capsule_height()
  local thickness = values.thickness or 3
  local progress = bind(values.progress or 0)
  local sweep = function()
    local p = math.max(0, math.min(1, tonumber(progress()) or 0))
    if p <= 0 then return 0 end
    return math.max(2, 360 * p)
  end
  local motion = function()
    return { duration = values.sweep_ms or theme.duration_medium(), easing = theme.easing() }
  end
  local children = {
    width = size, height = size,
    ui.Sdf {
      x = 0, y = 0, width = size, height = size,
      fill_color = values.track_color or C.islandBorder,
      ui.SdfShape { x = 0, y = 0, width = size, height = size, shape = "ring", thickness = thickness },
    },
    ui.Sdf {
      x = 0, y = 0, width = size, height = size,
      fill_color = values.fill_color or C.accent,
      opacity = function() return sweep() > 0 and 1 or 0 end,
      rotation = function() return sweep() / 2 end,
      behavior = { rotation = motion(), opacity = fast() },
      ui.SdfShape { x = 0, y = 0, width = size, height = size, shape = "ring", thickness = thickness },
      ui.SdfShape {
        x = -2, y = -2, width = size + 4, height = size + 4,
        shape = "pie", operation = "intersect",
        angle = sweep,
        behavior = { angle = motion() },
      },
    },
  }
  for _, child in ipairs(values) do children[#children + 1] = child end
  return ui.Item(children)
end

--- A glyph centred in a ring: the common case.
function M.ring_glyph(values)
  local glyph = kit.glyph {
    anchors = { center_in = true },
    glyph = values.glyph, size = values.glyph_size or 16,
    color = values.glyph_color or C.indicator,
  }
  local ring = {}
  for k, v in pairs(values) do ring[k] = v end
  ring.glyph, ring.glyph_size, ring.glyph_color = nil, nil, nil
  ring[#ring + 1] = glyph
  return M.ring(ring)
end

-- --------------------------------------------------------------- figures --

--- One reading in three lines: a small-caps label, the number, a note.
function M.figure(values)
  local width = values.width
  return ui.Column {
    gap = 1, width = width,
    kit.text { text = values.label, size = theme.size.label, weight = 600, color = C.textMuted,
      letter_spacing = 0.6 },
    kit.text { text = values.value, mono = true, size = theme.size.large, weight = 600,
      color = values.value_color or C.text, width = width, elide = "right" },
    kit.text { text = values.note or "", size = theme.size.label, color = C.textMuted,
      width = width, elide = "right",
      visible = function() return (val(values.note) or "") ~= "" end },
  }
end

--- A thin horizontal gauge: UsageBar. `width` is required.
function M.usage_bar(values)
  local width = bind(values.width)
  local height = bind(values.height or 6)
  local progress = bind(values.progress or 0)
  local out = {
    width = width, height = height,
    radius = function() return height() / 2 end,
    color = values.track_color or C.islandSurfaceHover,
    behavior = { height = fast() },
    ui.Rect {
      height = height,
      radius = function() return height() / 2 end,
      width = function()
        local p = math.max(0, math.min(1, tonumber(progress()) or 0))
        if p <= 0 then return 0 end
        return math.max(height(), width() * p)
      end,
      color = values.fill_color or C.accent,
      behavior = { width = values.fill_behavior or theme.behave("medium"), height = fast() },
    },
  }
  for k, v in pairs(values) do
    if out[k] == nil and k ~= "progress" and k ~= "fill_color" and k ~= "track_color"
      and k ~= "fill_behavior" then out[k] = v end
  end
  return ui.Rect(out)
end

-- ---------------------------------------------------------------- buttons --

--- A hover-tracking MouseArea over its parent. `on_click(button)`.
function M.hit(values)
  local hovered = values.hovered
  local enabled = values.enabled
  return ui.MouseArea {
    anchors = values.anchors or { fill = true },
    cursor = function()
      if enabled ~= nil and not val(enabled) then return "default" end
      return values.cursor or "pointer"
    end,
    accepted_buttons = values.right and { "left", "right" } or { "left" },
    on_entered = function() if hovered then hovered:set(true) end end,
    on_exited = function() if hovered then hovered:set(false) end end,
    on_clicked = function(_, _, _, _, button)
      if enabled ~= nil and not val(enabled) then return end
      if values.on_click then values.on_click(button) end
    end,
  }
end

--- A capsule action button: PillButton. `active` fills it with the accent.
--- `text`, `icon`, `height` (28), `width` (else measured), `enabled`.
function M.pill(values)
  local hovered = signal("pill", false)
  local active = bind(values.active or false)
  local enabled = values.enabled == nil and true or values.enabled
  local height = values.height or 28
  local padding = values.padding or 10
  local label = kit.text {
    text = values.text or "", size = theme.size.small,
    weight = function() return active() and 600 or 400 end,
    color = function() return active() and C.accentText() or C.text() end,
    visible = (values.text or "") ~= "",
  }
  local glyph = kit.glyph {
    glyph = values.icon or "", size = 12,
    color = function() return active() and C.accentText() or C.accent() end,
    visible = (values.icon or "") ~= "",
  }
  local row = ui.Row { anchors = { center_in = true }, gap = 6, align = "center", glyph, label }
  return ui.Rect {
    width = values.width or function() return (row.layout_width or 0) + 2 * padding end,
    height = height,
    radius = height / 2,
    opacity = function() return val(enabled) and 1 or 0.4 end,
    color = function()
      if active() then return hovered:get() and C.accentHover() or C.accent() end
      return hovered:get() and C.islandSurfaceHover or C.islandSurface
    end,
    border_width = 1,
    border_color = function()
      if active() or hovered:get() then return C.accent() end
      return C.islandBorder
    end,
    behavior = { color = fast(), border_color = fast(), opacity = fast() },
    row,
    M.hit { hovered = hovered, enabled = enabled, on_click = values.on_click },
  }
end

--- A square icon button, transparent until hovered: IconButton.
--- `icon`, `icon_size` (13), `width` (32), `height` (28), `radius`,
--- `active`, `icon_color`, `enabled`.
function M.icon_button(values)
  local hovered = signal("icon", false)
  local active = bind(values.active or false)
  local enabled = values.enabled == nil and true or values.enabled
  local width, height = values.width or 32, values.height or 28
  return ui.Rect {
    width = width, height = height,
    radius = values.radius or theme.radius_small,
    opacity = function() return val(enabled) and 1 or (values.dim_opacity or 0.35) end,
    color = function()
      if active() then return C.accent() end
      return hovered:get() and C.islandSurfaceHover or "#00000000"
    end,
    border_width = 1,
    border_color = function()
      if active() then return C.accent() end
      return hovered:get() and C.islandBorder or "#00000000"
    end,
    behavior = { color = fast(), border_color = fast() },
    kit.glyph {
      anchors = { center_in = true },
      glyph = values.icon, size = values.icon_size or 13,
      color = function()
        if active() then return C.accentText() end
        if hovered:get() then return C.accent() end
        return val(values.icon_color) or C.text()
      end,
      behavior = { color = fast() },
    },
    M.hit { hovered = hovered, enabled = enabled, on_click = values.on_click },
  }
end

--- A text link that lights under the pointer ("Rescan", "Clear").
function M.link(values)
  local hovered = signal("link", false)
  local enabled = values.enabled == nil and true or values.enabled
  local label = kit.text {
    text = values.text, size = values.size or theme.size.small,
    color = function() return hovered:get() and val(enabled) and C.accent() or C.textMuted() end,
    behavior = { color = fast() },
  }
  return ui.Item {
    anchors = values.anchors,
    width = function() return label.layout_width or 0 end,
    height = function() return label.layout_height or 0 end,
    visible = values.visible,
    label,
    M.hit { anchors = { fill = true, margins = -6 }, hovered = hovered, enabled = enabled,
      on_click = values.on_click },
  }
end

--- The knob slides rather than jumps: ToggleSwitch. `checked`, `on_toggled(checked)`.
function M.switch(values)
  local checked = bind(values.checked or false)
  local enabled = values.enabled == nil and true or values.enabled
  local w, h = values.width or 40, values.height or 22
  return ui.Rect {
    width = w, height = h, radius = h / 2,
    opacity = function() return val(enabled) and 1 or 0.45 end,
    color = function() return checked() and C.accent() or C.islandSurfaceHover end,
    border_width = 1,
    border_color = function() return checked() and C.accent() or C.islandBorder end,
    behavior = { color = fast(), border_color = fast(), opacity = fast() },
    ui.Rect {
      y = 3, width = h - 6, height = h - 6, radius = (h - 6) / 2,
      x = function() return checked() and (w - (h - 6) - 3) or 3 end,
      color = function() return checked() and C.accentText() or C.textMuted() end,
      behavior = { x = fast(), color = fast() },
    },
    M.hit { enabled = enabled, on_click = function()
      if values.on_toggled then values.on_toggled(not checked()) end
    end },
  }
end

-- ---------------------------------------------------------------- sliders --

--- Maps a drag across `width` to a value between `from` and `to`, and calls
--- `on_moved` with it on press and while held.
local function drag_area(values)
  local held = signal("drag", false)
  local width = bind(values.width)
  local function at(x)
    local w = math.max(1, width())
    local f = math.max(0, math.min(1, x / w))
    return math.floor(values.from + f * (values.to - values.from) + 0.5)
  end
  local enabled = values.enabled == nil and true or values.enabled
  return ui.MouseArea {
    anchors = { fill = true },
    cursor = function() return val(enabled) and "pointer" or "default" end,
    on_entered = function() if values.hovered then values.hovered:set(true) end end,
    on_exited = function() if values.hovered then values.hovered:set(false) end end,
    on_pressed = function(_, _, x)
      if not val(enabled) then return end
      held:set(true)
      values.on_moved(at(x))
    end,
    on_dragged = function(_, _, _, _, x)
      if held:get() then values.on_moved(at(x)) end
    end,
    on_released = function() held:set(false) end,
    on_wheel = function(_, _, _, _, _, steps)
      if not val(enabled) or not steps or steps == 0 then return end
      local current = tonumber(val(values.value)) or 0
      local step = values.step or 5
      values.on_moved(math.max(values.from, math.min(values.to, current - steps * step)))
    end,
  }
end
M.drag_area = drag_area

--- The track is the control: SliderRow. The whole row fills with the level,
--- the icon inside it mutes. `width`, `height` (40), `icon`, `value`,
--- `from` (0), `to` (100), `unit` ("%"), `available`, `dimmed`,
--- `on_moved(value)`, `on_icon(button)`.
function M.slider_row(values)
  local width = bind(values.width)
  local height = values.height or 40
  local value = bind(values.value or 0)
  local from, to = values.from or 0, values.to or 100
  local dimmed = bind(values.dimmed or false)
  local available = values.available == nil and true or values.available
  local hovered = signal("slider", false)
  local position = function()
    return math.max(0, math.min(1, ((tonumber(value()) or 0) - from) / math.max(1, to - from)))
  end
  return ui.Rect {
    width = width, height = height, radius = height / 2,
    color = C.islandSurface,
    border_width = 1,
    border_color = function() return hovered:get() and C.islandBorder or "#00000000" end,
    opacity = function() return val(available) and 1 or 0.45 end,
    behavior = { border_color = fast() },
    ui.Rect {
      x = 0, y = 0, height = height, radius = height / 2,
      width = function() return math.max(height, width() * position()) end,
      color = function() return dimmed() and C.islandSurfaceHover or C.accent() end,
      opacity = function() return dimmed() and 1 or 0.9 end,
      behavior = { width = fast(), color = fast() },
    },
    drag_area {
      width = width, from = from, to = to, value = value, enabled = available,
      hovered = hovered, on_moved = values.on_moved or function() end,
    },
    kit.text {
      anchors = { right = true, right_margin = 14, vertical_center = true },
      text = function() return tostring(value()) .. (values.unit or "%") end,
      size = theme.size.small, weight = 600,
      color = function() return dimmed() and C.textMuted() or C.text() end,
    },
    ui.Item {
      anchors = { left = true, left_margin = 13, vertical_center = true },
      width = 18, height = 18,
      kit.glyph {
        anchors = { center_in = true },
        glyph = values.icon, size = 15,
        color = function() return dimmed() and C.textMuted() or C.accentText() end,
      },
      M.hit { anchors = { fill = true, margins = -6 }, on_click = values.on_icon },
    },
  }
end

--- A setting row with a continuous value: SettingSlider. The name on the
--- left, the track and its figure at the right. `width`, `label`, `value`,
--- `from`, `to`, `step`, `decimals`, `unit`, `reading`, `on_moved(value)`.
--- (The figure is a reading, not the original's editable field.)
function M.setting_slider(values)
  local width = bind(values.width)
  local value = bind(values.value or 0)
  local from, to = values.from or 0, values.to or 100
  local track_w = values.track_width or 240
  local hovered = signal("setting", false)
  local position = function()
    return math.max(0, math.min(1, ((tonumber(value()) or 0) - from) / math.max(1e-9, to - from)))
  end
  local reading = values.reading or function()
    return string.format("%." .. (values.decimals or 0) .. "f", tonumber(value()) or 0) .. (values.unit or "")
  end
  return ui.Item {
    width = width, height = 48,
    ui.Rect { anchors = { left = true, right = true, top = true }, height = 1, color = C.hairline },
    kit.text {
      anchors = { left = true, left_margin = 14, vertical_center = true },
      text = values.label, size = theme.size.small,
    },
    kit.text {
      anchors = { right = true, right_margin = 14, vertical_center = true },
      text = reading, mono = true, size = theme.size.small, width = 48,
      horizontal_alignment = "right",
      color = C.textMuted,
    },
    ui.Item {
      anchors = { right = true, right_margin = 14 + 48 + 12, vertical_center = true },
      width = track_w, height = 16,
      ui.Rect {
        anchors = { left = true, right = true, vertical_center = true }, height = 4, radius = 2,
        color = C.islandSurfaceHover,
        ui.Rect { x = 0, y = 0, height = 4, radius = 2, color = C.accent,
          width = function() return track_w * position() end },
      },
      ui.Rect {
        anchors = { vertical_center = true },
        x = function() return track_w * position() - (hovered:get() and 8 or 6.5) end,
        width = function() return hovered:get() and 16 or 13 end,
        height = function() return hovered:get() and 16 or 13 end,
        radius = 8, color = C.accentText, border_width = 3, border_color = C.accent,
        behavior = { width = fast(), height = fast() },
      },
      drag_area {
        width = track_w, from = from, to = to, value = value, step = values.step or 1,
        hovered = hovered, on_moved = values.on_moved or function() end,
      },
    },
  }
end

-- ------------------------------------------------------------- segmented --

--- Two or three options, all visible: SegmentedControl. `options` is
--- `{ { id, label, icon? } }`; `current` the chosen id; `on_selected(id)`.
function M.segmented(values)
  local current = bind(values.current or "")
  local height = values.height or 28
  local children = { gap = 2, align = "center", anchors = { center_in = true } }
  for _, option in ipairs(values.options or {}) do
    local hovered = signal("segment", false)
    local glyph = (option.icon or "") ~= ""
    local active = function() return current() == option.id end
    local label = glyph
      and kit.glyph { anchors = { center_in = true }, glyph = option.icon,
        size = values.icon_size or theme.size.large,
        color = function()
          if active() then return C.accentText() end
          return hovered:get() and C.text() or C.textMuted()
        end }
      or kit.text { anchors = { center_in = true }, text = option.label, size = theme.size.small,
        weight = function() return active() and 600 or 400 end,
        color = function() return active() and C.accentText() or C.textMuted() end }
    children[#children + 1] = ui.Rect {
      height = height - 6,
      width = glyph and (height - 6) or function() return math.max(64, (label.layout_width or 0) + 20) end,
      radius = theme.radius_small - 2,
      color = function()
        if active() then return C.accent() end
        return hovered:get() and C.islandBorder or "#00000000"
      end,
      behavior = { color = fast() },
      label,
      M.hit { hovered = hovered, on_click = function()
        if values.on_selected then values.on_selected(option.id) end
      end },
    }
  end
  local row = ui.Row(children)
  return ui.Rect {
    anchors = values.anchors, visible = values.visible,
    width = function() return (row.layout_width or 0) + 6 end,
    height = height, radius = theme.radius_small,
    color = C.islandSurfaceHover, border_width = 1, border_color = C.islandBorder,
    row,
  }
end

-- ------------------------------------------------------------------ tile --

--- One toggle: QuickTile. `width`, `height`, `icon`, `label`, `detail`,
--- `active`, `available`, `expandable`, `on_toggled`, `on_expanded`.
function M.quick_tile(values)
  local hovered = signal("tile", false)
  local active = bind(values.active or false)
  local available = bind(values.available == nil and true or values.available)
  local expandable = bind(values.expandable or false)
  local width = bind(values.width)
  local chevron_hover = signal("chevron", false)
  return ui.Rect {
    x = values.x, y = values.y,
    width = width, height = values.height,
    radius = theme.radius_medium,
    opacity = function() return available() and 1 or 0.45 end,
    color = function() return hovered:get() and C.islandSurfaceHover or C.islandSurface end,
    border_width = 1,
    border_color = function() return active() and C.accent() or C.islandBorder end,
    behavior = { color = fast(), border_color = fast(), opacity = fast() },
    M.hit { hovered = hovered, enabled = available, on_click = values.on_toggled },
    ui.Row {
      anchors = { left = true, left_margin = 12, vertical_center = true },
      gap = 11, align = "center",
      ui.Rect {
        width = 34, height = 34, radius = 17,
        color = function() return active() and C.accent() or C.islandSurfaceHover end,
        behavior = { color = fast() },
        kit.glyph {
          anchors = { center_in = true }, glyph = values.icon, size = 16,
          color = function() return active() and C.accentText() or C.textMuted() end,
          behavior = { color = fast() },
        },
      },
      ui.Column {
        gap = 1,
        kit.text {
          text = values.label, size = theme.size.small, weight = 600, elide = "right",
          width = function() return math.max(10, width() - 12 - 34 - 11 - 12 - (expandable() and 16 or 0)) end,
        },
        kit.text {
          text = values.detail, size = theme.size.label, elide = "right",
          width = function() return math.max(10, width() - 12 - 34 - 11 - 12 - (expandable() and 16 or 0)) end,
          color = function() return active() and C.accent() or C.textMuted() end,
          behavior = { color = fast() },
        },
      },
    },
    -- Last, so it sits above the tile's own area.
    ui.Item {
      anchors = { right = true, top = true, bottom = true }, width = 28,
      visible = expandable,
      kit.glyph {
        anchors = { center_in = true }, glyph = "󰅂", size = 13,
        color = function() return chevron_hover:get() and C.accent() or C.textMuted() end,
        behavior = { color = fast() },
      },
      M.hit { hovered = chevron_hover, enabled = available, on_click = values.on_expanded },
    },
  }
end

--- A one-pixel rule.
function M.hairline(values)
  local out = { height = 1, color = C.hairline }
  for k, v in pairs(values or {}) do out[k] = v end
  return ui.Rect(out)
end

return M
