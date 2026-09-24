-- The small pieces everything is made of: text in the shell's faces, a
-- glyph, a hover-lit button, a capsule.
--
-- impasto's components/ folder, at the level most files reach for it. Each
-- takes a table like a morf node does and returns a node, so a caller can
-- add any property the piece does not know about.

local ui = require("morf.ui")
local theme = require("theme")

local C = theme.color
local kit = {}

local function merge(defaults, values)
  local out = {}
  for k, v in pairs(defaults) do out[k] = v end
  for k, v in pairs(values or {}) do out[k] = v end
  return out
end

--- Text in the UI face. `size` is a pixel size; `weight` 400..900;
--- `mono` switches to the mono face (which carries the glyphs).
function kit.text(values)
  local size = values.size or theme.size.regular
  local mono = values.mono
  local out = merge({
    font_family = function() return mono and theme.font_mono() or theme.font() end,
    font_size = size,
    font_weight = values.weight or 400,
    color = C.text,
  }, values)
  out.size, out.weight, out.mono = nil, nil, nil
  return ui.Text(out)
end

--- A Nerd Font glyph, centred in a square `box` wide.
function kit.glyph(values)
  local size = values.size or 14
  local out = merge({
    font_family = function() return theme.font_mono() end,
    font_size = size,
    color = C.text,
    horizontal_alignment = "center",
  }, values)
  out.size = nil
  out.text = values.glyph or values.text
  out.glyph = nil
  return ui.Text(out)
end

local hover_count = 0
--- A hover signal of its own, for pieces that light under the pointer.
function kit.hover_signal(name)
  hover_count = hover_count + 1
  return morf.signal("impasto.hover." .. (name or "") .. hover_count, false)
end

--- A rounded, hover-lit button with one child (or none). `on_click`,
--- `on_right_click`, `color`, `hover_color`, `radius`. Returns the node and
--- its hover signal.
function kit.button(values)
  local hovered = kit.hover_signal("button")
  local pressed = kit.hover_signal("pressed")
  local rest = values.color or C.surface
  local lit = values.hover_color or C.surfaceHover
  local on_click, on_right_click = values.on_click, values.on_right_click
  local out = merge({
    radius = values.radius or theme.radius_medium,
    color = function()
      local base = type(rest) == "function" and rest() or rest
      local over = type(lit) == "function" and lit() or lit
      return hovered:get() and over or base
    end,
    scale = function() return pressed:get() and 0.96 or 1 end,
    behavior = { color = theme.behave("fast"), scale = theme.behave("fast") },
  }, values)
  out.on_click, out.on_right_click, out.hover_color = nil, nil, nil
  out[#out + 1] = ui.MouseArea {
    anchors = { fill = true },
    cursor = "pointer",
    on_entered = function() hovered:set(true) end,
    on_exited = function() hovered:set(false) pressed:set(false) end,
    on_pressed = function() pressed:set(true) end,
    on_released = function() pressed:set(false) end,
    on_clicked = function(button)
      if button == "right" then
        if on_right_click then on_right_click() end
      elseif on_click then
        on_click()
      end
    end,
  }
  return ui.Rect(out), hovered
end

--- A circle with a glyph in it: impasto's IconButton.
function kit.icon_button(values)
  local d = values.diameter or 32
  local glyph = kit.glyph {
    glyph = values.glyph, size = values.glyph_size or 15,
    color = values.glyph_color or C.text,
    anchors = { center_in = true },
  }
  return kit.button {
    width = d, height = d, radius = d / 2,
    color = values.color or C.surface, hover_color = values.hover_color,
    on_click = values.on_click, on_right_click = values.on_right_click,
    glyph,
  }
end

--- A capsule: the bar's grouping shape.
function kit.capsule(values)
  return ui.Rect(merge({
    height = function() return theme.capsule_height() end,
    radius = function() return theme.capsule_height() / 2 end,
    color = function() return theme.color.island end,
    border_width = 1,
    border_color = theme.color.islandBorder,
  }, values))
end

return kit
