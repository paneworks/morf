-- The default kit's looks for the Scroll widgets, in the Adwaita manner: a
-- scrolled view's undershoot hairlines, a scroll area's second bar, a
-- carousel's indicator dots, a shelf's edge fades and round raised arrows,
-- and an infinite list's spinner row. The scrolling, snapping and loading
-- are lib.kit.scroll's and the Scroll archetype's.
local ui = require("morf.ui")
local control = require("lib.kit.control")

return function(S, theme, M)
  local P = theme.P
  local function quick() return { duration = theme.duration.small, easing = theme.ease.standard } end
  local function slide() return M.spring(420, 40) end

  -- Shared geometry (every look places these alike).
  local DOT, PITCH, DOTS_H, DOTS_GAP = 8, 16, 20, 6
  local MAX_DOTS = 12
  local ARROW, ARROW_GAP = 32, 8
  local FADE = 36
  local LOADING_H, CHIP = 48, 36

  local function room_x(t) return math.max(0, (t.content_width or 0) - (t.viewport_width or 0)) end
  local function pages(t)
    local vw = t.viewport_width or 0
    if vw <= 0 then return 1 end
    return math.max(1, math.min(MAX_DOTS, math.floor((t.content_width or 0) / vw + 0.5)))
  end
  local function page_at(t)
    local vw = t.viewport_width or 0
    if vw <= 0 then return 0 end
    return (t.content_x or 0) / vw
  end

  --- A press a scroll skin hangs on a part of itself (a page dot, an
  --- arrow): its look is `S[widget]`, its behaviour the Press archetype's.
  local function part(widget, props, on_clicked, settings)
    local spec = { widget = widget, on_clicked = on_clicked }
    for k, v in pairs(settings or {}) do spec[k] = v end
    return (control.make("Press", widget, spec, { props = props }))
  end
  local function blank() return ui.Item {} end

  -- ------------------------------------------------------------ pages --

  --- A carousel's dot: the ink, brighter the nearer its page is.
  function S.page_dot(t, spec)
    local i = spec.index
    local view = spec.view
    return {
      background = ui.Rect { x = (PITCH - DOT) / 2, y = (DOTS_H - DOT) / 2, width = DOT, height = DOT, radius = DOT / 2,
        color = function()
          local p = P()
          local near = math.max(0, 1 - math.abs(page_at(view) - (i - 1)))
          local a = (p.strong and 0.45 or 0.3) + (p.strong and 0.55 or 0.6) * near
          if t.hovered then a = math.min(1, a + 0.15) end
          return p.ink:alpha(a)
        end,
        scale = function() return t.down and 0.8 or 1 end,
        behavior = { scale = quick() } },
      content = blank(),
      indicator = ui.Rect { x = (PITCH - DOT) / 2 - 3, y = (DOTS_H - DOT) / 2 - 3, width = DOT + 6, height = DOT + 6,
        radius = DOT / 2 + 3, z = 50, color = "transparent", border_width = 2,
        border_color = function() local p = P() return p.strong and p.focus or p.focus:alpha(0.6) end,
        visible = function() return t.visual_focus end },
      icon = blank(), label = blank(), badge = blank(),
    }
  end

  local function dots(t, spec, look)
    local row = ui.Item { id = spec.id and (spec.id .. "-dots") or nil,
      width = function() return pages(t) * PITCH end, height = DOTS_H,
      x = function() return math.floor(((t.width or 0) - pages(t) * PITCH) / 2) end,
      y = function() return (t.height or 0) - DOTS_H - DOTS_GAP end,
      visible = function() return pages(t) > 1 end }
    for i = 1, MAX_DOTS do
      ui.reparent(part(look, { x = (i - 1) * PITCH, width = PITCH, height = DOTS_H, cursor = "pointer",
        accessible_name = "Page " .. i, visible = function() return i <= pages(t) end },
        function() if spec.glide then spec.glide((i - 1) * (t.viewport_width or 0), nil) end end,
        { index = i, view = t }), row)
    end
    return row
  end

  --- A pager: its pages slide sideways; the dots under them say which.
  function S.pager(t, spec)
    return { scroll_bar_x = dots(t, spec, "page_dot") }
  end

  -- ------------------------------------------------------------ shelf --

  --- A shelf's arrow: a round raised button, the card's tone with a
  --- shadow, the chevron in the ink.
  function S.shelf_arrow(t, spec)
    return {
      background = ui.Rect { anchors = { fill = true }, radius = ARROW / 2,
        color = function()
          local p = P()
          if t.down then return p.card:mix(p.ink, p.wash.active) end
          if t.hovered then return p.card:mix(p.ink, p.wash.hover) end
          return p.card
        end,
        border_width = function() return P().strong and 1 or 0 end, border_color = function() return P().border end,
        shadow_color = function() local p = P() return p.shade:alpha(p.dark and 0.6 or 0.9) end,
        shadow_blur = 6, shadow_offset_y = 1,
        behavior = { color = quick() } },
      content = M.icon(spec.icon, 20, function() return P().ink end, { anchors = { center_in = true } }),
      indicator = ui.Rect { anchors = { fill = true }, radius = ARROW / 2, color = "transparent", z = 50,
        border_width = 2, border_color = function() local p = P() return p.strong and p.focus or p.focus:alpha(0.6) end,
        visible = function() return t.visual_focus end },
      icon = blank(), label = blank(), badge = blank(),
    }
  end

  -- The shelf's ends fade out where there is more to scroll to: a mask on
  -- its view (only the alpha counts, so it is drawn in black), its edges
  -- solid while that end is reached and a ramp to nothing otherwise --
  -- whatever ground the shelf sits on.
  local function edge_mask(shown_before, shown_after)
    local function edge(side, shown)
      return ui.Item { anchors = { top = true, bottom = true, [side] = true }, width = FADE,
        rotation = side == "right" and 180 or 0,
        ui.Rect { anchors = { fill = true }, gradient = { angle = 90, stops = { "#00000000", "#000000" } } },
        ui.Rect { anchors = { fill = true }, color = "#000000",
          opacity = function() return shown() and 0 or 1 end, behavior = { opacity = quick() } } }
    end
    return ui.Item {
      ui.Rect { anchors = { fill = true, left_margin = FADE, right_margin = FADE }, color = "#000000" },
      edge("left", shown_before), edge("right", shown_after) }
  end

  --- The shelf's ends: a fade and an arrow at each side there is more to
  --- scroll to; the arrows move a page, landing on an item.
  local function shelf_edges(t, spec, arrow_look)
    local function more_before() return (t.content_x or 0) > 0.5 end
    local function more_after() return (t.content_x or 0) < room_x(t) - 0.5 end
    local function go(dir)
      local item = (type(spec.item_size) == "number" and spec.item_size > 0) and spec.item_size or 120
      local vw = t.viewport_width or 0
      local step = math.max(item, math.floor(vw * 0.8 / item) * item)
      local to = math.floor(((t.content_x or 0) + dir * step) / item + 0.5) * item
      if spec.glide then spec.glide(math.max(0, math.min(room_x(t), to)), nil) end
    end
    local function arrow(side, icon, shown, dir)
      return part(arrow_look, { width = ARROW, height = ARROW, cursor = "pointer",
        accessible_name = dir < 0 and "Scroll back" or "Scroll on",
        anchors = { [side] = true, vertical_center = true, left_margin = ARROW_GAP, right_margin = ARROW_GAP },
        opacity = function() return shown() and 1 or 0 end,
        visible = function() return shown() end,
        scale = function() return shown() and 1 or 0.6 end,
        behavior = { opacity = quick(), scale = slide() } },
        function() go(dir) end, { icon = icon })
    end
    if spec.flick then spec.flick.mask = edge_mask(more_before, more_after) end
    return {
      overscroll = ui.Item { anchors = { fill = true },
        arrow("left", "chevron_left", more_before, -1), arrow("right", "chevron_right", more_after, 1) },
    }
  end

  function S.shelf(t, spec) return shelf_edges(t, spec, "shelf_arrow") end

  -- ----------------------------------------------------- scroll views --

  --- Libadwaita's undershoot: a hairline at an edge the content runs on
  --- past.
  function S.scroll_view(t)
    local function line(edge, shown)
      return ui.Rect { anchors = { left = true, right = true, [edge] = true }, height = 1,
        color = function() return P().border end,
        opacity = function() return shown() and 1 or 0 end, behavior = { opacity = quick() } }
    end
    local function room_y() return (t.content_height or 0) - (t.viewport_height or 0) end
    return { edge_fade = ui.Item { anchors = { fill = true },
      line("top", function() return room_y() > 0.5 and (t.content_y or 0) > 0.5 end),
      line("bottom", function() return room_y() > 0.5 and (t.content_y or 0) < room_y() - 0.5 end) } }
  end

  --- A scroll area's sideways bar: the vertical one turned, a slim pill
  --- along the bottom, wider under the pointer.
  function S.scroll_bar_x(t, spec)
    local function length() return math.max(24, ((type(spec.size) == "function" and spec.size()) or 1) * (t.width or 0)) end
    return {
      track = ui.Item { anchors = { fill = true } },
      handle = ui.Rect { anchors = { bottom = true }, radius = 4,
        height = function() return (t.hovered or t.down) and 8 or 4 end,
        width = length,
        x = function() return (t.visual_position or 0) * ((t.width or 0) - length()) end,
        color = function() local p = P() return p.ink:alpha((t.hovered or t.down) and 0.5 or (p.strong and 0.7 or 0.3)) end,
        behavior = { height = quick() } },
      fill = blank(), second_handle = blank(), ticks = blank(), value_label = blank(), increase = blank(),
      decrease = blank(), background = blank(), content = blank(),
    }
  end

  local function scroll_bar_x(t, spec)
    local flick = spec.flick
    return control.make("Range", "scroll_bar_x", { widget = "scroll_bar_x", orientation = "horizontal", wheel = false,
      accessible_name = "Scroll position", height = 10,
      anchors = { left = true, right = true, bottom = true, left_margin = 4, right_margin = 14, bottom_margin = 2 },
      visible = function() return t.bar_x end,
      value = function() return t.position_x end,
      size = function() return t.size_x end,
      on_moved = function(v) if flick then flick.content_x = v * room_x(t) end end })
  end

  --- A scroll area: bars along both edges while there is somewhere to go.
  function S.scroll_area(t, spec)
    return { scroll_bar_x = scroll_bar_x(t, spec) }
  end

  --- An infinite list: once it reaches its end, a raised round chip
  --- rises at its foot with the spinner turning while more is fetched.
  function S.infinite_scroll(t, spec)
    return { overscroll = ui.Item { anchors = { left = true, right = true, bottom = true }, height = LOADING_H,
      visible = function() return t.loading end,
      ui.Rect { anchors = { horizontal_center = true }, y = (LOADING_H - CHIP) / 2, width = CHIP, height = CHIP,
        radius = CHIP / 2, color = function() return P().card end,
        border_width = function() return P().strong and 1 or 0 end, border_color = function() return P().border end,
        shadow_color = function() local p = P() return p.shade:alpha(p.dark and 0.7 or 1) end, shadow_blur = 8,
        shadow_offset_y = 1,
        opacity = function() return t.loading and 1 or 0 end,
        translate_y = function() return t.loading and 0 or LOADING_H / 2 end,
        scale = function() return t.loading and 1 or 0.6 end,
        behavior = { opacity = quick(), translate_y = slide(), scale = slide() },
        M.loading(20, function() return P().accent end,
          { anchors = { center_in = true }, active = function() return t.loading end, accessible_name = "Loading more" }) } } }
  end
end
