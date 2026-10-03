-- Tsugumori's skins for the kit's archetypes (lib.kit.skin): square,
-- framed, mechanical -- hatched runs, block handles, tick rulers, rolling
-- labels -- drawn from a control's live state `t`. The behaviour is the
-- archetype's.
local morf = require("morf")
local ui = require("morf.ui")
local stripes = require("themes.tsugumori.stripes")
local stroke = require("themes.tsugumori.strokes")

return function(theme, M, hud)
  local S = {}
  local C = theme.color
  local function get(v) if type(v) == "function" then return v() end return v end
  local function clamp01(v) return math.max(0, math.min(1, v)) end
  local quick = { duration = 140, easing = "out_cubic" }
  local MARK = "M0 0 H7 V1.5 H1.5 V7 H0 Z"

  -- Decoration made the first time the pointer, a press or the keyboard
  -- reaches the control (lib.kit.skin: a slot given as a function): a shell
  -- holds hundreds of controls, most never touched.
  local function lazy(_, build) return build end

  -- The wash a press or the pointer lays over a target, its two
  -- registration marks on the diagonal, and the keyboard's brackets.
  local function feedback(t)
    return lazy(t, function() return ui.Item { anchors = { fill = true }, z = 40,
      ui.Rect { anchors = { fill = true }, color = function() return C.primary end,
        opacity = function() return t.down and .16 or t.hovered and .07 or 0 end, behavior = { opacity = quick } },
      ui.Path { x = 1, y = 1, width = 7, height = 7, view_box = { 0, 0, 7, 7 }, d = MARK,
        fill_color = function() return C.primary end,
        opacity = function() return (t.hovered or t.down) and 1 or 0 end, behavior = { opacity = quick } },
      ui.Path { anchors = { right = true, bottom = true, right_margin = 1, bottom_margin = 1 }, width = 7, height = 7,
        view_box = { 0, 0, 7, 7 }, d = MARK, rotation = 180, fill_color = function() return C.primary end,
        opacity = function() return (t.hovered or t.down) and 1 or 0 end, behavior = { opacity = quick } },
      hud().corners { length = 6, weight = 2, color = function() return C.primary end,
        visible = function() return t.visual_focus end },
    } end)
  end

  -- A tick ruler along `len`: a short tick every 8 px, a long one every 5th.
  local function ruler(x, y, len, size, color)
    local n = math.max(4, math.floor(len / 8))
    local d = {}
    for k = 0, n do
      d[#d + 1] = ("M%.1f 0 V%g "):format(len * k / n, k % 5 == 0 and size or size / 2)
    end
    return ui.Path { x = x, y = y, width = len, height = size, view_box = { 0, 0, len, size },
      d = table.concat(d), fill_color = "transparent", stroke_color = color, stroke_width = 1 }
  end

  -- ----------------------------------------------------------- presses --

  --- A framed button whose caption rolls up through a strip of marks when
  --- the pointer arrives: `icon`, `label`, `color`/`ink`.
  local function pill(t, spec)
    local ink = spec.ink or function() return C.onPrimaryContainer end
    local fill = spec.color or function() return C.primaryContainer end
    local function caption() return tostring(get(spec.label) or ""):upper() end
    local function width() return t.width > 0 and t.width or (get(spec.width) or 0) end
    local measure = M.menu_label { text = caption, font_size = theme.typography.menu, height = 18, opacity = 0 }
    local function natural_width()
      return math.max(1, measure.layout_width or utf8.len(caption()) * theme.typography.menu * .62)
    end
    local function available() return math.max(0, width() - (spec.icon and 36 or 16)) end
    local function label_size()
      return math.max(9, math.min(theme.typography.menu, theme.typography.menu * available() / natural_width()))
    end
    local function text_width() return math.min(available(), natural_width() * label_size() / theme.typography.menu + 2) end
    local strip = ui.Column { y = -36, gap = 0,
      M.menu_label { text = "/ / / / / /", height = 18, color = ink },
      M.menu_label { text = "+ | + | + |", height = 18, color = ink },
      M.menu_label { text = caption, font_size = label_size, height = 18, width = text_width, elide = "right", color = ink },
    }
    local content = ui.Row { anchors = { center_in = true }, gap = 6, align = "center",
      spec.icon and M.icon(spec.icon, 17, ink) or nil,
      ui.Item { id = (spec.id or "pill") .. "-label", width = text_width, height = 18, clip = true,
        visible = function() return text_width() > 0 end, strip },
    }
    local was, running = false, nil
    morf.effect("tsugumori.label." .. (spec.id or tostring(strip)), function()
      local now = t.hovered
      if was == now then return end
      was = now
      if running then running:stop() end
      if now then running = morf.animation.play { { node = strip, property = "y", from = 0, to = -36, duration = 340, easing = "out_cubic" } }
      else strip.y = -36 end
    end, { owner = strip })
    return {
      background = ui.Rect { anchors = { fill = true }, radius = 0, border_width = 1,
        color = function() return t.hovered and fill():mix(ink(), 0.12) or fill() end,
        border_color = function() return t.hovered and stroke(C, "focus") or stroke(C, "idle") end,
        behavior = { color = quick, border_color = quick } },
      content = ui.Item { anchors = { fill = true }, measure, content },
      badge = feedback(t),
    }
  end

  --- A square rail with a block that slides across and takes the accent.
  local function switch(t)
    return {
      background = ui.Rect { anchors = { fill = true }, radius = 0, border_width = 1,
        color = function() return C.surfaceContainerHighest end,
        border_color = function() return t.checked and stroke(C, "focus") or stroke(C, "idle") end },
      indicator = ui.Rect { y = 5, x = function() return t.checked and 28 or 5 end, width = 19, height = 22,
        color = function() return t.checked and C.primary or C.outline end,
        behavior = { x = { duration = 150, easing = "out_cubic" } } },
      badge = feedback(t),
    }
  end

  --- A framed icon toggle in the alert tone while `on`, with a lit tab.
  local function icon(t, spec)
    local w, h = spec.width or 32, spec.height or 32
    local function on() return spec.on and spec.on() == true end
    local alert = M.signal("alert")
    local c = math.max(3, math.floor(math.min(w, h) * .22))
    return {
      background = ui.Path { width = w, height = h, view_box = { 0, 0, w, h }, d = M.frame_path(w, h, c, .5),
        fill_color = function() return on() and alert():alpha(.16) or C.surfaceContainerHigh end,
        stroke_color = function() return on() and alert():alpha(.75) or stroke(C, "idle") end,
        stroke_width = 1, stroke_join = "miter", behavior = { fill_color = quick, stroke_color = quick } },
      icon = M.icon(function() return on() and (spec.icon_on or spec.icon_off) or (spec.icon_off or spec.icon_on) end,
        math.floor(math.min(w, h) * .56), function() return on() and alert() or C.onSurfaceVariant end,
        { anchors = { center_in = true }, fill = on }),
      indicator = ui.Rect { x = w - c - 1, y = 2, width = c - 1, height = 2, color = alert,
        opacity = function() return on() and 1 or 0 end, behavior = { opacity = quick } },
      badge = feedback(t),
    }
  end

  --- A square box, hatched when checked, a bar when partial.
  local function checkbox(t)
    return {
      background = ui.Rect { anchors = { fill = true, margins = 3 }, color = "transparent", border_width = 1,
        border_color = function() return (t.checked or t.partial) and C.primary or stroke(C, "idle") end },
      indicator = ui.Item { anchors = { fill = true, margins = 6 }, clip = true,
        visible = function() return t.checked or t.partial end,
        ui.Rect { anchors = { fill = true }, color = function() return C.primary:alpha(t.partial and .5 or 1) end } },
      badge = feedback(t),
    }
  end

  --- A square frame with a block in it when chosen.
  local function radio(t)
    return {
      background = ui.Rect { anchors = { fill = true, margins = 3 }, color = "transparent", border_width = 1,
        border_color = function() return t.checked and C.primary or stroke(C, "idle") end },
      indicator = ui.Rect { anchors = { center_in = true }, color = function() return C.primary end,
        width = function() return t.checked and 8 or 0 end, height = function() return t.checked and 8 or 0 end,
        behavior = { width = quick, height = quick } },
      badge = feedback(t),
    }
  end

  function S.Press(t, spec)
    local widget = spec.widget
    -- A layout's own area draws itself: the wash, the marks and the
    -- keyboard's brackets here.
    if widget == "area" then return { badge = feedback(t) } end
    if widget == "switch" then return switch(t)
    elseif widget == "icon" then return icon(t, spec)
    elseif widget == "checkbox" or widget == "check_menu_item" then return checkbox(t)
    elseif widget == "radio" or widget == "radio_menu_item" then return radio(t)
    end
    return pill(t, spec)
  end

  -- ------------------------------------------------------------ ranges --

  --- The slider: a faint band, the hatched run up to the value, a tick
  --- ruler under it, a square block handle and the reading at the end.
  local function slider(t, spec)
    local W, H = spec.width, spec.bar_height or 44
    local compact = H < 36
    local isz = compact and 16 or 20
    local left = spec.icon and (isz + 12) or 0
    local right = W - (spec.label ~= false and 50 or 0)
    local span = math.max(1, right - left)
    local band = compact and math.max(6, math.floor(H * .42)) or math.floor(H * .42)
    local by = compact and 4 + math.floor((H - band) / 2) or 4 + math.floor(H * .18)
    local function value() return clamp01(t.visual_position) end
    local function engaged() return t.hovered or t.down end
    local follow = { duration = 90, easing = "out_cubic" }
    local tick = compact and 4 or 6
    local ty = by + band + 3
    local id = spec.id or "slider"
    return {
      track = ui.Rect { id = id .. "-track", x = left, y = by, width = span, height = band,
        color = function() return C.primary:alpha(engaged() and .1 or .06) end,
        border_width = 1, border_color = function() return C.primary:alpha(.18) end, behavior = { color = quick } },
      fill = ui.Item { id = id .. "-level", x = left, y = by, height = band, clip = true,
        width = function() return span * value() end, behavior = { width = follow },
        visible = function() return value() > .002 end,
        ui.Rect { width = span, height = band, color = function() return C.primary:alpha(.2) end },
        stripes.box { width = span, height = band, gap = 6, weight = 2, color = function() return C.primary end } },
      ticks = ty + tick <= H + 8 and ruler(left, ty, span, tick, function() return C.primary:alpha(.32) end) or nil,
      handle = ui.Rect { id = id .. "-handle", y = by - 4, width = 6, height = band + 8,
        x = function() return left + span * value() - 3 end, behavior = { x = follow },
        color = function() return C.primary end },
      decrease = spec.icon and M.icon(spec.icon, isz, function() return engaged() and C.primary or C.onSurfaceVariant end,
        { x = 2, y = by + math.floor((band - isz) / 2) }) or nil,
      value_label = spec.label ~= false and M.text { id = id .. "-value", x = W - 46, y = by + math.floor((band - 18) / 2),
        width = 46, height = 18, horizontal_alignment = "right", font_size = 12,
        color = function() return engaged() and C.primary or C.onSurfaceVariant end,
        text = function() return ("%03d"):format(math.floor(clamp01(t.position) * 100 + .5)) end } or nil,
      second_handle = hud().corners { length = 6, weight = 2, color = function() return C.primary end,
        visible = function() return t.visual_focus end },
    }
  end

  --- The media position: a hatched run with a block head over a faint band
  --- and a tick ruler; it flows between the player's one-second updates.
  local function seek_bar(t, spec)
    local W = spec.width
    local previous = clamp01(t.visual_position)
    local function tip(v) return math.max(0, math.min(W - 4, W * v - 2)) end
    -- A width of 0 is no width at all -- the clip would take its child's --
    -- so an empty run keeps a sliver.
    local function run(v) return math.max(1e-3, W * v) end
    local fill = ui.Item { id = "media-progress-fill", y = 11, height = 10, width = run(previous), clip = true,
      ui.Rect { width = W, height = 10, color = function() return C.primary:alpha(.2) end },
      stripes.box { width = W, height = 10, gap = 6, weight = 2, color = function() return C.primary end } }
    local grip = ui.Rect { id = "media-progress-handle", y = 7, width = 4, height = 18,
      x = tip(previous), color = function() return C.primary end }
    local running
    morf.effect("tsugumori.media.progress." .. tostring(fill), function()
      local v = clamp01(t.visual_position)
      local active = get(spec.active) ~= false
      local playing = spec.playing and spec.playing()
      -- Interpolate the player's one-second ticks; a drag and a seek settle
      -- at once.
      local flowing = playing and not t.dragging and v > previous and v - previous < .03
      previous = v
      if running then running:stop() running = nil end
      if not active or t.dragging then fill.width, grip.x = run(v), tip(v) return end
      local duration, easing = flowing and 1000 or 180, flowing and "linear" or "out_cubic"
      running = morf.animation.play { { parallel = {
        { node = fill, property = "width", to = run(v), duration = duration, easing = easing },
        { node = grip, property = "x", to = tip(v), duration = duration, easing = easing },
      } } }
    end, { owner = fill })
    return {
      track = ui.Item { width = W, height = 34 },
      background = ui.Item { width = W, height = 34,
        ui.Rect { y = 11, width = W, height = 10, color = function() return C.primary:alpha(.06) end,
          border_width = 1, border_color = function() return C.primary:alpha(.18) end },
        ruler(0, 24, W, 5, function() return C.primary:alpha(.3) end) },
      fill = fill,
      handle = grip,
    }
  end

  function S.Range(t, spec)
    if spec.widget == "seek_bar" then return seek_bar(t, spec) end
    return slider(t, spec)
  end

  return S
end
