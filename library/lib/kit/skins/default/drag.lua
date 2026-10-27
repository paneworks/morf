-- The default kit's Drag skin, in the Adwaita manner: a GtkPaned's
-- hairline divider whose grip shows under the pointer, a window handle's
-- quiet wash, a drag source's pill that leaves a dimmed copy behind while
-- an accent one rides the pointer, and a drop target that fills with the
-- accent while it would take what is over it. Behaviour -- following,
-- snapping -- is lib.kit.archetypes.drag's and the archetype's.
local ui = require("morf.ui")

return function(S, theme, draw)
  local P = theme.P
  local R = theme.radius
  local quick = draw.quick
  local function lift_spring() return draw.spring(520, 32) end
  local function W(t) return (t.width and t.width > 0) and t.width or 0 end
  local function H(t) return (t.height and t.height > 0) and t.height or 0 end
  local ring = draw.ring

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
          color = function() local p = P() return t.active and p.accent_bg or p.border end,
          behavior = { color = quick() } },
        ui.Rect {
          x = function() return (W(t) - (across and 4 or 32)) / 2 end,
          y = function() return (H(t) - (across and 32 or 4)) / 2 end,
          width = across and 4 or 32, height = across and 32 or 4, radius = 2,
          scale = function() return t.active and 1.25 or 1 end,
          opacity = function() return shown() and 1 or 0 end,
          color = function() local p = P() return t.active and p.accent_bg or p.window_fg:alpha(0.45) end,
          behavior = { opacity = quick(), color = quick(), scale = draw.spring(420, 30) } } },
    }
    for k, v in pairs(extra or {}) do slots[k] = v end
    return slots
  end

  S.paned = function(t, spec) return divider(t, spec) end

  --- GtkWindowHandle: a wash under the pointer and while the window moves.
  S.window_handle = function(t)
    return {
      background = ui.Rect { anchors = { fill = true }, radius = R.large,
        color = function() local p = P() return p.window_fg:alpha(t.active and p.wash.active or (t.hovered and p.wash.hover or 0)) end,
        behavior = { color = quick() } },
      drop_indicator = ring(t, R.large),
    }
  end

  -- A chip's face: its icon and label.
  local function chip_face(spec, ink)
    return ui.Row { anchors = { center_in = true }, gap = 8, align = "center",
      spec.icon and draw.icon(spec.icon, 18, ink) or nil,
      draw.text { text = spec.label or "", color = ink } }
  end

  --- GtkDragSource: a pill; carried, it stays behind dimmed and a
  --- copy of it in the accent rides the pointer on a shadow.
  S.drag_source = function(t, spec)
    local function h() return math.max(1, H(t)) end
    return {
      handle = ui.Item {},
      background = ui.Rect { anchors = { fill = true }, radius = function() return h() / 2 end,
        opacity = function() return t.active and 0.45 or 1 end,
        color = function()
          local p = P()
          return p.window_fg:alpha(t.down and p.wash.checked or (t.hovered and p.wash.raised_hover or p.wash.button))
        end,
        border_width = function() return P().strong and 1 or 0 end, border_color = function() return P().border end,
        behavior = { color = quick(), opacity = quick() } },
      content = ui.Item { anchors = { fill = true }, opacity = function() return t.active and 0.45 or 1 end,
        chip_face(spec, function() return P().window_fg end) },
      ghost = ui.Rect { width = function() return W(t) end, height = function() return H(t) end, z = 30,
        radius = function() return h() / 2 end,
        translate_x = function() return t.delta_x or 0 end, translate_y = function() return t.delta_y or 0 end,
        scale = function() return t.active and 1.06 or 0.9 end,
        visible = function() return t.active end,
        color = function() return P().accent_bg end,
        shadow_color = function() return P().dark and "#00000099" or "#00000040" end, shadow_blur = 16,
        shadow_offset_y = 6,
        behavior = { scale = lift_spring() },
        chip_face(spec, function() return P().accent_fg end) },
      drop_indicator = ring(t, function() return h() / 2 end),
    }
  end

  --- GtkDropTarget: a rounded outline round an icon and a caption;
  --- while a drag it would take is over it, it fills with the accent, its
  --- outline turns the accent and the icon swells.
  S.drop_target = function(t, spec)
    return {
      handle = ui.Item {},
      background = ui.Rect { anchors = { fill = true }, radius = R.large,
        color = function() local p = P() return p.accent_bg:alpha(t.accepting and (p.dark and 0.22 or 0.12) or 0) end,
        border_width = 2,
        border_color = function() local p = P() return t.accepting and p.accent_bg or (p.strong and p.border or p.window_fg:alpha(0.22)) end,
        behavior = { color = quick(), border_color = quick() } },
      content = ui.Item { anchors = { fill = true },
        draw.icon(spec.icon or "upload", 32, function() local p = P() return t.accepting and p.accent or p.dim_fg end,
          { anchors = { horizontal_center = true }, y = function() return H(t) / 2 - 34 end,
            scale = function() return t.accepting and 1.3 or 1 end,
            translate_y = function() return t.accepting and -4 or 0 end,
            behavior = { scale = draw.spring(380, 22), translate_y = draw.spring(380, 22) } }),
        draw.text { anchors = { left = true, right = true }, y = function() return H(t) / 2 + 8 end,
          horizontal_alignment = "center", text = spec.label or "Drop here",
          font_weight = function() return t.accepting and 700 or 400 end,
          color = function() local p = P() return t.accepting and p.accent or p.dim_fg end } },
    }
  end

  --- Any other drag: a pane's divider.
  S.Drag = function(t, spec) return divider(t, spec) end
end
