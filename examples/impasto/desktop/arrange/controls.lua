-- The small controls the arranging cards are made of: a heading, a
-- selectable tile, a slider, a switch and a round colour swatch.
--
-- The inspector's `Heading`, its tiles (every "Shape", "Where", "Face" and
-- "Style" choice is a Rectangle with an accent border when current), the
-- shell's SliderRow and ToggleSwitch, at the size the inspector draws them.
-- Values may be functions, so a binding follows them.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")

local C = theme.color
local M = {}

local function read(v)
  if type(v) == "function" then return v() end
  return v
end
M.read = read

local count = 0
local function signal(name, value)
  count = count + 1
  return morf.signal("impasto.desk.arrange." .. name .. "." .. count, value)
end
M.signal = signal

--- A section's heading.
function M.heading(text, values)
  values = values or {}
  return kit.text {
    text = text, size = theme.size.label, weight = 600, color = C.textMuted,
    height = 14, visible = values.visible,
  }
end

--- A tile that is one choice: `width`, `height`, `current()`, `on_click`,
--- children drawn inside; `dim()` greys it out and refuses the click.
function M.tile(values)
  local hovered = signal("tile", false)
  local out = {
    x = values.x, y = values.y, visible = values.visible,
    width = values.width or 50, height = values.height or 48,
    radius = values.radius or theme.radius_small,
    opacity = function() return read(values.dim) and 0.4 or 1 end,
    color = function()
      if read(values.current) then return C.islandSurfaceHover end
      if hovered:get() then return C.islandSurface end
      return morf.color("transparent")
    end,
    border_width = 1,
    border_color = function() return read(values.current) and C.accent() or C.hairline end,
    behavior = { color = theme.behave("fast"), border_color = theme.behave("fast") },
  }
  for _, child in ipairs(values) do out[#out + 1] = child end
  out[#out + 1] = ui.MouseArea {
    anchors = { fill = true }, cursor = "pointer",
    on_entered = function() hovered:set(true) end,
    on_exited = function() hovered:set(false) end,
    on_clicked = function()
      if read(values.dim) then return end
      if values.on_click then values.on_click() end
    end,
  }
  return ui.Rect(out)
end

--- The small dot a "follow the desk" tile wears in its corner.
function M.default_dot(width)
  return ui.Rect { x = width - 10, y = 4, width = 6, height = 6, radius = 3, color = C.textMuted }
end

--- A slider from `from` to `to`: `get()` reads, `set(value)` writes a whole
--- number; `icon` a glyph on its left, `unit` after the figure.
function M.slider(values)
  local width = values.width
  local icon_w = values.icon and 22 or 0
  local figure_w = 46
  local track_w = width - icon_w - figure_w - 8
  local held = signal("slider", false)
  local from, to = values.from, values.to
  local function fraction()
    local v = tonumber(read(values.get)) or from
    return math.max(0, math.min(1, (v - from) / math.max(1, to - from)))
  end
  local function at(x)
    local f = math.max(0, math.min(1, x / track_w))
    return math.floor(from + f * (to - from) + 0.5)
  end
  local children = {}
  if values.icon then
    children[#children + 1] = kit.glyph { x = 0, y = 0, width = 18, height = values.height or 30,
      vertical_alignment = "center", glyph = values.icon, size = 13, color = C.textMuted }
  end
  local h = values.height or 30
  children[#children + 1] = ui.Item {
    x = icon_w, y = 0, width = track_w, height = h,
    ui.Rect { x = 0, y = h / 2 - 2, width = track_w, height = 4, radius = 2, color = C.islandSurfaceHover },
    ui.Rect { x = 0, y = h / 2 - 2, height = 4, radius = 2, color = C.accent,
      width = function() return math.max(4, fraction() * track_w) end },
    ui.Rect {
      width = 14, height = 14, radius = 7, y = h / 2 - 7,
      x = function() return fraction() * (track_w - 14) end,
      color = C.scrimText,
      border_width = function() return held:get() and 2 or 0 end,
      border_color = C.accent,
    },
    ui.MouseArea {
      anchors = { fill = true }, cursor = "pointer",
      on_pressed = function(_, _, lx) held:set(true) values.set(at(lx)) end,
      on_dragged = function(_, _, _, _, lx) if held:get() then values.set(at(lx)) end end,
      on_released = function() held:set(false) end,
      on_wheel = function(_, _, _, _, _, steps)
        if steps ~= 0 then
          local v = tonumber(read(values.get)) or from
          values.set(math.max(from, math.min(to, math.floor(v - steps * math.max(1, (to - from) / 40) + 0.5))))
        end
      end,
    },
  }
  children[#children + 1] = kit.text {
    mono = true, x = width - figure_w, y = 0, width = figure_w, height = h,
    vertical_alignment = "center", horizontal_alignment = "right",
    size = theme.size.label, color = C.text,
    text = function() return tostring(math.floor((tonumber(read(values.get)) or 0) + 0.5)) .. (values.unit or "") end,
  }
  return ui.Item { width = width, height = h, visible = values.visible, table.unpack(children) }
end

--- An on/off switch: `get()` and `set(on)`.
function M.switch(values)
  return ui.Rect {
    width = 36, height = 20, radius = 10,
    x = values.x, y = values.y,
    color = function() return read(values.get) and C.accent() or C.islandSurfaceHover end,
    behavior = { color = theme.behave("fast") },
    ui.Rect {
      y = 3, width = 14, height = 14, radius = 7, color = C.scrimText,
      x = function() return read(values.get) and 19 or 3 end,
      behavior = { x = theme.behave("fast") },
    },
    ui.MouseArea {
      anchors = { fill = true }, cursor = "pointer",
      on_clicked = function() values.set(not read(values.get)) end,
    },
  }
end

--- A round colour, ringed in the accent when current.
function M.swatch(values)
  return ui.Rect {
    width = values.size or 26, height = values.size or 26, radius = (values.size or 26) / 2,
    color = values.color,
    border_width = function() return read(values.current) and 2 or 1 end,
    border_color = function() return read(values.current) and C.accent() or C.hairline end,
    ui.MouseArea { anchors = { fill = true }, cursor = "pointer", on_clicked = values.on_click },
  }
end

--- A thin rule across `width`.
function M.rule(width)
  return ui.Rect { width = width, height = 1, color = C.hairline }
end

--- A model of none or one row `{ id = value() }`, kept current from a
--- module-level effect: a Repeater over it rebuilds its delegate whenever the
--- value changes, and drops it when the value is "". The model is replaced on
--- the next tick, outside the effect.
function M.keyed(name, value)
  local model = morf.list_model({})
  local seen = nil
  local function apply()
    local now = value()
    if now == seen then return end
    seen = now
    model:replace(now ~= "" and { { id = now } } or {}, "id")
  end
  local ok, err = pcall(morf.effect, "impasto.desk.arrange.keyed." .. name, function()
    value()
    morf.timer(1, apply, false)
  end)
  if not ok then morf.log("warn", "impasto: " .. name .. ": " .. tostring(err)) end
  return model
end

return M
