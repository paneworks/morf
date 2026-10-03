-- The default kit's looks for the Transform archetype and each of its
-- widgets, in the Adwaita manner: a floating panel as a GNOME window -- a
-- rounded ground with a soft shadow, a header bar with the title centred
-- and round window buttons, its corners squaring off as it maximizes; an
-- image cropper dimming what is outside the crop, with white corner
-- brackets, a hairline frame and thirds that fade in while it is
-- dragged; a resize box as a blue outline with white knobs that swell
-- under the pointer and a rotate knob on a stalk; a picture-in-picture as
-- a rounded video tile whose controls fade in under the pointer, which
-- squashes as it glides to a corner; an event block as a calendar chip in
-- its tone with a grab bar along its bottom.
--
-- The layouts are shared: Material and Tsugumori call this with their
-- own palette view and roundings (`theme.P`, `theme.radius`), may give
-- the pieces below in `M` first (`transform_knob`, `transform_button`,
-- `transform_rotate_handle`, `transform_tones`, `transform_title`),
-- and restyle what they draw their own way after.
local morf = require("morf")
local ui = require("morf.ui")

return function(S, theme, M)
  local P = theme.P
  local R = theme.radius
  local function get(v) if type(v) == "function" then return v() end return v end
  local function quick() return { duration = theme.duration.small, easing = theme.ease.standard } end
  local function shadow(a) local p = P() return morf.color("#000000"):alpha((p.dark and 1.6 or 1) * a) end
  -- A floating panel's tones: its ground, its header bar and the line
  -- under it (a theme may give its own in `M.transform_tones`).
  local tones = M.transform_tones or {}
  local function tone(name, default) return function() local f = tones[name] return f and f() or default() end end
  local window_ground = tone("ground", function() return P().window end)
  local window_header = tone("header", function() return P().header end)
  local window_line = tone("line", function() return P().border end)
  local window_edge = tone("edge", function() local p = P() return p.dark and p.border or p.shade end)
  -- How a title is written (a theme may set it in capitals).
  local title_text = M.transform_title or function(s) return s end
  -- The light a cropper's lines are drawn in over any picture.
  local function paper() local p = P() return p.paper or p.on_accent end

  -- ------------------------------------------------------------ pieces --

  --- A grip's look on a resize box: a corner a knob that swells under the
  --- pointer and while held, an edge a short pill along it.
  local knob = M.transform_knob or function(s, opts)
    opts = opts or {}
    local size = opts.size or 10
    local function big() return s.hovered() or s.held() end
    local vertical = s.name == "e" or s.name == "w"
    local w = function() if s.corner then return big() and size + 4 or size end return vertical and 5 or 18 end
    local h = function() if s.corner then return big() and size + 4 or size end return vertical and 18 or 5 end
    return ui.Rect { anchors = { center_in = true }, width = w, height = h,
      radius = function() return opts.square and 2 or math.min(get(w), get(h)) / 2 end,
      color = function() local p = P() return s.held() and p.accent or (p.knob or p.on_accent) end,
      border_width = 1.5, border_color = function() return P().accent end,
      shadow_color = function() return shadow(0.25) end, shadow_blur = 3, shadow_offset_y = 1,
      behavior = { width = M.spring(520, 30), height = M.spring(520, 30), color = quick() } }
  end
  M.transform_knob = knob

  --- A window button: a round wash that deepens under the pointer, the
  --- symbol in the ink.
  local button = M.transform_button or function(icon, name, action, size)
    size = size or 24
    local area
    area = ui.MouseArea { width = size, height = size, cursor = "pointer", accessible_role = "button",
      accessible_name = name, on_clicked = action,
      ui.Rect { anchors = { fill = true }, radius = size / 2,
        color = function()
          local p = P()
          local a = p.wash.button
          if area and area.pressed then a = p.wash.active elseif area and area.hovered then a = p.wash.button + p.wash.hover end
          return p.ink:alpha(a)
        end, behavior = { color = quick() } },
      M.icon(icon, 16, function() return P().ink end, { anchors = { center_in = true } }) }
    return area
  end
  M.transform_button = button

  --- A button over a picture: a round dark wash that deepens under the
  --- pointer, the symbol in the picture's light.
  local function over_picture(icon, name, action, size)
    size = size or 28
    local area
    area = ui.MouseArea { width = size, height = size, cursor = "pointer", accessible_role = "button",
      accessible_name = name, on_clicked = action,
      ui.Rect { anchors = { fill = true }, radius = size / 2,
        color = function()
          local a = (area and area.pressed) and 0.6 or ((area and area.hovered) and 0.5 or 0.35)
          return morf.color("#000000"):alpha(a)
        end, behavior = { color = quick() } },
      M.icon(icon, 16, paper, { anchors = { center_in = true } }) }
    return area
  end

  --- The keyboard's ring round the box.
  local function ring(t, radius)
    return ui.Rect { anchors = { fill = true, margins = -3 }, color = "transparent", radius = radius,
      border_width = 2, border_color = function() local p = P() return p.focus or p.accent end,
      visible = function() return t.visual_focus end }
  end

  --- What a turned box is turned by: a hairline stalk up from the top
  --- edge to a knob with the turn's symbol.
  local rotate_handle = M.transform_rotate_handle or function(spec, opts)
    opts = opts or {}
    local STALK, KNOB = spec.stalk or 28, opts.knob or 18
    return ui.Item { anchors = { horizontal_center = true, top = true, top_margin = -STALK - KNOB / 2 },
      width = KNOB, height = STALK + KNOB / 2,
      ui.Rect { x = math.floor(KNOB / 2), anchors = { bottom = true }, width = 1, height = STALK - KNOB / 2,
        color = function() return P().accent end },
      ui.Rect { width = KNOB, height = KNOB, radius = KNOB / 2,
        color = function() local p = P() return p.knob or p.on_accent end,
        border_width = 1.5, border_color = function() return P().accent end,
        shadow_color = function() return shadow(0.25) end, shadow_blur = 3, shadow_offset_y = 1,
        M.icon("rotate_right", 13, function() return P().accent end, { anchors = { center_in = true } }) } }
  end
  M.transform_rotate_handle = rotate_handle

  -- --------------------------------------------------------- archetype --

  --- Any transform: the accent's outline, knobs on the corners, the turn's
  --- stalk.
  function S.Transform(t, spec)
    return {
      frame = ui.Item { anchors = { fill = true },
        ui.Rect { anchors = { fill = true }, color = "transparent", border_width = 1,
          border_color = function() return P().accent end },
        ring(t, R.small) },
      handle = function(s) return knob(s) end,
      rotate_handle = get(spec.rotatable) and rotate_handle(spec) or nil,
    }
  end

  -- ---------------------------------------------------------- widgets --

  --- A floating panel: a GNOME window. Its ground rounds at the window's
  --- radius and squares off as it maximizes; a header bar the height of
  --- the title bar with the title centred (dimmed while it has no focus)
  --- and round minimize, maximize and close buttons at its end.
  function S.floating_panel(t, spec, node, send)
    local TITLE = spec.title_height or 40
    local function radius() return t.maximized and 0 or R.window end
    local ease = { radius = M.spring(420, 40) }
    local buttons = ui.Row { anchors = { right = true, top = true, right_margin = 8 }, height = TITLE, gap = 8,
      align = "center",
      button("minimize", "Minimize", function() send("minimize") end),
      button(function() return t.maximized and "filter_none" or "crop_square" end, "Maximize",
        function() send("maximize") end),
      button("close", "Close", function() if spec.on_close then spec.on_close() end end) }
    return {
      background = ui.Rect { anchors = { fill = true }, radius = radius, behavior = ease,
        color = window_ground,
        border_width = function() local p = P() return p.strong and 2 or 1 end,
        border_color = window_edge,
        shadow_color = function() return shadow(t.down and 0.28 or 0.18) end,
        shadow_blur = function() return t.down and 22 or 14 end, shadow_offset_y = 4 },
      frame = ui.Item { anchors = { fill = true },
        ui.Rect { anchors = { left = true, right = true, top = true, margins = 1 }, height = TITLE - 1,
          radius = radius, behavior = ease, color = window_header },
        ui.Rect { anchors = { left = true, right = true, top = true, left_margin = 1, right_margin = 1,
          top_margin = TITLE / 2 }, height = TITLE / 2 - 1, color = window_header,
          visible = function() return not t.minimized end },
        ui.Rect { anchors = { left = true, right = true, top = true, top_margin = TITLE - 1 }, height = 1,
          color = window_line, visible = function() return not t.minimized end },
        M.text { anchors = { left = true, right = true, top = true, left_margin = 96, right_margin = 104 },
          height = TITLE, text = function() return title_text(tostring(get(spec.title) or "")) end, font_weight = 700,
          horizontal_alignment = "center", vertical_alignment = "center", elide = "right",
          color = function() local p = P() return t.focused and p.ink or p.ink_dim end, behavior = { color = quick() } },
        buttons,
        ring(t, R.window) },
      handle = function() return nil end,
    }
  end

  --- An image cropper: what is outside the crop dimmed, a hairline frame
  --- with brackets on its corners, short bars at the middle of its edges,
  --- and the thirds while a drag is under way.
  function S.image_cropper(t, spec)
    local function cw() return t.container_width end
    local function ch() return t.container_height end
    local function dim() return morf.color("#000000"):alpha(0.62) end
    local function shade(props) props.color = dim return ui.Rect(props) end
    local B, L = 3, 18
    local function bracket(anchors, horizontal)
      return ui.Rect { anchors = anchors, width = horizontal and L or B, height = horizontal and B or L, color = paper }
    end
    local guides = ui.Item { anchors = { fill = true },
      opacity = function() return t.active ~= "none" and 1 or 0 end, behavior = { opacity = quick() } }
    for i = 1, 2 do
      ui.reparent(ui.Rect { width = 1, anchors = { top = true, bottom = true }, color = function() return paper():alpha(0.6) end,
        x = function() return math.floor(t.box_width * i / 3) end }, guides)
      ui.reparent(ui.Rect { height = 1, anchors = { left = true, right = true }, color = function() return paper():alpha(0.6) end,
        y = function() return math.floor(t.box_height * i / 3) end }, guides)
    end
    return {
      background = get(spec.dim) ~= false and ui.Item { anchors = { fill = true },
        shade { x = function() return -t.x end, y = function() return -t.y end, width = cw,
          height = function() return math.max(0, t.y) end },
        shade { x = function() return -t.x end, y = function() return t.box_height end, width = cw,
          height = function() return math.max(0, ch() - t.y - t.box_height) end },
        shade { x = function() return -t.x end, width = function() return math.max(0, t.x) end,
          height = function() return t.box_height end },
        shade { x = function() return t.box_width end, width = function() return math.max(0, cw() - t.x - t.box_width) end,
          height = function() return t.box_height end } } or nil,
      frame = ui.Item { anchors = { fill = true },
        ui.Rect { anchors = { fill = true }, color = "transparent", border_width = 1,
          border_color = function() return paper():alpha(0.85) end },
        bracket({ left = true, top = true, left_margin = -B, top_margin = -B }, true),
        bracket({ left = true, top = true, left_margin = -B, top_margin = -B }, false),
        bracket({ right = true, top = true, right_margin = -B, top_margin = -B }, true),
        bracket({ right = true, top = true, right_margin = -B, top_margin = -B }, false),
        bracket({ left = true, bottom = true, left_margin = -B, bottom_margin = -B }, true),
        bracket({ left = true, bottom = true, left_margin = -B, bottom_margin = -B }, false),
        bracket({ right = true, bottom = true, right_margin = -B, bottom_margin = -B }, true),
        bracket({ right = true, bottom = true, right_margin = -B, bottom_margin = -B }, false),
        ring(t, 0) },
      guide = get(spec.guides) ~= false and guides or nil,
      handle = function(s)
        if s.corner then return nil end
        local vertical = s.name == "e" or s.name == "w"
        return ui.Rect { anchors = { center_in = true }, radius = 1.5, color = paper,
          width = function() return vertical and B or ((s.hovered() or s.held()) and 28 or 20) end,
          height = function() return vertical and ((s.hovered() or s.held()) and 28 or 20) or B end,
          behavior = { width = M.spring(520, 30), height = M.spring(520, 30) } }
      end,
    }
  end

  --- A resize box: the accent's outline round the item, white knobs on
  --- its corners and edges, the turn's knob on a stalk, and the size
  --- while it is resized.
  function S.resize_box(t, spec)
    return {
      frame = ui.Item { anchors = { fill = true },
        ui.Rect { anchors = { fill = true, margins = -1 }, color = "transparent", border_width = 1.5,
          border_color = function() return P().accent end },
        ring(t, R.small) },
      handle = function(s) return knob(s) end,
      rotate_handle = get(spec.rotatable) and rotate_handle(spec) or nil,
      guide = ui.Rect { anchors = { horizontal_center = true }, y = function() return t.box_height + 12 end,
        width = 96, height = 26, radius = 13, color = function() return P().accent end,
        opacity = function() return t.active == "resize" and 1 or 0 end, behavior = { opacity = quick() },
        M.text { anchors = { fill = true }, horizontal_alignment = "center", vertical_alignment = "center",
          font_size = theme.size.small, font_weight = 600, color = function() return P().on_accent end,
          text = function() return ("%d × %d"):format(math.floor(t.box_width + 0.5), math.floor(t.box_height + 0.5)) end } },
    }
  end

  --- A picture-in-picture: a rounded tile with a shadow, its content cut
  --- to its corners, a scrim with its buttons that fades in under the
  --- pointer; it squashes a little as it glides to a corner.
  function S.pip_window(t, spec, node, send)
    t.content_radius = R.large
    if node and M.STRETCH then node.stretch = M.STRETCH end
    local function shown() return t.hovered or t.down end
    return {
      background = ui.Rect { anchors = { fill = true }, radius = R.large,
        color = function() return P().shade end,
        shadow_color = function() return shadow(t.down and 0.4 or 0.28) end,
        shadow_blur = function() return t.down and 24 or 16 end, shadow_offset_y = 6 },
      frame = ui.Item { anchors = { fill = true },
        ui.Rect { anchors = { left = true, right = true, top = true }, height = 44, radius = R.large,
          gradient = { angle = 180, stops = { morf.color("#000000"):alpha(0.55), morf.color("#000000"):alpha(0) } },
          opacity = function() return shown() and 1 or 0 end, behavior = { opacity = quick() } },
        ui.Row { anchors = { right = true, top = true, margins = 8 }, gap = 6,
          opacity = function() return shown() and 1 or 0 end, behavior = { opacity = quick() },
          over_picture("open_in_full", "Expand", function() send("maximize") end),
          over_picture("close", "Close", function() if spec.on_close then spec.on_close() end end) },
        ui.Rect { anchors = { fill = true }, color = "transparent", radius = R.large, border_width = 1,
          border_color = function() return P().border:alpha(0.5) end },
        ring(t, R.large) },
      handle = function() return nil end,
    }
  end

  --- An event block: a calendar chip in its tone (`tone`, the accent by
  --- default) with a stripe down its leading edge and a grab bar along
  --- its bottom that shows under the pointer; lifted while it is held.
  function S.event_block(t, spec)
    local function tone() return get(spec.tone) or "accent" end
    local function fill() return M.canvas_fill and M.canvas_fill(tone()) or P().card end
    local function strong() return (M.canvas_tone and M.canvas_tone(tone())) or P().accent end
    return {
      background = ui.Rect { anchors = { fill = true }, radius = R.medium, color = fill,
        shadow_color = function() return shadow(t.active ~= "none" and 0.3 or 0) end,
        shadow_blur = function() return t.active ~= "none" and 14 or 0 end, shadow_offset_y = 3,
        ui.Rect { anchors = { left = true, top = true, bottom = true, margins = 4 }, width = 3, radius = 1.5, color = strong } },
      frame = ui.Item { anchors = { fill = true },
        ui.Rect { anchors = { horizontal_center = true, bottom = true, bottom_margin = 5 }, height = 4, radius = 2,
          width = function() return t.handle == "s" and 36 or 24 end, color = strong,
          opacity = function() return (t.hovered or t.active ~= "none") and 0.9 or 0.35 end,
          behavior = { opacity = quick(), width = M.spring(520, 30) } },
        ui.Rect { anchors = { fill = true }, color = "transparent", radius = R.medium, border_width = 1.5,
          border_color = strong, opacity = function() return t.focused and 1 or 0 end, behavior = { opacity = quick() } },
        ring(t, R.medium) },
      handle = function() return nil end,
    }
  end
end
