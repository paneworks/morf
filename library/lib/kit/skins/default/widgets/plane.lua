-- The default kit's looks for the Plane widgets beyond the colour plane
-- (skins.lua), in the libadwaita manner: a view-coloured field with
-- hairlines, the accent for what moves, white knobs with a shade under
-- them. A hue wheel is a ring of hues, a joystick a well whose stick
-- springs home (the archetype's `spring`; the cap's behaviour carries it),
-- its stalk a distance field tracking the cap. The behaviour is the
-- archetype's (crates/morf-kit/src/plane.rs).
local morf = require("morf")
local ui = require("morf.ui")

return function(S, theme, M)
  local P = theme.P
  local R = theme.radius
  local function get(v) if type(v) == "function" then return v() end return v end
  local function clamp01(v) v = tonumber(v) or 0 return v < 0 and 0 or (v > 1 and 1 or v) end
  local function num(v, d) v = get(v) return type(v) == "number" and v or d end
  local function quick() return { duration = theme.duration.small, easing = theme.ease.standard } end

  local function full(slots)
    for _, name in ipairs { "background", "field", "crosshair", "handle", "content" } do
      if slots[name] == nil then slots[name] = ui.Item {} end
    end
    return slots
  end
  local function ring(t, radius, box)
    return function()
      local props = { z = 50, color = "transparent", radius = radius, border_width = 2,
        border_color = function() local p = P() return p.strong and p.focus or p.focus:alpha(0.6) end,
        visible = function() return t.visual_focus end }
      if box then props.x, props.y, props.width, props.height = box[1], box[2], box[3], box[4]
      else props.anchors = { fill = true } end
      return ui.Rect(props)
    end
  end
  local function path(w, h, props)
    props.width, props.height, props.view_box = w, h, { 0, 0, w, h }
    if props.fill_color == nil then props.fill_color = "transparent" end
    return ui.Path(props)
  end
  local function grid_d(w, h, cols, rows)
    local d = {}
    for k = 1, cols - 1 do d[#d + 1] = ("M%.1f 0 V%g"):format(w * k / cols, h) end
    for k = 1, rows - 1 do d[#d + 1] = ("M0 %.1f H%g"):format(h * k / rows, w) end
    return table.concat(d, " ")
  end
  local function knob_edge() local p = P() return p.strong and p.ink or p.shade:alpha(p.dark and 0.6 or 0.8) end
  local function shadow() local p = P() return morf.color("#000000"):alpha(p.dark and 0.5 or 0.22) end
  local function hairline() local p = P() return p.strong and p.ink:alpha(0.5) or p.ink:alpha(0.08) end
  local function field_box(W, H, radius)
    return ui.Rect { width = W, height = H, radius = radius or R.medium,
      color = function() local p = P() return p.dark and p.view or p.view end,
      border_width = function() return P().strong and 2 or 1 end, border_color = function() return P().border end }
  end
  --- A line from a fixed point to a moving one: a bar turned about its
  --- start, so nothing is redrawn as it moves.
  local function segment(ax, ay, bx, by, width, color)
    return ui.Rect { height = width, color = color, transform_origin_x = 0, transform_origin_y = 0.5,
      x = function() return ax() end, y = function() return ay() - width / 2 end,
      width = function() local dx, dy = bx() - ax(), by() - ay() return math.sqrt(dx * dx + dy * dy) end,
      rotation = function() return math.deg(math.atan(by() - ay(), bx() - ax())) end }
  end
  local function dot(K, x, y, props)
    props = props or {}
    props.width, props.height, props.radius = K, K, K / 2
    props.x = function() return x() - K / 2 end
    props.y = function() return y() - K / 2 end
    props.color = props.color or function() return P().knob end
    props.border_width = props.border_width or 1
    props.border_color = props.border_color or knob_edge
    props.shadow_color, props.shadow_blur, props.shadow_offset_y = shadow, 4, 1
    return ui.Rect(props)
  end

  --- A hue wheel: a ring of every hue (`x`: the angle, clockwise from
  --- red at twelve o'clock), a knob on the ring filled with the hue it
  --- stands on, and the chosen hue as a disc in the middle.
  local function hue_at(f) return morf.color(("hsl(%d, 100%%, 50%%)"):format(math.floor(clamp01(f) * 360 + 0.5) % 360)) end
  local function hues()
    local stops = {}
    for k = 0, 12 do stops[#stops + 1] = ("hsl(%d, 100%%, 50%%)"):format(k * 30 % 360) end
    return stops
  end
  function S.hue_wheel(t, spec)
    local Sz = math.min(num(spec.width, 180), num(spec.height, 180))
    local c, T = Sz / 2, Sz * 0.14
    local rm = c - T / 2
    local function angle() return clamp01(t.position_x) * 2 * math.pi end
    local function hx() return c + rm * math.sin(angle()) end
    local function hy() return c - rm * math.cos(angle()) end
    local K = T + 8
    return full {
      background = ui.Item { width = Sz, height = Sz },
      field = ui.Item { width = Sz, height = Sz,
        ui.Sdf { anchors = { fill = true }, gradient = { kind = "conic", stops = hues() },
          ui.SdfShape { shape = "circle", anchors = { fill = true } },
          ui.SdfShape { shape = "circle", anchors = { fill = true, margins = T }, operation = "subtract" } },
        ui.Rect { x = c - rm + T, y = c - rm + T, width = 2 * (rm - T), height = 2 * (rm - T), radius = rm - T,
          color = function() return hue_at(t.position_x) end,
          border_width = 1, border_color = function() return P().border end } },
      handle = dot(K, hx, hy, { color = function() return hue_at(t.position_x) end, border_width = 3,
        border_color = function() return P().knob end,
        ui.Rect { x = -1, y = -1, width = K + 2, height = K + 2, radius = K / 2 + 1, color = "transparent",
          border_width = 1, border_color = knob_edge } }),
      crosshair = ring(t, c, { 0, 0, Sz, Sz }),
    }
  end

  --- An XY pad: a gridded field with its axes, lines through the knob to
  --- the edges, the knob in the accent.
  function S.xy_pad(t, spec)
    local W, H = num(spec.width, 180), num(spec.height, 180)
    local function hx() return clamp01(t.visual_x) * W end
    local function hy() return clamp01(t.visual_y) * H end
    return full {
      background = ui.Item { width = W, height = H },
      field = ui.Item { width = W, height = H, field_box(W, H),
        path(W, H, { d = grid_d(W, H, 4, 4), stroke_width = 1, stroke_color = hairline }),
        path(W, H, { d = ("M%g 6 V%g M6 %g H%g"):format(W / 2, H - 6, H / 2, W - 6), stroke_width = 1,
          stroke_color = function() local p = P() return p.strong and p.ink or p.ink:alpha(0.2) end }) },
      crosshair = ui.Item { width = W, height = H,
        ui.Rect { x = 1, width = W - 2, height = 1, y = hy, color = function() return P().accent:alpha(0.5) end },
        ui.Rect { y = 1, width = 1, height = H - 2, x = hx, color = function() return P().accent:alpha(0.5) end },
        ring(t, R.medium)() },
      handle = dot(18, hx, hy, { color = function() return P().accent end, border_width = 3,
        border_color = function() return P().knob end }),
    }
  end

  --- A panner: a round room with rings at a third and two thirds, the
  --- listener in the middle, a line from them to the source.
  function S.pan_pad(t, spec)
    local Sz = math.min(num(spec.width, 180), num(spec.height, 180))
    local c = Sz / 2
    local function hx() return clamp01(t.visual_x) * Sz end
    local function hy() return clamp01(t.visual_y) * Sz end
    local function circle(r) return ("M%g %g A%g %g 0 1 1 %g %g A%g %g 0 1 1 %g %g"):format(c, c - r, r, r, c, c + r, r, r, c, c - r) end
    return full {
      background = ui.Item { width = Sz, height = Sz },
      field = ui.Item { width = Sz, height = Sz, field_box(Sz, Sz, c),
        path(Sz, Sz, { d = circle(c / 3) .. " " .. circle(c * 2 / 3) .. (" M%g 4 V%g M4 %g H%g"):format(c, Sz - 4, c, Sz - 4),
          stroke_width = 1, stroke_color = hairline }),
        M.icon("headphones", 20, function() return P().ink_dim end,
          { x = c - 12, y = c - 12, width = 24, height = 24, horizontal_alignment = "center", vertical_alignment = "center" }) },
      crosshair = ui.Item { width = Sz, height = Sz,
        segment(function() return c end, function() return c end, hx, hy, 2, function() return P().accent:alpha(0.6) end),
        ring(t, c, { 0, 0, Sz, Sz })() },
      handle = dot(20, hx, hy, { color = function() return P().accent end, border_width = 3,
        border_color = function() return P().knob end }),
    }
  end

  --- An envelope's breakpoint: the rise from the start to the point and
  --- the fall to the end, a drop line to the floor, the point a square.
  function S.envelope_point(t, spec)
    local W, H = num(spec.width, 220), num(spec.height, 150)
    local pad = 8
    local fw, fh = W - 2 * pad, H - 2 * pad
    local function hx() return pad + clamp01(t.visual_x) * fw end
    local function hy() return pad + clamp01(t.visual_y) * fh end
    local function x0() return pad end
    local function x1() return W - pad end
    local function floor() return H - pad end
    local accent = function() return P().accent end
    return full {
      background = ui.Item { width = W, height = H },
      field = ui.Item { width = W, height = H, field_box(W, H),
        path(W, H, { d = grid_d(W, H, 8, 4), stroke_width = 1, stroke_color = hairline }) },
      crosshair = ui.Item { width = W, height = H,
        ui.Rect { width = 1, x = hx, y = hy, height = function() return floor() - hy() end,
          color = function() return P().accent:alpha(0.35) end },
        segment(x0, floor, hx, hy, 2, accent),
        segment(hx, hy, x1, floor, 2, accent),
        ring(t, R.medium)() },
      handle = ui.Rect { width = 14, height = 14, radius = 3,
        x = function() return hx() - 7 end, y = function() return hy() - 7 end,
        color = function() return P().knob end, border_width = 3, border_color = accent },
    }
  end

  --- A joystick: a round well with its cross, a cap on a stalk that
  --- stretches from the middle and springs home when let go.
  function S.joystick(t, spec)
    local Sz = math.min(num(spec.width, 160), num(spec.height, 160))
    local c, K = Sz / 2, math.floor(Sz * 0.3)
    local reach = c - K / 2 - 4
    local function hx() return c + (clamp01(t.visual_x) - 0.5) * 2 * reach end
    local function hy() return c + (clamp01(t.visual_y) - 0.5) * 2 * reach end
    local home = ui.spring { stiffness = 420, damping = 15 }
    local cap = ui.Rect { width = K, height = K, radius = K / 2,
      x = function() return hx() - K / 2 end, y = function() return hy() - K / 2 end,
      behavior = { x = home, y = home }, stretch = { stiffness = 260, damping = 16, scale = 0.08, max = 0.2 },
      color = function() local p = P() return t.down and p.knob:mix(p.ink, 0.06) or p.knob end,
      border_width = 1, border_color = knob_edge, shadow_color = shadow, shadow_blur = 10, shadow_offset_y = 3,
      ui.Rect { x = K / 2 - 6, y = K / 2 - 6, width = 12, height = 12, radius = 6,
        color = function() return P().accent end } }
    return full {
      background = ui.Item { width = Sz, height = Sz },
      field = ui.Item { width = Sz, height = Sz,
        ui.Rect { width = Sz, height = Sz, radius = c,
          color = function() local p = P() return p.ink:alpha(p.wash.button) end,
          border_width = function() return P().strong and 2 or 1 end, border_color = function() return P().border end },
        path(Sz, Sz, { d = ("M%g 10 V%g M10 %g H%g"):format(c, Sz - 10, c, Sz - 10), stroke_width = 1,
          stroke_color = hairline }),
        -- The stalk: the middle and the cap fused by a smooth union.
        ui.Sdf { anchors = { fill = true }, blend = K * 0.9,
          fill_color = function() local p = P() return p.shade:alpha(p.dark and 0.9 or 0.6) end,
          ui.SdfShape { shape = "circle", x = c - 9, y = c - 9, width = 18, height = 18 },
          ui.SdfShape { shape = "circle", operation = "smooth_union", track = cap } } },
      handle = cap,
      crosshair = ring(t, c, { 0, 0, Sz, Sz }),
    }
  end

  --- A minimap: a page in miniature and the frame of what the view shows
  --- over it (`spec.view`: its share of the page, { w, h }).
  function S.minimap_viewport(t, spec)
    local W, H = num(spec.width, 150), num(spec.height, 190)
    local view = get(spec.view) or { 0.5, 0.32 }
    local fw, fh = W * view[1], H * view[2]
    local lines = {}
    local y = 14
    local widths = { 0.8, 0.95, 0.6, 0, 0.9, 0.85, 0.7, 0, 0.92, 0.5, 0.88, 0.76, 0, 0.9, 0.64, 0.8, 0.94, 0.4 }
    for i, f in ipairs(widths) do
      if f > 0 then lines[#lines + 1] = ("M12 %d H%d"):format(y, 12 + math.floor((W - 24) * f)) end
      y = y + (f > 0 and 9 or 7)
      if y > H - 12 then break end
      if i == 4 then y = y + 34 end
    end
    local function fx() return clamp01(t.visual_x) * (W - fw) end
    local function fy() return clamp01(t.visual_y) * (H - fh) end
    return full {
      background = ui.Item { width = W, height = H },
      field = ui.Item { width = W, height = H, field_box(W, H, R.small),
        path(W, H, { d = table.concat(lines, " "), stroke_width = 3, stroke_cap = "round",
          stroke_color = function() local p = P() return p.ink:alpha(p.strong and 0.6 or 0.2) end }),
        ui.Rect { x = 12, y = 56, width = W - 24, height = 26, radius = 3,
          color = function() return P().accent:alpha(0.25) end } },
      crosshair = ui.Item { width = W, height = H,
        ui.Rect { x = fx, y = fy, width = fw, height = fh, radius = 3, color = function() return P().accent:alpha(0.12) end },
        ring(t, R.small)() },
      handle = ui.Rect { x = fx, y = fy, width = fw, height = fh, radius = 3, color = "transparent",
        border_width = 2, border_color = function() return P().accent end },
    }
  end

  --- A crop: a picture, dimmed outside the crop from its fixed corner
  --- (`spec.anchor`, { x, y } fractions) to the handle's, thirds inside,
  --- brackets on every corner -- the dragged one in the accent.
  function S.crop_handle(t, spec)
    local W, H = num(spec.width, 220), num(spec.height, 160)
    local anchor = get(spec.anchor) or { 0.14, 0.16 }
    local ax, ay = anchor[1] * W, anchor[2] * H
    local function hx() return clamp01(t.visual_x) * W end
    local function hy() return clamp01(t.visual_y) * H end
    local function x0() return math.min(ax, hx()) end
    local function x1() return math.max(ax, hx()) end
    local function y0() return math.min(ay, hy()) end
    local function y1() return math.max(ay, hy()) end
    local scrim = function() return morf.color("#000000"):alpha(0.45) end
    local frame = function() return P().knob end
    local hill = ("M0 %g L%g %g L%g %g L%g %g L%g %g L%g %g Z"):format(H, W * 0.28, H * 0.5, W * 0.46, H * 0.72,
      W * 0.68, H * 0.38, W, H * 0.66, W, H)
    local shade = ui.Item { width = W, height = H,
      ui.Rect { x = 0, y = 0, width = W, height = y0, color = scrim },
      ui.Rect { x = 0, y = y1, width = W, height = function() return H - y1() end, color = scrim },
      ui.Rect { x = 0, y = y0, width = x0, height = function() return y1() - y0() end, color = scrim },
      ui.Rect { x = x1, y = y0, width = function() return W - x1() end, height = function() return y1() - y0() end, color = scrim },
      ui.Rect { x = x0, y = y0, width = function() return x1() - x0() end, height = function() return y1() - y0() end,
        color = "transparent", border_width = 1, border_color = frame } }
    for k = 1, 2 do
      ui.reparent(ui.Rect { width = 1, y = y0, height = function() return y1() - y0() end,
        x = function() return x0() + (x1() - x0()) * k / 3 end, color = function() return P().knob:alpha(0.5) end }, shade)
      ui.reparent(ui.Rect { height = 1, x = x0, width = function() return x1() - x0() end,
        y = function() return y0() + (y1() - y0()) * k / 3 end, color = function() return P().knob:alpha(0.5) end }, shade)
    end
    -- Brackets: two bars at each corner, pointing inward.
    local L, T = 16, 3
    local corners = ui.Item { width = W, height = H }
    for _, corner in ipairs { { "x0", "y0" }, { "x1", "y0" }, { "x0", "y1" }, { "x1", "y1" } } do
      local cxf = corner[1] == "x0" and x0 or x1
      local cyf = corner[2] == "y0" and y0 or y1
      local sx = corner[1] == "x0" and 1 or -1
      local sy = corner[2] == "y0" and 1 or -1
      local function dragged() return math.abs(cxf() - hx()) < 0.5 and math.abs(cyf() - hy()) < 0.5 end
      local color = function() local p = P() return dragged() and p.accent or p.knob end
      ui.reparent(ui.Rect { width = L, height = T, color = color,
        x = function() return sx > 0 and cxf() - 1 or cxf() - L + 1 end,
        y = function() return sy > 0 and cyf() - 1 or cyf() - T + 1 end }, corners)
      ui.reparent(ui.Rect { width = T, height = L, color = color,
        x = function() return sx > 0 and cxf() - 1 or cxf() - T + 1 end,
        y = function() return sy > 0 and cyf() - 1 or cyf() - L + 1 end }, corners)
    end
    return full {
      background = ui.Item { width = W, height = H },
      field = ui.ClipRect { width = W, height = H, radius = R.small, color = "transparent",
        ui.Rect { anchors = { fill = true }, gradient = function()
          local p = P() return { angle = 180, stops = { p.info, p.extra } } end },
        ui.Rect { x = W * 0.7, y = H * 0.14, width = H * 0.2, height = H * 0.2, radius = H * 0.1,
          color = function() return P().warning end },
        path(W, H, { d = hill, fill_color = function() local p = P() return p.shade:mix(p.success, 0.4) end }) },
      crosshair = ui.Item { width = W, height = H, shade, ring(t, R.small)() },
      handle = corners,
    }
  end
end
