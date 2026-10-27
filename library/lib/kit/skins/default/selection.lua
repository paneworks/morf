-- The default kit's Selection skin, in the Adwaita manner: a toggle
-- group's raised plate on its trough, view switchers with a sliding plate,
-- carousel dots and lines, a sidebar's washed rows, accent discs on a
-- calendar, swatch rings. Every one keeps the archetype's layout (an
-- entry per item, laid out by the glue) and moves one thing between
-- entries: a plate, a disc, a dot or a bar, drawn as a distance-field
-- layer riding an invisible track whose two edges travel one after the
-- other -- the front edge leaves first, the back follows -- so it
-- stretches out towards the new entry and draws itself in there. A
-- configuration's own `delegate` keeps its entries; only the ground is
-- drawn round them then.
local morf = require("morf")
local ui = require("morf.ui")

return function(S, theme, draw)
  local P = theme.P
  local R = theme.radius
  local get, quick, wash = draw.get, draw.quick, draw.wash
  local function label_of(v)
    if type(v) == "table" then return tostring(v.label or v.name or v.text or v.caption or v.title or "") end
    return tostring(v)
  end
  local function icon_of(v) return type(v) == "table" and v.icon or nil end
  local function nothing() return ui.Item {} end
  local function focus_color() local p = P() return p.strong and p.focus or p.focus:alpha(0.6) end
  local seq = 0
  local function key(name) seq = seq + 1 return "kit.default.selection." .. name .. "." .. seq end

  -- --------------------------------------------------------- travel --

  local function ease_out(u, power) return 1 - (1 - u) ^ (power or 3) end
  local N = 18
  --- Moves `node` from box `a` to box `b` ({x, y, w, h}) with its two
  --- edges on each axis apart: the leading one in the first 55 % of the
  --- time, the trailing one from a fifth of the way in.
  local function travel(node, a, b, duration)
    local tracks = {}
    for _, axis in ipairs { { "x", "width", 1, 3 }, { "y", "height", 2, 4 } } do
      local l0, r0 = a[axis[3]], a[axis[3]] + a[axis[4]]
      local l1, r1 = b[axis[3]], b[axis[3]] + b[axis[4]]
      if l0 == l1 and r0 == r1 then
        node[axis[1]], node[axis[2]] = l1, r1 - l1
      else
        local forward = l1 >= l0
        local pos, len = {}, {}
        for i = 0, N do
          local u = i / N
          local function edge(from, to, leading)
            local k = leading and ease_out(math.min(1, u / 0.55), 3) or ease_out(math.max(0, (u - 0.2) / 0.8), 2)
            return from + (to - from) * k
          end
          local l = edge(l0, l1, not forward)
          local r = edge(r0, r1, forward)
          pos[#pos + 1] = { at = u, value = l }
          len[#len + 1] = { at = u, value = math.max(0, r - l) }
        end
        tracks[#tracks + 1] = { node = node, property = axis[1], duration = duration, keyframes = pos }
        tracks[#tracks + 1] = { node = node, property = axis[2], duration = duration, keyframes = len }
      end
    end
    if #tracks == 0 then return nil end
    return morf.animation.play { { parallel = tracks } }
  end

  --- An invisible track that follows the current entry's box, given
  --- through `fit(x, y, w, h)` (the box a look wants in it: a disc, an
  --- inset plate). `on_move(duration)` hears each journey.
  local function glide(t, fit, on_move)
    local track = ui.Item { visible = function() return t.current > 0 end }
    local last, running
    morf.effect(key("glide"), function()
      local x, y, w, h = t.current_x, t.current_y, t.current_width, t.current_height
      if t.current < 1 or w <= 0 or h <= 0 then return end
      local box = { fit(x, y, w, h) }
      if not last or theme.reduced then
        track.x, track.y, track.width, track.height = box[1], box[2], box[3], box[4]
      elseif box[1] ~= last[1] or box[2] ~= last[2] or box[3] ~= last[3] or box[4] ~= last[4] then
        if running then running:stop() end
        local far = math.abs(box[1] - last[1]) + math.abs(box[2] - last[2])
        local duration = math.floor(math.min(460, 260 + far * 0.6))
        running = travel(track, { track.x or last[1], track.y or last[2], track.width or last[3],
          track.height or last[4] }, box, duration)
        if on_move then on_move(duration) end
      end
      last = box
    end, { owner = track })
    return track
  end

  local function inset(d) return function(x, y, w, h) return x + d, y + d, math.max(0, w - 2 * d), math.max(0, h - 2 * d) end end
  --- A disc of `size` centred in the entry (at `dx` from its left, when given).
  local function disc(size, dx)
    return function(x, y, w, h)
      local cx = dx and (x + dx) or (x + w / 2)
      return cx - size / 2, y + h / 2 - size / 2, size, size
    end
  end

  --- The moving layer: a field box of `radius` (a number or a function of
  --- the track's height) in `color` riding `track`, with `extra` nodes
  --- (an outline, a check) inside the track.
  local function plate(track, radius, color, extra)
    for _, node in ipairs(extra or {}) do ui.reparent(node, track) end
    return ui.Item { anchors = { fill = true },
      ui.Sdf { anchors = { fill = true },
        ui.SdfShape { shape = "box", track = track, fill_color = color,
          radius = type(radius) == "function" and function() return radius(track.height or 0) end or radius } },
      track }
  end

  --- The pointer's wash and the keyboard's ring on one entry.
  local function hover(t, s, radius, props)
    props = props or {}
    props.anchors = props.anchors or { fill = true, margins = props.margins or 0 }
    props.margins = nil
    props.radius = radius
    props.color = function()
      local p = P()
      if s.down() then return p.window_fg:alpha(p.wash.active) end
      return s.hovered() and p.window_fg:alpha(p.wash.hover) or p.window_fg:alpha(0)
    end
    props.border_width = function() return t.visual_focus and s.current() and 2 or 0 end
    props.border_color = focus_color
    props.behavior = { color = quick() }
    return ui.Rect(props)
  end

  local function trough(radius)
    return ui.Rect { anchors = { fill = true }, radius = radius,
      color = function() local p = P() return p.window_fg:alpha(p.dark and 0.08 or 0.06) end,
      border_width = function() return P().strong and 1 or 0 end, border_color = function() return P().border end }
  end

  local function caption(text, s, props)
    props = props or {}
    props.text = text
    props.font_size = props.font_size or theme.size.small
    props.font_weight = props.font_weight or 700
    props.color = props.color or function() local p = P() return s.current() and p.window_fg or p.dim_fg end
    props.behavior = { color = quick() }
    return draw.text(props)
  end

  --- A raised plate: the view's tone with a hairline shade (light), a
  --- lighter wash (dark), the ink's outline (high contrast).
  local function raised() local p = P() return p.dark and p.window_fg:alpha(0.16) or p.view_bg end
  local function raised_edge()
    return ui.Rect { anchors = { fill = true }, color = "transparent", radius = R.small,
      border_width = 1, border_color = function() local p = P() return p.strong and p.window_fg or p.shade:alpha(p.dark and 0 or 0.7) end }
  end

  local function delegated(spec, slots)
    if spec.delegate then slots.indicator = nothing() end
    return slots
  end

  -- --------------------------------------------------- linked choices --
  --- AdwToggleGroup (and GtkStackSwitcher): a trough, and a raised plate
  --- that runs to the chosen toggle and stretches on the way.
  function S.toggle_group(t, spec)
    local track = glide(t, inset(3))
    return delegated(spec, {
      background = trough(R.medium + 2),
      indicator = plate(track, R.small, raised, { raised_edge() }),
      item = function(_, value, s)
        local glyph = icon_of(value)
        local label = label_of(value)
        local look = ui.Item { anchors = { fill = true }, hover(t, s, R.small, { margins = 3 }) }
        if glyph and label == "" then
          ui.reparent(draw.icon(glyph, 16, function() local p = P() return s.current() and p.window_fg or p.dim_fg end,
            { anchors = { center_in = true } }), look)
        else
          ui.reparent(caption(label, s, { anchors = { fill = true, left_margin = 6, right_margin = 6 },
            horizontal_alignment = "center", vertical_alignment = "center", elide = "right" }), look)
        end
        return look
      end,
    })
  end

  --- AdwViewSwitcher: no trough; each view an icon over its name, the
  --- chosen one under the selected wash, which slides between them.
  function S.view_switcher(t, spec)
    local track = glide(t, inset(4))
    return delegated(spec, {
      background = nothing(),
      indicator = plate(track, R.medium, wash("selected")),
      item = function(_, value, s)
        local function ink() local p = P() return s.current() and p.window_fg or p.dim_fg end
        return ui.Item { anchors = { fill = true },
          hover(t, s, R.medium, { margins = 4 }),
          ui.Column { anchors = { center_in = true }, gap = 2, align = "center",
            draw.icon(icon_of(value) or "circle", 20, ink, { fill = s.current }),
            caption(label_of(value), s, { color = ink }) } }
      end,
    })
  end

  --- AdwInlineViewSwitcher: a round trough, the icon beside the name,
  --- and a pill plate.
  function S.inline_view_switcher(t, spec)
    local track = glide(t, inset(3))
    local function pill(h) return h / 2 end
    return delegated(spec, {
      background = ui.Item { anchors = { fill = true },
        ui.Rect { anchors = { fill = true }, radius = function() return (t.height or 0) / 2 end,
          color = function() local p = P() return p.window_fg:alpha(p.dark and 0.08 or 0.06) end,
          border_width = function() return P().strong and 1 or 0 end, border_color = function() return P().border end } },
      indicator = plate(track, pill, raised, {
        ui.Rect { anchors = { fill = true }, color = "transparent", radius = function() return (track.height or 0) / 2 end,
          border_width = 1, border_color = function() local p = P() return p.strong and p.window_fg or p.shade:alpha(p.dark and 0 or 0.7) end } }),
      item = function(_, value, s)
        local glyph = icon_of(value)
        local function ink() local p = P() return s.current() and p.window_fg or p.dim_fg end
        return ui.Item { anchors = { fill = true },
          hover(t, s, nil, { margins = 3 }),
          ui.Row { anchors = { center_in = true }, gap = 6, align = "center",
            glyph and draw.icon(glyph, 16, ink) or nil,
            caption(label_of(value), s, { color = ink }) } }
      end,
    })
  end

  --- A sidebar: an icon and a name per row, the chosen row's icon filled,
  --- its name bold, under a plate of the selected wash.
  function S.sidebar_list(t, spec)
    local track = glide(t, inset(0))
    return delegated(spec, {
      background = nothing(),
      indicator = plate(track, R.small, wash("selected")),
      item = function(_, value, s)
        local glyph = icon_of(value)
        return ui.Item { anchors = { fill = true },
          hover(t, s, R.small),
          glyph and draw.icon(glyph, 18, function() return P().window_fg end,
            { x = 12, anchors = { vertical_center = true }, fill = s.current }) or nil,
          draw.text { anchors = { fill = true, left_margin = glyph and 42 or 12, right_margin = 8 }, text = label_of(value),
            font_size = theme.size.normal, elide = "right", vertical_alignment = "center",
            font_weight = function() return s.current() and 700 or 400 end, color = function() return P().window_fg end } }
      end,
    })
  end

  --- A month: the day numbers, today's choice an accent disc that runs
  --- across the weeks; the blanks before the first draw nothing.
  function S.day_grid(t, spec)
    local function size() return math.max(0, math.min(t.current_width, t.current_height) - 4) end
    local track = glide(t, function(x, y, w, h) local d = math.max(0, math.min(w, h) - 4) return x + (w - d) / 2, y + (h - d) / 2, d, d end)
    return delegated(spec, {
      background = nothing(),
      indicator = plate(track, function(h) return h / 2 end, function() return P().accent_bg end),
      item = function(index, value, s)
        if label_of(value) == "" or (type(value) == "table" and value.blank) then return ui.Item {} end
        return ui.Item { anchors = { fill = true },
          ui.Rect { anchors = { center_in = true }, width = size, height = size, radius = 999,
            color = function() local p = P() return (s.hovered() and not s.current()) and p.window_fg:alpha(p.wash.hover) or p.window_fg:alpha(0) end,
            border_width = function() return t.visual_focus and s.current() and 2 or 0 end,
            border_color = function() local p = P() return p.strong and p.window_fg or p.focus end },
          draw.text { anchors = { center_in = true }, text = label_of(value), font_size = theme.size.small,
            font_weight = function() return s.current() and 700 or 400 end,
            color = function() local p = P() return s.current() and p.accent_fg or p.window_fg end,
            behavior = { color = quick() } } }
      end,
    })
  end

  --- Colour swatches: rounded squares of their colours; a ring in the
  --- ink runs round the chosen one, which shows a check.
  function S.swatch_grid(t, spec)
    local track = glide(t, inset(-1))
    return delegated(spec, {
      background = nothing(),
      indicator = plate(track, R.medium + 1, function() return P().window_fg:alpha(0) end, {
        ui.Rect { anchors = { fill = true }, color = "transparent", radius = R.medium + 1, border_width = 2,
          border_color = function() return P().window_fg end } }),
      item = function(_, value, s)
        local color = type(value) == "table" and value.color or nil
        return ui.Item { anchors = { fill = true },
          ui.Rect { anchors = { fill = true, margins = 4 }, radius = R.small,
            color = function() return get(color) or P().accent_bg end,
            scale = function() return s.hovered() and 1.06 or 1 end, behavior = { scale = draw.spring(420, 30) } },
          draw.icon("check", 18, function() return P().accent_fg end, { anchors = { center_in = true },
            opacity = function() return s.current() and 1 or 0 end, behavior = { opacity = quick() } }) }
      end,
    })
  end

  --- Emoji: each glyph large, a wash under the pointer, the chosen one on
  --- a rounded plate that slides.
  function S.emoji_grid(t, spec)
    local track = glide(t, inset(2))
    return delegated(spec, {
      background = nothing(),
      indicator = plate(track, R.medium, wash("checked")),
      item = function(_, value, s)
        return ui.Item { anchors = { fill = true },
          hover(t, s, R.medium, { margins = 2 }),
          draw.text { anchors = { center_in = true }, text = type(value) == "table" and (value.emoji or value.label) or tostring(value),
            font_size = math.floor((spec.item_height or 40) * 0.55), color = function() return P().window_fg end } }
      end,
    })
  end

  --- AdwCarouselIndicatorDots: small dots, and one accent pill that runs to the
  --- current slide -- the dots it passes melt into it while it moves.
  function S.carousel_indicator_dots(t, spec)
    local field
    local track = glide(t, function(x, y, w, h) return x + w / 2 - 11, y + h / 2 - 4, 22, 8 end, function(duration)
      if not field then return end
      field.blend = 6
      morf.timer(duration, function() if field then field.blend = 0 end end, false)
    end)
    field = ui.Sdf { anchors = { fill = true }, blend = 0, behavior = { blend = { duration = 160 } },
      ui.SdfShape { shape = "box", radius = 4, track = track, operation = "smooth_union",
        fill_color = function() return P().accent_bg end } }
    local dots = {}
    return delegated(spec, {
      background = nothing(),
      indicator = ui.Item { anchors = { fill = true }, field, track },
      item = function(index, _, s)
        if index == 1 then
          for _, shape in ipairs(dots) do ui.destroy(shape, true) end
          dots = {}
        end
        local dot = ui.Item { anchors = { center_in = true }, width = 8, height = 8 }
        if field then
          dots[#dots + 1] = ui.SdfShape { shape = "circle", track = dot, operation = "smooth_union",
            fill_color = function() local p = P() return p.window_fg:alpha(s.hovered() and 0.5 or (p.strong and 0.6 or 0.25)) end }
          ui.reparent(dots[#dots], field)
        end
        return ui.Item { anchors = { fill = true }, dot }
      end,
    })
  end

  S.stack_switcher = S.toggle_group

  --- AdwViewSwitcherBar: the view switcher along the bottom of a narrow
  --- window, on the header bar's ground with a hairline over it.
  function S.view_switcher_bar(t, spec)
    local slots = S.view_switcher(t, spec)
    ui.destroy(slots.background, true)
    slots.background = ui.Item { anchors = { fill = true },
      ui.Rect { anchors = { fill = true }, color = function() return P().headerbar_bg end },
      ui.Rect { anchors = { left = true, right = true, top = true }, height = 1,
        color = function() return P().headerbar_shade end } }
    return slots
  end

  --- AdwCarouselIndicatorLines: a short bar per page, the current one in
  --- the accent, the bar running to it.
  function S.carousel_indicator_lines(t, spec)
    local track = glide(t, function(x, y, w, h) return x + 2, y + h / 2 - 2, math.max(0, w - 4), 4 end)
    return delegated(spec, {
      background = nothing(),
      indicator = plate(track, 2, function() return P().accent_bg end),
      item = function(_, _, s)
        return ui.Item { anchors = { fill = true },
          ui.Rect { anchors = { left = true, right = true, vertical_center = true, left_margin = 2, right_margin = 2 },
            height = 4, radius = 2,
            color = function() local p = P() return p.window_fg:alpha(s.hovered() and 0.5 or (p.strong and 0.6 or 0.25)) end } }
      end,
    })
  end

  --- Any other selection: each entry its label centred, the chosen one an
  --- accent disc with white ink.
  function S.Selection(t, spec)
    return {
      item = function(_, value, s)
        return ui.Item { anchors = { fill = true },
          ui.Rect { anchors = { fill = true, margins = 2 }, radius = R.small,
            color = function()
              local p = P()
              if s.current() then return p.accent_bg end
              return s.hovered() and p.window_fg:alpha(p.wash.hover) or p.window_fg:alpha(0)
            end,
            border_width = function() return t.visual_focus and s.current() and 2 or 0 end,
            border_color = focus_color, behavior = { color = quick() } },
          draw.text { anchors = { center_in = true }, text = label_of(value), font_size = theme.size.small,
            color = function() local p = P() return s.current() and p.accent_fg or p.window_fg end } }
      end,
    }
  end
end
