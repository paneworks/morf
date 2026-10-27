-- The default kit's Range skin, in the libadwaita manner: a scale's thin
-- pill trough with the accent up to a white round knob, marks under it, an
-- osd scale on its dark pill; a spin button's value in an entry's well
-- with minus and plus at its end; an overlay scroll bar's slim pill.
-- The behaviour -- drags, snapping, keys -- is the archetype's.
local morf = require("morf")
local ui = require("morf.ui")

return function(S, theme, draw)
  local P = theme.P
  local R = theme.radius
  local get, quick, ring = draw.get, draw.quick, draw.ring
  local has = require("lib.kit.archetypes.press").has
  local function clamp01(v) v = tonumber(v) or 0 return v < 0 and 0 or (v > 1 and 1 or v) end
  local function num(v, d) v = get(v) return type(v) == "number" and v or d end
  local function trim(v) local s = ("%.2f"):format(v) s = s:gsub("0+$", ""):gsub("%.$", "") return s end
  --- The soft shade a raised knob casts.
  local function shadow() local p = P() return morf.color("#000000"):alpha(p.dark and 0.5 or 0.22) end
  local function knob_edge() local p = P() return p.strong and p.window_fg or p.shade:alpha(p.dark and 0.6 or 0.8) end
  --- Where a position is drawn: right to left flips it.
  local function vis(t, at) return t.mirrored and 1 - at or at end

  --- The round white knob, `K` across, centred on (`x`, `y`) (bindings).
  local function knob(t, K, x, y, id)
    return ui.Rect { id = id, width = K, height = K, radius = K / 2,
      x = function() return x() - K / 2 end, y = function() return y() - K / 2 end,
      color = function() local p = P() return t.down and p.knob:mix(p.window_fg, 0.08) or p.knob end,
      border_width = 1, border_color = knob_edge, shadow_color = shadow, shadow_blur = 4, shadow_offset_y = 1 }
  end

  -- A scale's `marks`: { value = v, label = s } or plain values, a short
  -- line under the trough for each, the ones the accent passed in its ink.
  local function marks_of(t, spec, x0, travel, y)
    local from, to = num(spec.from, 0), num(spec.to, 1)
    local node = ui.Item {}
    for _, m in ipairs(get(spec.marks) or {}) do
      local value = type(m) == "table" and m.value or m
      local at = clamp01((value - from) / math.max(1e-9, to - from))
      ui.reparent(ui.Rect { width = 2, height = 6, radius = 1, y = function() return get(y) end,
        x = function() return x0 + travel() * vis(t, at) - 1 end,
        color = function()
          local p = P()
          if clamp01(t.position) >= at - 1e-6 then return p.accent_bg end
          return p.strong and p.window_fg or p.window_fg:alpha(0.3)
        end }, node)
      if type(m) == "table" and m.label then
        ui.reparent(draw.text { y = function() return get(y) + 8 end, width = 60, horizontal_alignment = "center", text = m.label,
          x = function() return x0 + travel() * vis(t, at) - 30 end,
          font_size = theme.size.small, color = function() return P().dim_fg end }, node)
      end
    end
    return node
  end

  --- GtkScale: `orientation`, `digits` (the reading after it; left out,
  --- none), `marks`, `has_origin` (the accent from the start; false, none),
  --- `icon` before it; `osd` on its dark pill.
  local function scale(t, spec)
    local TR, K = 4, 20
    local osd = has(spec, "osd")
    if spec.orientation == "vertical" then
      local function VW() return get(spec.width) or 30 end
      local function VH() return get(spec.height) or 160 end
      local function travel() return math.max(0, VH() - K) end
      local function hy() return K / 2 + travel() * clamp01(t.visual_position) end
      local function cx() return VW() / 2 end
      return {
        track = ui.Item { x = 0, y = K / 2, width = VW, height = travel },
        background = ui.Rect { x = function() return cx() - TR / 2 end, y = K / 2, width = TR, height = travel,
          radius = TR / 2, color = function() return P().track end },
        fill = ui.Rect { x = function() return cx() - TR / 2 end, width = TR, radius = TR / 2,
          visible = spec.has_origin ~= false,
          color = function() return P().accent_bg end,
          y = hy, height = function() return math.max(0, VH() - K / 2 - hy()) end },
        handle = knob(t, K, cx, hy),
        second_handle = ring(t, R.small),
      }
    end
    local function W() return get(spec.width) or 200 end
    local function Hh() return get(spec.height) or (osd and 44 or 32) end
    local left = (spec.icon and 28 or 0) + (osd and 14 or 0)
    local right = (spec.digits and 44 or 0) + (osd and 14 or 0)
    local x0 = left + K / 2
    local function travel() return math.max(1, W() - left - right - K) end
    local function cy() return math.floor(Hh() / 2) end
    local function hx() return x0 + travel() * clamp01(t.visual_position) end
    local slots = {
      track = ui.Item { x = x0, y = 0, width = travel, height = Hh },
      background = ui.Item { width = W, height = Hh,
        osd and ui.Rect { anchors = { fill = true }, radius = function() return Hh() / 2 end,
          color = function() local p = P() return p.strong and p.window_fg or morf.color("#000000"):alpha(0.65) end } or nil,
        ui.Rect { x = x0 - TR / 2, y = function() return cy() - TR / 2 end,
          width = function() return travel() + TR end, height = TR, radius = TR / 2,
          color = function() return osd and morf.color("#ffffff"):alpha(0.25) or P().track end } },
      fill = ui.Rect { id = spec.id and spec.id .. "-level", x = x0 - TR / 2, y = function() return cy() - TR / 2 end,
        height = TR, radius = TR / 2, visible = spec.has_origin ~= false,
        width = function() return math.max(0, hx() - x0 + TR) end,
        color = function() return osd and morf.color("#ffffff") or P().accent_bg end },
      handle = knob(t, K, hx, cy, spec.id and spec.id .. "-handle"),
      second_handle = ring(t, R.small),
    }
    if spec.marks then slots.ticks = marks_of(t, spec, x0, travel, function() return cy() + 10 end) end
    if spec.icon then
      slots.content = draw.icon(spec.icon, 18, function() return osd and morf.color("#ffffff") or P().window_fg end,
        { x = osd and 14 or 2, y = function() return cy() - 11 end })
    end
    if spec.digits then
      local digits = spec.digits
      slots.value_label = draw.text { id = spec.id and spec.id .. "-value", x = function() return W() - right + 4 end,
        width = 40, horizontal_alignment = "right", y = function() return cy() - 10 end, height = 20,
        text = function()
          if type(spec.format) == "function" then return spec.format(t.value or 0) end
          return ("%." .. digits .. "f"):format(t.value or 0)
        end,
        font_size = theme.size.small, color = function() return P().dim_fg end }
    end
    return slots
  end

  local function stepper(send, icon, event, x, y, size)
    return require("lib.kit.archetypes.press").make("button", { x = x, y = y, width = size, height = size,
      icon = icon, variant = "flat", accessible_name = event, on_clicked = function() send(event) end })
  end

  --- GtkSpinButton: the value in an entry's well, minus and plus at its
  --- end; `format(v)` writes the value (`digits` decimals otherwise).
  local function spin_button(t, spec, send)
    local W, Hh = num(spec.width, 150), num(spec.height, 34)
    local B = Hh - 4
    local digits = spec.digits or (num(spec.step, 1) >= 1 and 0 or 2)
    local format = spec.format or function(v)
      if digits == 0 then return ("%d"):format(math.floor(v + 0.5)) end
      return trim(v)
    end
    return {
      track = ui.Item { width = W - 2 * B, height = Hh },
      background = ui.Rect { width = W, height = Hh, radius = R.small,
        color = function() local p = P() return p.window_fg:alpha(p.dark and 0.08 or 0.07) end,
        border_width = function() return P().strong and 1 or 0 end, border_color = function() return P().border end },
      value_label = draw.text { x = 10, y = 0, width = W - 2 * B - 14, height = Hh, vertical_alignment = "center",
        text = function() return format(t.value or 0) end },
      decrease = stepper(send, "remove", "decrease", W - 2 * B - 2, 2, B),
      increase = stepper(send, "add", "increase", W - B - 2, 2, B),
      ticks = ui.Rect { x = W - B - 2, y = 8, width = 1, height = Hh - 16, color = function() return P().border end },
      second_handle = ring(t, R.small),
    }
  end

  --- GtkScrollbar, overlaid: a slim pill the length of the view's share,
  --- wider under the pointer.
  local function scrollbar(t, spec)
    local vertical = spec.orientation == "vertical"
    local function extent() return math.max(0, vertical and t.height or t.width) end
    local function length() return math.min(extent(), math.max(24, clamp01(get(spec.size) or 1) * extent())) end
    local function thickness() return math.min(vertical and t.width or t.height, (t.hovered or t.down) and 8 or 4) end
    local function offset() return t.visual_position * (extent() - length()) end
    return {
      track = ui.Item { anchors = { fill = true } },
      handle = ui.Rect { anchors = vertical and { right = true } or { bottom = true }, radius = 4,
        width = vertical and thickness or length, height = vertical and length or thickness,
        x = not vertical and offset or nil, y = vertical and offset or nil,
        border_width = 1, border_color = function() return P().scrollbar_outline:alpha(0.4) end,
        color = function() local p = P() return p.window_fg:alpha((t.hovered or t.down) and 0.5 or (p.strong and 0.7 or 0.3)) end,
        behavior = { [vertical and "width" or "height"] = quick() } },
    }
  end

  function S.Range(t, spec, _, send)
    if spec.widget == "spin_button" then return spin_button(t, spec, send) end
    if spec.widget == "scrollbar" then return scrollbar(t, spec) end
    return scale(t, spec)
  end
end
