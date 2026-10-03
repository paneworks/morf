-- The default kit's look for each Popup widget, in the Adwaita manner:
-- raised grounds with a hairline and a soft shadow; popovers and menus
-- from a button with a beak filleted into their body (one distance field);
-- dark translucent tooltips and toasts; dialogs with the window's 15 px
-- corners; sheets flush with the edge they come from. Each comes in from
-- where it hangs -- a menu grows out of its button, a sheet slides, a toast
-- stretches -- on critically damped springs (none with motion reduced).
-- What a popup holds and how it behaves are not the skin's.
local morf = require("morf")
local ui = require("morf.ui")

return function(S, theme, M)
  local P = theme.P
  local R = theme.radius
  local function shade(a) return morf.color("#000000"):alpha(a) end
  local function get(v) if type(v) == "function" then return v() end return v end

  -- ------------------------------------------------------------- inks --

  -- Popups drawn on the dark on-screen-display ground (a tooltip, a toast
  -- of words, a snackbar, a lightbox) write in white; the glue and a
  -- configuration ask here (lib.kit.popup: `popup_ink`).
  local OSD = { tooltip = true, snackbar = true, lightbox = true, toast = true }
  function M.popup_ink(widget, level)
    if not OSD[widget] then return nil end
    return function()
      local p = P()
      if level == "lo" then return p.on_accent:alpha(0.72) end
      if level == "accent" then return p.strong and p.on_accent or p.accent:mix(p.on_accent, 0.45) end
      return p.on_accent
    end
  end

  -- ------------------------------------------------------------ motion --

  --- The side of its anchor a popup hangs on and where along it.
  local function placement(t)
    local side, align = tostring(t.placement or "center"):match("^(%a+)%-?(%a*)$")
    return side or "center", (align ~= nil and align ~= "") and align or "center"
  end
  local ALONG = { start = 0, center = 0.5, ["end"] = 1 }

  -- How each comes in: grows out of its anchor's side, from the middle,
  -- rises / slides in by its own size (a sheet), drops from above, stretches
  -- (a toast), slides in from the right (a notification), zooms (a picture).
  local HOW = {
    menu = "anchored", context_menu = "anchored", menu_bar_menu = "anchored", submenu = "anchored",
    dropdown_list = "anchored", autocomplete_list = "anchored", popover = "anchored", tooltip = "anchored",
    rich_tooltip = "anchored", hover_card = "anchored", tour_step = "anchored",
    command_palette = "drop", dialog = "center", message_dialog = "center", alert_dialog = "center",
    about_dialog = "zoom", preferences_dialog = "center", shortcuts_dialog = "center",
    bottom_sheet = "rise", side_sheet = "from_end", drawer = "from_start",
    toast = "stretch", snackbar = "stretch", banner = "drop", notification_popup = "slide", lightbox = "zoom",
  }

  --- The pose a popup comes in from (and is put back in while it is shut):
  --- its scale, horizontal scale, offsets and opacity, about an origin on
  --- the side it hangs from. `w`, `h`: its size, as far as it is known.
  local function pose(widget, placement_, w, h)
    local how = HOW[widget] or "center"
    local side, align = tostring(placement_ or "center"):match("^(%a+)%-?(%a*)$")
    side, align = side or "center", (align ~= nil and align ~= "") and align or "center"
    local p = { ox = 0.5, oy = 0.5, tx = 0, ty = 0, s = 1, sx = 1, op = 0 }
    if how == "anchored" then
      p.s = 0.94
      if side == "bottom" then p.oy, p.ty, p.ox = 0, -6, ALONG[align]
      elseif side == "top" then p.oy, p.ty, p.ox = 1, 6, ALONG[align]
      elseif side == "right" then p.ox, p.tx, p.oy = 0, -6, ALONG[align]
      elseif side == "left" then p.ox, p.tx, p.oy = 1, 6, ALONG[align]
      else p.s = 0.9 end
    elseif how == "center" then p.s = 0.92
    elseif how == "rise" then p.ty, p.op = h + 48, 1
    elseif how == "from_end" then p.tx, p.op = w + 48, 1
    elseif how == "from_start" then p.tx, p.op = -(w + 48), 1
    elseif how == "drop" then p.oy, p.ty, p.s = 0, -18, 0.96
    elseif how == "stretch" then p.sx, p.s, p.ty = 0.55, 0.9, 14
    elseif how == "slide" then p.tx, p.s = 56, 0.98
    elseif how == "zoom" then p.s = 0.82 end
    return p
  end

  local function known(v, fallback) v = get(v) return (type(v) == "number" and v > 0) and v or fallback end

  --- The motion the glue gives a popup's node as it makes it (lib.kit.popup:
  --- `popup_motion`): its pose, at rest while `state.open()` and back in
  --- the one it comes in from while shut -- so every opening travels the
  --- same way -- and the springs that carry it. Adwaita's springs do not
  --- overshoot; a toast's and a picture's do, a little.
  function M.popup_motion(widget, spec, state)
    local how = HOW[widget] or "center"
    local glide = M.spring(how == "anchored" and 520 or 380)
    local pop = theme.reduced and { duration = 0 } or ui.spring { stiffness = 380, damping = 20 }
    local grow = (how == "stretch" or how == "zoom") and pop or glide
    -- (Its size as the spec gives it: a pose that read the node's own
    -- layout would chase itself.)
    local rest = pose(widget, spec.placement, known(spec.width, 280), known(spec.height, 280))
    local function now() return rest end
    local function shut(key, rest) return function() if state.open() then return rest end return now()[key] end end
    return {
      -- How long it stays in the layer once shut, while it goes.
      linger = (theme.reduced or (theme.duration.small or 0) <= 0) and 0 or 220,
      behavior = { scale = grow, scale_x = grow, translate_x = glide, translate_y = glide,
        opacity = { duration = theme.duration.small, easing = theme.ease.decelerate } },
      transform_origin_x = function() return now().ox end, transform_origin_y = function() return now().oy end,
      scale = shut("s", 1), scale_x = shut("sx", 1), translate_x = shut("tx", 0), translate_y = shut("ty", 0),
      opacity = function() return (state.open() or now().op == 1) and 1 or 0 end,
    }
  end

  -- ----------------------------------------------------------- grounds --

  local function hairline() local p = P() return p.strong and p.border or p.shade:alpha(p.dark and 0.9 or 0.55) end
  local function edge() return P().strong and 2 or 1 end
  local function raised() return P().raised end
  --- A shadow `depth` (1 a menu, 2 a dialog, 3 a palette) under a ground.
  local function shadow(props, depth)
    props.shadow_color = function()
      local p = P()
      if p.strong then return shade(0) end
      return shade((p.dark and 0.36 or 0.09) + depth * 0.02)
    end
    props.shadow_blur = 3 + depth * 3
    props.shadow_offset_y = depth
    return props
  end

  --- A ground: `corners` (one radius, or { tl, tr, br, bl }), `color`,
  --- `border` (a colour binding), `width` (the border's), `depth`.
  local function ground(o)
    local c = type(o.corners) == "table" and o.corners or { o.corners, o.corners, o.corners, o.corners }
    return ui.Rect(shadow({ anchors = { fill = true },
      top_left_radius = c[1], top_right_radius = c[2], bottom_right_radius = c[3], bottom_left_radius = c[4],
      color = o.color or raised, border_width = o.width or edge, border_color = o.border or hairline,
      behavior = { color = { duration = theme.duration.small } } }, o.depth or 1))
  end

  -- A beak's box: the field's triangle is inscribed in it, its base sunk
  -- 3 px into the body and its tip 10.5 px out, towards the anchor.
  local BEAK, ROOM = 18, 24
  --- A body with a beak filleted into it as one distance field, its edge
  --- and shadow drawn round both: `radius`, `color`, `border`, `width`,
  --- `depth`. The beak sits on the side facing the anchor.
  local function beaked(t, o)
    local function at()
      local side, align = placement(t)
      local w, h = t.width or 0, t.height or 0
      local function along(len) return align == "start" and 22 or (align == "end" and len - 22 or len / 2) end
      if side == "bottom" then return along(w) - 9, -10.5, 0 end
      if side == "top" then return along(w) - 9, h - 7.5, 180 end
      if side == "right" then return -10.5, along(h) - 9, -90 end
      if side == "left" then return w - 7.5, along(h) - 9, 90 end
    end
    local field = ui.Sdf(shadow({ anchors = { fill = true, margins = -ROOM }, blend = 6, blend_profile = "circular",
      fill_color = o.color or raised, stroke_color = o.border or hairline, stroke_width = o.width or edge,
      ui.SdfShape { shape = "box", anchors = { fill = true, margins = ROOM }, radius = o.radius or R.large },
      ui.SdfShape { shape = "triangle", operation = "smooth_union", width = BEAK, height = BEAK,
        visible = function() return at() ~= nil end,
        x = function() local x = at() return (x or 0) + ROOM end,
        y = function() local _, y = at() return (y or 0) + ROOM end,
        rotation = function() local _, _, r = at() return r or 0 end },
    }, o.depth or 1))
    return ui.Item { anchors = { fill = true }, field }
  end

  --- A wash of `tone` across the top `height` px of a ground, fading out:
  --- an about box's glow, a card's cover.
  local function glow(tone, height, radius, strength)
    return ui.Rect { anchors = { left = true, right = true, top = true, margins = 1 }, height = height,
      top_left_radius = radius, top_right_radius = radius,
      gradient = function()
        local c = tone()
        return { angle = 180, stops = { c:alpha(strength), c:alpha(0) } }
      end }
  end

  local function stack(...) return ui.Item { anchors = { fill = true }, ... } end
  -- The archetype's dim slot would lay a shade over the popup itself; the
  -- layer's own scrim dims what is under it.
  local function none() return ui.Item {} end
  local function handle()
    return ui.Rect { anchors = { top = true, horizontal_center = true, top_margin = 10 }, width = 36, height = 4,
      radius = 2, z = 5, color = function() local p = P() return p.ink:alpha(p.strong and 0.8 or 0.24) end }
  end
  local function accent() return P().accent end

  -- ------------------------------------------------------------- looks --

  local LOOK = {}

  -- Menus: a popover menu from a button wears the beak; one from the
  -- pointer, a bar or a field does not.
  function LOOK.menu(t) return { background = beaked(t, { radius = R.large }) } end
  function LOOK.context_menu() return { background = ground { corners = R.large } } end
  function LOOK.menu_bar_menu()
    -- Hung from the bar: square where it meets it.
    return { background = ground { corners = { R.small / 2, R.small / 2, R.large, R.large } } }
  end
  function LOOK.submenu()
    -- One step deeper than the menu it came from.
    return { background = ground { corners = R.large,
      color = function() local p = P() return p.raised:mix(p.ink, p.dark and 0.05 or 0.025) end } }
  end
  function LOOK.dropdown_list() return { background = ground { corners = R.medium } } end
  function LOOK.autocomplete_list()
    -- Hangs from the field being typed in, edged in its focus colour.
    return { background = ground { corners = { R.small / 2, R.small / 2, R.medium, R.medium },
      border = function() local p = P() return p.strong and p.accent or p.focus:alpha(0.45) end } }
  end

  -- Floating.
  function LOOK.popover(t) return { background = beaked(t, { radius = R.large }) } end
  function LOOK.tooltip()
    return { background = ui.Rect { anchors = { fill = true }, radius = R.small,
      color = function() local p = P() return p.strong and p.ink or shade(0.82) end } }
  end
  function LOOK.rich_tooltip()
    return { background = stack(ground { corners = R.large },
      ui.Rect { anchors = { left = true, top = true, bottom = true, left_margin = 1, top_margin = 12, bottom_margin = 12 },
        width = 3, radius = 1.5, color = accent }) }
  end
  function LOOK.hover_card()
    -- A profile card: a cover of the accent behind its head.
    return { background = stack(ground { corners = R.large, depth = 2 }, glow(accent, 52, R.large, 0.22)) }
  end
  function LOOK.tour_step(t)
    return { background = beaked(t, { radius = R.large, border = accent, width = 2, depth = 2 }) }
  end

  -- Dialogs.
  function LOOK.command_palette()
    return { background = ground { corners = R.window, depth = 3 } }
  end
  function LOOK.dialog() return { background = ground { corners = R.window, depth = 2 } } end
  function LOOK.message_dialog()
    return { background = stack(ground { corners = R.window, depth = 2 }, glow(accent, 64, R.window, 0.12)) }
  end
  function LOOK.alert_dialog()
    return { background = stack(ground { corners = R.window, depth = 2 },
      glow(function() return P().destructive end, 80, R.window, 0.14)) }
  end
  function LOOK.about_dialog()
    return { background = stack(ground { corners = R.window, depth = 2 }, glow(accent, 110, R.window, 0.24)) }
  end
  -- Windows of their own: the window's ground, not the raised one.
  local function window() return { background = ground { corners = R.window, depth = 2, color = function() return P().window end } } end
  function LOOK.preferences_dialog() return window() end
  function LOOK.shortcuts_dialog() return window() end

  -- Sheets: flush with the edge they come from.
  function LOOK.bottom_sheet()
    return { background = ground { corners = { R.window, R.window, 0, 0 }, depth = 2 }, content = handle() }
  end
  function LOOK.side_sheet()
    return { background = ground { corners = { R.window, 0, 0, R.window }, depth = 2 } }
  end
  function LOOK.drawer()
    return { background = ground { corners = { 0, R.window, R.window, 0 }, depth = 2,
      color = function() return P().sidebar end } }
  end

  -- Transient.
  function LOOK.toast(t, spec)
    -- Words alone: the dark pill. A tray of notes: a raised one.
    if spec.content then return { background = ground { corners = R.large, depth = 2 } } end
    return { background = ui.Rect { anchors = { fill = true },
      radius = function() return (t.height or 0) / 2 end,
      color = function() local p = P() return p.strong and p.ink or shade(0.84) end } }
  end
  function LOOK.snackbar(t, spec)
    -- The dark bar, and the time it has left running out along its foot.
    local left = ui.Rect { anchors = { left = true, right = true, bottom = true, left_margin = R.medium,
      right_margin = R.medium, bottom_margin = 3 }, height = 2, radius = 1, transform_origin_x = 0,
      color = M.popup_ink("snackbar", "accent") }
    local running
    morf.effect("kit.default.snackbar." .. tostring(left), function()
      local open = t.open
      if running then running:stop() running = nil end
      if open and not theme.reduced and spec.behavior == nil then
        running = morf.animation.play { { node = left, property = "scale_x", from = 1, to = 0,
          duration = spec.timeout or 5000 } }
      else left.scale_x = 1 end
    end, { owner = left })
    return { background = stack(ui.Rect { anchors = { fill = true }, radius = R.medium,
      color = function() local p = P() return p.strong and p.ink or shade(0.86) end }, left) }
  end
  function LOOK.banner()
    return { background = stack(ground { corners = R.medium,
        color = function() local p = P() return p.raised:mix(p.accent, p.dark and 0.2 or 0.1) end,
        border = function() return P().accent:alpha(0.35) end },
      ui.Rect { anchors = { left = true, top = true, bottom = true, margins = 1 }, width = 4,
        top_left_radius = R.medium, bottom_left_radius = R.medium, color = accent }) }
  end
  function LOOK.notification_popup() return { background = ground { corners = R.window, depth = 2 } } end
  function LOOK.lightbox()
    return { background = ui.Rect(shadow({ anchors = { fill = true }, radius = R.large, color = shade(0.9) }, 3)) }
  end

  for widget, look in pairs(LOOK) do
    S[widget] = function(t, spec)
      local slots = look(t, spec)
      slots.dim = none()
      return slots
    end
  end
end
