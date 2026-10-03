-- The default kit's looks for each Navigation widget, in the Adwaita
-- manner. Every widget moves its pages its own way: a navigation view's
-- new page slides over the old one, which drifts back and dims; a view
-- stack cross-fades; tab pages and a carousel slide side by side; an
-- onboarding page rises in; a wizard's steps nudge along; a detail pane
-- settles in from a touch smaller. With `chrome = true` in its spec the
-- widget also draws its furniture over its pages (which leave it room): a
-- header with a back button and the page's title, a tab strip, carousel
-- dots, an onboarding progress, a wizard's step count, a detail card.
-- A composite that draws its own leaves `chrome` out. The page's name is
-- `spec.titles[name]`, or the name itself.
local morf = require("morf")
local ui = require("morf.ui")

return function(S, theme, M)
  local P = theme.P
  local R = theme.radius
  local function quick() return { duration = theme.duration.small, easing = theme.ease.standard } end
  local function slide() return M.spring(380, 40) end
  -- The page names in order (the glue hands the skin the list).
  local function names_of(spec)
    local out = {}
    for _, name in ipairs(type(spec.pages) == "table" and spec.pages or {}) do
      if type(name) == "string" then out[#out + 1] = name end
    end
    return out
  end
  local function index_of(t, spec)
    for i, name in ipairs(names_of(spec)) do if name == t.current then return i end end
    return 0
  end
  local function title_of(spec, name)
    local titles = spec.titles or {}
    local text = titles[name] or tostring(name or "")
    return (text:gsub("^%l", string.upper))
  end

  -- -------------------------------------------------------- transitions --

  --- A transition from `o`: `to` comes from (`in_x` widths along the
  --- direction, `in_y` px, `in_opacity`, `in_scale`), `from` leaves to the
  --- `out_*` values; `over`, the page in front slides the full width while
  --- the one behind drifts `under` widths and fades to `dim`.
  local function mover(t, spec, o)
    local latest
    local function width(node) return spec.width or (node and node.layout_width) or t.width or 400 end
    return function(from, to, direction)
      latest = to
      direction = direction or 1
      local W = width(to)
      local function reset(node) node.translate_x, node.translate_y, node.opacity, node.scale, node.z = 0, 0, 1, 1, 0 end
      if theme.reduced or not from then
        reset(to)
        if from and from ~= to then from.visible = false reset(from) end
        return
      end
      local steps = {}
      local function go(node, prop, a, b, duration, easing, delay)
        node[prop] = a
        steps[#steps + 1] = { node = node, property = prop, to = b, duration = duration, easing = easing or "out_cubic",
          delay = delay }
      end
      local d = o.duration or theme.duration.page
      if o.over then
        local front, back = to, from
        if direction < 0 then front, back = from, to end
        front.z, back.z = 1, 0
        if direction > 0 then
          go(to, "translate_x", W, 0, d)
          go(from, "translate_x", 0, -W * (o.under or 0.3), d)
          go(from, "opacity", 1, o.dim or 0.5, d)
          to.opacity = 1
        else
          go(from, "translate_x", 0, W, d)
          go(to, "translate_x", -W * (o.under or 0.3), 0, d)
          go(to, "opacity", o.dim or 0.5, 1, d)
        end
      else
        go(to, "translate_x", direction * W * (o.in_x or 0), 0, d, o.easing)
        go(to, "translate_y", o.in_y or 0, 0, d, o.easing)
        go(to, "opacity", o.in_opacity or 1, 1, o.fade_in or d, "linear", o.delay)
        go(to, "scale", o.in_scale or 1, 1, d, o.easing)
        steps[#steps + 1] = { node = from, property = "translate_x", to = -direction * W * (o.out_x or 0), duration = d,
          easing = o.easing or "out_cubic" }
        steps[#steps + 1] = { node = from, property = "translate_y", to = o.out_y or 0, duration = d, easing = "out_cubic" }
        steps[#steps + 1] = { node = from, property = "opacity", to = o.out_opacity or 1, duration = o.fade_out or d,
          easing = "linear" }
        steps[#steps + 1] = { node = from, property = "scale", to = o.out_scale or 1, duration = d, easing = "out_cubic" }
      end
      morf.animation.play { { parallel = steps }, on_finished = function()
        if from ~= latest then from.visible = false reset(from) end
        if to == latest then to.z = 0 end
      end }
    end
  end

  -- ------------------------------------------------------------ chrome --

  local function hairline(y)
    return ui.Rect { anchors = { left = true, right = true }, y = y, height = 1,
      color = function() local p = P() return p.strong and p.border or p.border:alpha(0.8) end }
  end

  --- A flat round back button that shows while there is somewhere to go
  --- back to, sliding in from the left.
  local function back_button(t, send, x, y)
    return ui.MouseArea { x = x, y = y, width = 34, height = 34, cursor = "pointer",
      accessible_role = "button", accessible_name = "Back",
      visible = function() return t.can_go_back end,
      opacity = function() return t.can_go_back and 1 or 0 end,
      on_clicked = function() send("pop") end,
      behavior = { opacity = quick() },
      ui.Rect { anchors = { fill = true }, radius = 17, color = function() return P().ink:alpha(P().wash.hover) end },
      M.icon("arrow_back", 20, function() return P().ink end, { anchors = { center_in = true } }) }
  end

  --- A header bar: the back button, the current page's title centred
  --- (cross-fading as it changes), a hairline under it.
  local function header(t, spec, send, opts)
    opts = opts or {}
    local H = opts.height or 44
    local text = M.text { text = function() return title_of(spec, t.current) end, font_weight = 700,
      font_size = opts.size or theme.size.normal, color = function() return P().ink end }
    local holder = ui.Item { y = 0, width = function() return t.width end, height = H,
      ui.Rect { anchors = { fill = true }, top_left_radius = R.large, top_right_radius = R.large,
        color = function() return P().header end },
      hairline(H - 1),
      ui.Item { y = (H - (opts.size or theme.size.normal) * 1.4) / 2, height = H,
        x = function()
          if opts.left then return t.can_go_back and 50 or 14 end
          return ((t.width or 0) - (text.layout_width or 0)) / 2
        end,
        behavior = { x = slide() }, text },
    }
    ui.reparent(back_button(t, send, 6, (H - 34) / 2), holder)
    return holder
  end

  local function ground()
    return ui.Rect { anchors = { fill = true }, radius = R.large, color = function() return P().view end,
      border_width = function() local p = P() return (p.strong and 2) or (p.dark and 0) or 1 end,
      border_color = function() return P().border end }
  end

  -- ------------------------------------------------------------ widgets --

  function S.navigation_view(t, spec, _, send)
    return {
      transition = mover(t, spec, { over = true, under = 0.3, dim = 0.4 }),
      background = spec.chrome and ground() or nil,
      page = spec.chrome and header(t, spec, send) or nil,
    }
  end

  function S.settings_subpages(t, spec, _, send)
    return {
      transition = mover(t, spec, { over = true, under = 0.15, dim = 0.2, duration = 280 }),
      background = spec.chrome and ground() or nil,
      page = spec.chrome and header(t, spec, send, { left = true, size = theme.size.larger, height = 48 }) or nil,
    }
  end

  function S.view_stack(t, spec)
    return {
      transition = mover(t, spec, { in_opacity = 0, out_opacity = 0, fade_in = 200, fade_out = 140, duration = 200 }),
      background = spec.chrome and ground() or nil,
    }
  end

  --- Tab pages: a strip of the pages' names over them, an accent bar
  --- sliding to the chosen one; the pages slide side by side.
  function S.tab_pages(t, spec, _, send)
    local slots = { transition = mover(t, spec, { in_x = 1, out_x = 1, duration = 300 }) }
    if not spec.chrome then return slots end
    local names = names_of(spec)
    local function slot() return (t.width or 0) / math.max(1, #names) end
    local strip = ui.Item { width = function() return t.width end, height = 44,
      ui.Rect { anchors = { fill = true }, top_left_radius = R.large, top_right_radius = R.large,
        color = function() return P().header end }, hairline(43) }
    for i, name in ipairs(names) do
      ui.reparent(ui.MouseArea { x = function() return (i - 1) * slot() end, width = slot, height = 42, cursor = "pointer",
        accessible_role = "tab", accessible_name = title_of(spec, name),
        on_clicked = function() send("go", name) end,
        M.text { anchors = { center_in = true }, text = title_of(spec, name),
          font_weight = function() return t.current == name and 700 or 400 end,
          color = function() local p = P() return t.current == name and p.ink or p.ink_dim end } }, strip)
    end
    ui.reparent(ui.Rect { y = 40, height = 3, radius = 1.5, color = function() return P().accent end,
      width = function() return math.min(72, slot() - 24) end,
      x = function() return (math.max(1, index_of(t, spec)) - 0.5) * slot() - math.min(72, slot() - 24) / 2 end,
      behavior = { x = slide() } }, strip)
    slots.background = ground()
    slots.indicator = strip
    return slots
  end

  --- A carousel: slides side by side, the old one shrinking back as it
  --- goes; dots under them, an accent pill running to the current one,
  --- and round arrow buttons at the sides.
  function S.carousel(t, spec, _, send)
    local slots = { transition = mover(t, spec, { in_x = 1, out_x = 0.5, out_scale = 0.9, out_opacity = 0, fade_out = 260,
      duration = 360 }) }
    if not spec.chrome then return slots end
    local names = names_of(spec)
    local DOT, GAP = 8, 10
    local function row_x() return ((t.width or 0) - (#names * DOT + (#names - 1) * GAP)) / 2 end
    local dots = ui.Item { anchors = { left = true, right = true, bottom = true }, height = 32 }
    for i, name in ipairs(names) do
      ui.reparent(ui.MouseArea { x = function() return row_x() + (i - 1) * (DOT + GAP) - 4 end, y = 8, width = DOT + 8,
        height = 16, cursor = "pointer", accessible_name = title_of(spec, name), on_clicked = function() send("go", name) end,
        ui.Rect { x = 4, y = 4, width = DOT, height = DOT, radius = DOT / 2,
          color = function() local p = P() return p.ink:alpha(p.strong and 0.6 or 0.25) end } }, dots)
    end
    ui.reparent(ui.Rect { y = 12, height = DOT, radius = DOT / 2, width = 20,
      x = function() return row_x() + (math.max(1, index_of(t, spec)) - 1) * (DOT + GAP) - 6 end,
      color = function() return P().accent end, behavior = { x = slide() }, stretch = M.STRETCH }, dots)
    local function arrow(icon, x, event, show)
      return ui.MouseArea { x = x, y = function() return (t.height or 0) - 30 end, width = 28, height = 28,
        cursor = "pointer", accessible_role = "button", accessible_name = event,
        opacity = function() return show() and 1 or 0 end, visible = show, behavior = { opacity = quick() },
        on_clicked = function() send(event) end,
        ui.Rect { anchors = { fill = true }, radius = 14, color = function() return P().ink:alpha(P().wash.hover) end },
        M.icon(icon, 18, function() return P().ink end, { anchors = { center_in = true } }) }
    end
    slots.background = ground()
    slots.indicator = dots
    local right = arrow("chevron_right", 0, "next", function() return index_of(t, spec) < #names end)
    right.x = function() return (t.width or 0) - 36 end
    slots.back = ui.Item { anchors = { fill = true },
      arrow("chevron_left", 8, "previous", function() return index_of(t, spec) > 1 end), right }
    return slots
  end

  --- Onboarding: each page rises in as the last fades; a row of short
  --- bars at the top fills with the accent up to the current page.
  function S.onboarding(t, spec)
    local slots = { transition = mover(t, spec, { in_y = 28, in_opacity = 0, in_scale = 0.97, out_y = -12, out_opacity = 0,
      fade_in = 260, fade_out = 140, duration = 380, easing = theme.ease.decelerate }) }
    if not spec.chrome then return slots end
    local names = names_of(spec)
    local GAP = 6
    local function seg() return ((t.width or 0) - 32 - GAP * (#names - 1)) / math.max(1, #names) end
    local bars = ui.Item { x = 16, y = 10, width = function() return (t.width or 0) - 32 end, height = 4 }
    for i in ipairs(names) do
      ui.reparent(ui.Item { x = function() return (i - 1) * (seg() + GAP) end, width = seg, height = 4,
        ui.Rect { anchors = { fill = true }, radius = 2, color = function() return P().track end },
        ui.Rect { height = 4, radius = 2, color = function() return P().accent end,
          width = function() return i <= index_of(t, spec) and seg() or 0 end, behavior = { width = slide() } } }, bars)
    end
    slots.background = ground()
    slots.indicator = bars
    return slots
  end

  --- A wizard: steps nudge along as they fade; a header with the step's
  --- count and name, and a thin accent progress under it.
  function S.wizard(t, spec)
    local slots = { transition = mover(t, spec, { in_x = 0.08, out_x = 0.08, in_opacity = 0, out_opacity = 0, fade_in = 220,
      fade_out = 140, duration = 280 }) }
    if not spec.chrome then return slots end
    local names = names_of(spec)
    local H = 44
    slots.background = ground()
    slots.page = ui.Item { width = function() return t.width end, height = H,
      ui.Rect { anchors = { fill = true }, top_left_radius = R.large, top_right_radius = R.large,
        color = function() return P().header end },
      ui.Rect { anchors = { left = true, right = true }, y = H - 2, height = 2, color = function() return P().track end },
      ui.Rect { y = H - 2, height = 2, color = function() return P().accent end,
        width = function() return (t.width or 0) * index_of(t, spec) / math.max(1, #names) end, behavior = { width = slide() } },
      M.text { x = 16, anchors = { vertical_center = true }, font_weight = 700,
        text = function() return title_of(spec, t.current) end, color = function() return P().ink end },
      M.text { anchors = { right = true, right_margin = 16, vertical_center = true }, font_size = theme.size.small,
        text = function() return ("Step %d of %d"):format(index_of(t, spec), #names) end,
        color = function() return P().ink_dim end } }
    return slots
  end

  --- Master and detail: the detail settles in from a touch smaller as the
  --- old fades, on a card.
  function S.master_detail(t, spec)
    return {
      transition = mover(t, spec, { in_scale = 0.97, in_opacity = 0, out_opacity = 0, fade_in = 220, fade_out = 120,
        duration = 260 }),
      background = spec.chrome and ground() or nil,
    }
  end
end
