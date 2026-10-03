-- The default kit's looks for the Drag archetype's widgets, in the
-- Adwaita manner: hairline dividers whose grip shows under the pointer,
-- boxed-list rows and flat tabs that lift off the page on a soft shadow
-- while they are carried, cards that tip away as they are swiped, a row's
-- actions in the accent and the destructive red under its trailing edge, a
-- raised disc with an accent arc for pulling to refresh, the sheet's grey
-- pill, and a drop target that fills with the accent while it would take
-- what is over it. Behaviour -- following, reordering, latching, snapping
-- -- is lib.kit.drag's and the archetype's.
local ui = require("morf.ui")

return function(S, theme, M)
  local P = theme.P
  local R = theme.radius
  local function get(v) if type(v) == "function" then return v() end return v end
  local function quick() return { duration = theme.duration.small, easing = theme.ease.standard } end
  local function lift_spring() return M.spring(520, 32) end
  local function W(t) return (t.width and t.width > 0) and t.width or 0 end
  local function H(t) return (t.height and t.height > 0) and t.height or 0 end
  local function clamp01(v) return v < 0 and 0 or (v > 1 and 1 or v) end

  local function ring(t, radius)
    return function()
      return ui.Rect { anchors = { fill = true }, z = 50, color = "transparent", radius = radius,
        border_width = 2, border_color = function() local p = P() return p.strong and p.focus or p.focus:alpha(0.6) end,
        visible = function() return t.visual_focus end }
    end
  end

  -- Lifts a slot's node while the control is carried (each slot fills the
  -- control, so all of them grow about the same centre).
  local function lift(t, scale, props)
    props.scale = function() return t.active and (scale or 1.03) or 1 end
    props.behavior = { scale = lift_spring() }
    if props.anchors == nil then props.anchors = { fill = true } end
    return ui.Item(props)
  end

  -- A card's ground that lifts while it is carried: a shadow that fades in
  -- under it (a static blur, only its opacity moves), a touch larger.
  local function lifted(t, radius, scale)
    return lift(t, scale, {
      ui.Rect { anchors = { fill = true, margins = 2 }, radius = radius, color = function() return P().shade end,
        shadow_color = function() return P().dark and "#000000aa" or "#00000059" end,
        shadow_blur = 22, shadow_offset_y = 8,
        opacity = function() return t.active and 1 or 0 end, behavior = { opacity = quick() } },
      ui.Rect { anchors = { fill = true }, radius = radius,
        color = function()
          local p = P()
          if t.active then return p.raised end
          if t.down then return p.card:mix(p.ink, p.wash.active) end
          if t.hovered then return p.card:mix(p.ink, p.wash.hover) end
          return p.card
        end,
        border_width = function() local p = P() return (p.strong and 2) or (p.dark and 0) or 1 end,
        border_color = function() local p = P() return t.active and p.accent or p.border end,
        behavior = { color = quick() } } })
  end

  -- The six-dot grip a carried row is held by.
  local function grip_icon(t, props)
    return M.icon("drag_indicator", 20, function()
      local p = P()
      return t.active and p.accent_ink or (t.hovered and p.ink or p.ink_dim)
    end, props)
  end

  -- A pane's divider: a hairline, the grip a short pill that shows under
  -- the pointer and takes the accent while held.
  local function divider(t, spec, extra)
    local across = (spec.axis or "x") == "x"
    local function shown() return t.active or t.hovered or t.visual_focus end
    local slots = {
      background = ring(t, 3),
      handle = ui.Item { anchors = { fill = true },
        ui.Rect {
          x = function() return across and math.floor(W(t) / 2) or 0 end,
          y = function() return across and 0 or math.floor(H(t) / 2) end,
          width = function() return across and 1 or W(t) end,
          height = function() return across and H(t) or 1 end,
          color = function() local p = P() return t.active and p.accent or p.border end,
          behavior = { color = quick() } },
        ui.Rect {
          x = function() return (W(t) - (across and 4 or 32)) / 2 end,
          y = function() return (H(t) - (across and 32 or 4)) / 2 end,
          width = across and 4 or 32, height = across and 32 or 4, radius = 2,
          scale = function() return t.active and 1.25 or 1 end,
          opacity = function() return shown() and 1 or 0 end,
          color = function() local p = P() return t.active and p.accent or p.ink:alpha(0.45) end,
          behavior = { opacity = quick(), color = quick(), scale = M.spring(420, 30) } } },
    }
    for k, v in pairs(extra or {}) do slots[k] = v end
    return slots
  end

  S.split_pane = function(t, spec) return divider(t, spec) end

  --- A panel's edge: the divider, and while it is dragged the width it
  --- would have in a small raised label beside it.
  S.resizable_panel = function(t, spec)
    return divider(t, spec, {
      ghost = ui.Rect { x = -68, y = function() return H(t) / 2 - 14 end, width = 60, height = 28, radius = 14,
        color = function() return P().raised end, border_width = 1, border_color = function() return P().border end,
        opacity = function() return t.active and 1 or 0 end, behavior = { opacity = quick() },
        M.text { anchors = { fill = true }, horizontal_alignment = "center", vertical_alignment = "center",
          font_size = theme.size.small, font_weight = 700, color = function() return P().ink end,
          text = function() return ("%d px"):format(math.floor((t.value or 0) + 0.5)) end } },
    })
  end

  --- A corner grip: three diagonal hairlines on a round wash.
  S.resize_grip = function(t)
    return {
      background = ui.Rect { anchors = { fill = true }, radius = R.small,
        color = function() local p = P() return p.ink:alpha(t.active and p.wash.active or (t.hovered and p.wash.hover or 0)) end,
        behavior = { color = quick() } },
      handle = ui.Path { anchors = { fill = true, margins = 4 }, view_box = { 0, 0, 16, 16 },
        d = "M15 4 L4 15 M15 9 L9 15 M15 13.5 L13.5 15", fill_color = "transparent", stroke_width = 1.5,
        stroke_cap = "round",
        stroke_color = function() local p = P() return t.active and p.accent or p.ink_dim end },
      content = ring(t, R.small),
    }
  end

  --- A row of a reorderable list: a boxed-list row with its grip at the
  --- start, an icon and the label; carried, it lifts on a shadow.
  S.reorderable_rows = function(t, spec)
    return {
      background = lifted(t, R.large),
      handle = lift(t, 1.03, {
        grip_icon(t, { x = 12, anchors = { vertical_center = true } }) }),
      content = lift(t, 1.03, {
        spec.icon and M.icon(spec.icon, 20, function() return P().ink end,
          { x = 44, anchors = { vertical_center = true } }) or nil,
        M.text { x = spec.icon and 76 or 44, anchors = { vertical_center = true }, text = spec.label or "",
          color = function() return P().ink end } }),
      drop_indicator = ring(t, R.large),
    }
  end

  --- A tab of a reorderable strip: flat, the current one on the checked
  --- wash; carried, it lifts as a card.
  S.reorderable_tabs = function(t, spec)
    return {
      handle = ui.Item {},
      background = lift(t, 1.04, {
        ui.Rect { anchors = { fill = true }, radius = R.medium, color = function() return P().shade end,
          shadow_color = function() return P().dark and "#00000099" or "#00000038" end, shadow_blur = 14, shadow_offset_y = 4,
          opacity = function() return t.active and 1 or 0 end, behavior = { opacity = quick() } },
        ui.Rect { anchors = { fill = true }, radius = R.medium,
          color = function()
            local p = P()
            if t.active then return p.raised end
            if t.highlighted then return p.ink:alpha(p.wash.checked) end
            if t.down then return p.ink:alpha(p.wash.active) end
            return p.ink:alpha(t.hovered and p.wash.hover or 0)
          end,
          border_width = function() return (P().strong or t.active) and 1 or 0 end,
          border_color = function() return P().border end,
          behavior = { color = quick() } } }),
      content = lift(t, 1.04, { M.text { anchors = { fill = true }, horizontal_alignment = "center",
        vertical_alignment = "center", text = spec.label or "",
        font_weight = function() return t.highlighted and 700 or 400 end,
        color = function() return P().ink end } }),
      drop_indicator = ring(t, R.medium),
    }
  end

  --- A tile of a sortable grid: a card with its icon over its label.
  S.sortable_grid = function(t, spec)
    return {
      handle = ui.Item {},
      background = lifted(t, R.large, 1.06),
      content = lift(t, 1.06, {
        M.icon(spec.icon or "apps", 30, function() local p = P() return t.active and p.accent_ink or p.ink end,
          { anchors = { horizontal_center = true }, y = function() return H(t) / 2 - 30 end }),
        M.text { anchors = { left = true, right = true }, y = function() return H(t) / 2 + 8 end,
          horizontal_alignment = "center", elide = "right", text = spec.label or "",
          font_size = theme.size.small, color = function() return P().ink end } }),
      drop_indicator = ring(t, R.large),
    }
  end

  --- A card swiped away: it lifts as it is held and fades the further it
  --- goes.
  S.swipe_dismiss = function(t, spec)
    local function fade() return 1 - 0.5 * clamp01(math.abs(t.delta_x or 0) / math.max(1, W(t))) end
    return {
      handle = ui.Item {},
      background = lifted(t, R.large, 1.01),
      content = ui.Item { anchors = { fill = true }, opacity = fade,
        M.icon("notifications", 22, function() return P().accent_ink end, { x = 16, anchors = { vertical_center = true } }),
        M.text { x = 52, y = function() return H(t) / 2 - 20 end, text = spec.label or "", font_weight = 700,
          color = function() return P().ink end },
        M.text { x = 52, y = function() return H(t) / 2 + 2 end, text = spec.detail or "",
          font_size = theme.size.small, color = function() return P().ink_dim end } },
      drop_indicator = ring(t, R.large),
    }
  end

  --- A row whose actions wait under its trailing edge: the row a view-
  --- coloured strip, the actions solid plates in their tones whose icons
  --- grow in as they are uncovered.
  S.swipe_actions = function(t, spec)
    local reveal = spec.reveal or 128
    local actions = spec.actions or {}
    local function uncovered()
      local base = t.open and reveal or 0
      if not t.active then return base / reveal end
      return clamp01((base - (t.delta_x or 0)) / reveal)
    end
    -- (Past the trailing edge: the left one right to left.)
    local plates = ui.Item { x = function() return t.mirrored and -reveal or W(t) end, width = reveal,
      height = function() return H(t) end }
    local each = reveal / math.max(1, #actions)
    for i, action in ipairs(actions) do
      local function tone()
        local p = P()
        if action.tone == "destructive" then return p.destructive end
        if action.tone == "warning" then return p.warning end
        return p.accent
      end
      ui.reparent(ui.Rect { x = (i - 1) * each, width = each, height = function() return H(t) end, color = tone,
        M.icon(action.icon or "check", 22, function() return P().on_accent end,
          { anchors = { horizontal_center = true }, y = function() return H(t) / 2 - 20 end,
            scale = function() return 0.6 + 0.4 * uncovered() end, opacity = uncovered }),
        M.text { anchors = { left = true, right = true }, y = function() return H(t) / 2 + 4 end,
          horizontal_alignment = "center", text = action.label or "", font_size = theme.size.small,
          color = function() return P().on_accent end, opacity = uncovered } }, plates)
    end
    return {
      handle = ui.Item {},
      background = ui.Rect { anchors = { fill = true }, color = function()
          local p = P()
          return t.down and p.view:mix(p.ink, p.wash.active) or (t.hovered and p.view:mix(p.ink, p.wash.hover) or p.view)
        end,
        behavior = { color = quick() },
        ui.Rect { anchors = { left = true, right = true, bottom = true }, height = 1, color = function() return P().border end } },
      content = ui.Item { anchors = { fill = true },
        M.text { x = 16, anchors = { vertical_center = true }, text = spec.label or "",
          color = function() return P().ink end },
        ui.Rect { anchors = { fill = true }, color = "transparent", border_width = 2,
          border_color = function() return P().focus:alpha(0.6) end, visible = function() return t.visual_focus end } },
      drop_indicator = plates,
    }
  end

  -- An arc of `sweep` degrees round (cx, cy), as path data.
  local function arc(cx, cy, r, from, sweep)
    local function at(deg) local a = math.rad(deg) return cx + r * math.sin(a), cy - r * math.cos(a) end
    local x, y = at(from)
    local d = { ("M%.2f %.2f"):format(x, y) }
    local pieces = math.max(1, math.ceil(sweep / 90))
    for i = 1, pieces do
      local ex, ey = at(from + sweep * i / pieces)
      d[#d + 1] = ("A%.2f %.2f 0 0 1 %.2f %.2f"):format(r, r, ex, ey)
    end
    return table.concat(d, " ")
  end

  --- Pulling to refresh: a raised disc comes down with the pull, its
  --- accent arc turning with it, and spins while the list refreshes.
  S.pull_to_refresh = function(t, spec)
    local REST = spec.pull_distance or 64
    local function shown() return t.refreshing or (t.pull or 0) > 0.02 end
    local spinner = ui.Item { width = 36, height = 36, rotation = function() return (t.pull or 0) * 300 end,
      loop = function() if t.refreshing then return { rotation = { to = 360, duration = 800 } } end return nil end,
      ui.Path { anchors = { fill = true }, view_box = { 0, 0, 36, 36 }, d = arc(18, 18, 10, 0, 270),
        fill_color = "transparent", stroke_width = 3, stroke_cap = "round", stroke_color = function() return P().accent end } }
    return {
      handle = ui.Item {},
      drop_indicator = ui.Rect { width = 40, height = 40, radius = 20, z = 5,
        x = function() return (W(t) - 40) / 2 end,
        y = function() return t.refreshing and (REST - 40) / 2 or math.min(1.4, t.pull or 0) * REST / 2 - 20 end,
        scale = function() return t.refreshing and 1 or 0.4 + 0.6 * clamp01(t.pull or 0) end,
        visible = shown, stretch = M.STRETCH,
        color = function() return P().raised end,
        border_width = function() return P().strong and 1 or 0 end, border_color = function() return P().border end,
        shadow_color = function() return P().dark and "#00000080" or "#00000030" end, shadow_blur = 8,
        shadow_offset_y = 2,
        behavior = { y = M.spring(420, 30) },
        ui.Item { anchors = { center_in = true }, width = 36, height = 36, spinner } },
    }
  end

  --- The grab handle of a sheet: Adwaita's short grey pill, wider and in
  --- the accent while it is held.
  S.sheet_handle = function(t)
    return {
      background = ring(t, R.small),
      handle = ui.Rect { y = 10, height = 4, radius = 2,
        width = function() return t.active and 48 or 36 end,
        x = function() return (W(t) - (t.active and 48 or 36)) / 2 end,
        color = function()
          local p = P()
          if t.active then return p.accent end
          return p.ink:alpha(t.hovered and 0.45 or 0.25)
        end,
        behavior = { width = M.spring(420, 30), x = M.spring(420, 30), color = quick() } },
    }
  end

  --- A window's title strip: the title, a hairline under it, a wash under
  --- the pointer and while it is moved.
  S.window_move = function(t, spec)
    return {
      background = ui.Item { anchors = { fill = true },
        ui.Rect { anchors = { fill = true, margins = 1 }, radius = R.large,
          color = function() local p = P() return p.ink:alpha(t.active and p.wash.active or (t.hovered and p.wash.hover or 0)) end,
          behavior = { color = quick() } },
        ui.Rect { anchors = { left = true, right = true, bottom = true }, height = 1, color = function() return P().border end } },
      content = M.text { anchors = { fill = true }, horizontal_alignment = "center", vertical_alignment = "center",
        text = spec.label or "", font_weight = 700, color = function() return P().ink end },
      handle = M.icon("open_with", 16, function() local p = P() return t.active and p.accent_ink or p.ink_dim end,
        { x = 12, anchors = { vertical_center = true }, opacity = function() return (t.hovered or t.active) and 1 or 0 end,
          behavior = { opacity = quick() } }),
      drop_indicator = ring(t, R.large),
    }
  end

  -- A chip's face: its icon and label.
  local function chip_face(spec, ink)
    return ui.Row { anchors = { center_in = true }, gap = 8, align = "center",
      spec.icon and M.icon(spec.icon, 18, ink) or nil,
      M.text { text = spec.label or "", color = ink } }
  end

  --- Something to drag out: a pill; carried, it stays behind dimmed and a
  --- copy of it in the accent rides the pointer on a shadow.
  S.drag_source = function(t, spec)
    local function h() return math.max(1, H(t)) end
    return {
      handle = ui.Item {},
      background = ui.Rect { anchors = { fill = true }, radius = function() return h() / 2 end,
        opacity = function() return t.active and 0.45 or 1 end,
        color = function()
          local p = P()
          return p.ink:alpha(t.down and p.wash.checked or (t.hovered and p.wash.raised_hover or p.wash.button))
        end,
        border_width = function() return P().strong and 1 or 0 end, border_color = function() return P().border end,
        behavior = { color = quick(), opacity = quick() } },
      content = ui.Item { anchors = { fill = true }, opacity = function() return t.active and 0.45 or 1 end,
        chip_face(spec, function() return P().ink end) },
      ghost = ui.Rect { width = function() return W(t) end, height = function() return H(t) end, z = 30,
        radius = function() return h() / 2 end,
        translate_x = function() return t.delta_x or 0 end, translate_y = function() return t.delta_y or 0 end,
        scale = function() return t.active and 1.06 or 0.9 end,
        visible = function() return t.active end,
        color = function() return P().accent end,
        shadow_color = function() return P().dark and "#00000099" or "#00000040" end, shadow_blur = 16,
        shadow_offset_y = 6,
        behavior = { scale = lift_spring() },
        chip_face(spec, function() return P().on_accent end) },
      drop_indicator = ring(t, function() return h() / 2 end),
    }
  end

  --- Where a drag lands: a rounded outline round an icon and a caption;
  --- while a drag it would take is over it, it fills with the accent, its
  --- outline turns the accent and the icon swells.
  S.drop_zone = function(t, spec)
    return {
      handle = ui.Item {},
      background = ui.Rect { anchors = { fill = true }, radius = R.large,
        color = function() local p = P() return p.accent:alpha(t.accepting and (p.dark and 0.22 or 0.12) or 0) end,
        border_width = 2,
        border_color = function() local p = P() return t.accepting and p.accent or (p.strong and p.border or p.ink:alpha(0.22)) end,
        behavior = { color = quick(), border_color = quick() } },
      content = ui.Item { anchors = { fill = true },
        M.icon(spec.icon or "upload", 32, function() local p = P() return t.accepting and p.accent_ink or p.ink_dim end,
          { anchors = { horizontal_center = true }, y = function() return H(t) / 2 - 34 end,
            scale = function() return t.accepting and 1.3 or 1 end,
            translate_y = function() return t.accepting and -4 or 0 end,
            behavior = { scale = M.spring(380, 22), translate_y = M.spring(380, 22) } }),
        M.text { anchors = { left = true, right = true }, y = function() return H(t) / 2 + 8 end,
          horizontal_alignment = "center", text = spec.label or "Drop here",
          font_weight = function() return t.accepting and 700 or 400 end,
          color = function() local p = P() return t.accepting and p.accent_ink or p.ink_dim end } },
    }
  end
end
