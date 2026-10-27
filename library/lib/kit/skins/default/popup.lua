-- The default kit's Popup skin, in the Adwaita manner: raised grounds with
-- a hairline and a soft shadow; popovers from a button with a beak
-- filleted into their body (one distance field); dark translucent tooltips
-- and toasts; dialogs with the window's 15 px corners; a bottom sheet flush
-- with the edge it comes from. Each comes in from where it hangs -- a menu
-- grows out of its button, a sheet slides, a toast stretches -- on
-- critically damped springs (none with motion reduced). What a popup holds
-- and how it behaves are not the skin's.
--
-- `S.popup_motion(widget, spec, state)` is how a popup comes in, which
-- lib.kit.archetypes.popup asks the skin for when it builds one.
local morf = require("morf")
local ui = require("morf.ui")

return function(S, theme, draw)
  local P = theme.P
  local R = theme.radius
  local get = draw.get
  local function shade(a) return morf.color("#000000"):alpha(a) end

  -- ------------------------------------------------------------ motion --

  --- The side of its anchor a popup hangs on and where along it.
  local function placement(t)
    local side, align = tostring(t.placement or "center"):match("^(%a+)%-?(%a*)$")
    return side or "center", (align ~= nil and align ~= "") and align or "center"
  end
  local ALONG = { start = 0, center = 0.5, ["end"] = 1 }

  -- How each comes in: grows out of its anchor's side, from the middle,
  -- rises by its own size (a sheet), stretches (a toast), slides in from the
  -- right (a notification), zooms (an about dialog).
  local HOW = {
    popover_menu = "anchored", popover_menu_bar = "anchored", popover = "anchored", tooltip = "anchored",
    dropdown = "anchored", dialog = "center", alert_dialog = "center", about_dialog = "zoom",
    preferences_dialog = "center", shortcuts_dialog = "center", bottom_sheet = "rise",
    toast = "stretch", notification = "slide",
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

  --- The motion the glue gives a popup's node as it makes it (lib.kit.archetypes.popup:
  --- `popup_motion`): its pose, at rest while `state.open()` and back in
  --- the one it comes in from while shut -- so every opening travels the
  --- same way -- and the springs that carry it. Adwaita's springs do not
  --- overshoot; a toast's and a picture's do, a little.
  function S.popup_motion(widget, spec, state)
    local how = HOW[widget] or "center"
    local glide = draw.spring(how == "anchored" and 520 or 380)
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
  local function raised() return P().popover_bg end
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

  local function stack(...) return ui.Item { anchors = { fill = true }, ... } end
  -- The archetype's dim slot would lay a shade over the popup itself; the
  -- layer's own scrim dims what is under it.
  local function none() return ui.Item {} end
  local function handle()
    return ui.Rect { anchors = { top = true, horizontal_center = true, top_margin = 10 }, width = 36, height = 4,
      radius = 2, z = 5, color = function() local p = P() return p.window_fg:alpha(p.strong and 0.8 or 0.24) end }
  end

  -- ------------------------------------------------------------- looks --

  local LOOK = {}

  -- A popover menu from a button wears the beak; one from the pointer or
  -- a bar (`has_arrow = false`) does not.
  function LOOK.popover_menu(t, spec)
    if spec.has_arrow == false then return { background = ground { corners = R.large } } end
    return { background = beaked(t, { radius = R.large }) }
  end
  function LOOK.popover_menu_bar()
    -- Hung from the bar: square where it meets it.
    return { background = ground { corners = { R.small / 2, R.small / 2, R.large, R.large } } }
  end
  function LOOK.dropdown() return { background = ground { corners = R.medium } } end
  function LOOK.popover(t, spec)
    if spec.has_arrow == false then return { background = ground { corners = R.large } } end
    return { background = beaked(t, { radius = R.large }) }
  end
  function LOOK.tooltip()
    return { background = ui.Rect { anchors = { fill = true }, radius = R.small,
      color = function() local p = P() return p.strong and p.window_fg or shade(0.82) end } }
  end

  -- Dialogs, on the dialog's ground.
  local function dialog() return { background = ground { corners = R.window, depth = 2,
    color = function() return P().dialog_bg end } } end
  LOOK.dialog, LOOK.alert_dialog, LOOK.about_dialog = dialog, dialog, dialog
  LOOK.preferences_dialog, LOOK.shortcuts_dialog = dialog, dialog

  -- AdwBottomSheet: flush with the bottom, a drag handle at its top.
  function LOOK.bottom_sheet()
    return { background = ground { corners = { R.window, R.window, 0, 0 }, depth = 2 }, content = handle() }
  end

  -- AdwToast: words alone, the dark pill.
  function LOOK.toast(t)
    return { background = ui.Rect { anchors = { fill = true },
      radius = function() return (t.height or 0) / 2 end,
      color = function() local p = P() return p.strong and p.window_fg or shade(0.84) end } }
  end
  -- A desktop notification: a raised card.
  function LOOK.notification() return { background = ground { corners = R.large, depth = 2 } } end

  for widget, look in pairs(LOOK) do
    S[widget] = function(t, spec)
      local slots = look(t, spec)
      slots.dim = none()
      return slots
    end
  end

  --- Any other popup: the popover's ground.
  function S.Popup()
    return { dim = none(), background = ground { corners = R.large } }
  end
end
