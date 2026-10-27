-- The default kit's Dock skin, in the Adwaita manner: stacks on the view's
-- ground under a strip of flat tabs whose current one is raised, hairline
-- dividers that turn blue and thicken under the pointer, floating panels as
-- raised cards, a tinted plate where a dragged tab would land. Each widget
-- adds what it is: documents as a tab bar whose chosen tab is a raised card
-- (GNOME's tab bar), tool windows with flat tabs underlined in the accent
-- on the sidebar's shade, shelves as rounded cards apart on the window's
-- ground with pill tabs, a tabbed container framed as one card.
local ui = require("morf.ui")

return function(S, theme, draw)
  local P = theme.P
  local R = theme.radius
  local get, quick = draw.get, draw.quick

  --- A dock's tab: flat, the current one raised to the view's ground with
  --- a line under it (the accent while its stack has focus) that grows out
  --- from the middle as it is chosen. `look`: `top` (margin), `radius`,
  --- `current()` (the chosen tab's ground), `underline` (false: none;
  --- "always": also in a stack without focus).
  function draw.dock_tab(s, look)
    look = look or {}
    local close_area
    local tab = ui.Rect { anchors = { fill = true, left_margin = 2, right_margin = 2, top_margin = look.top or 4 },
      radius = look.radius or R.small,
      color = function()
        local p = P()
        if s.current() then return look.current and look.current() or p.view_bg end
        if s.hovered() then return p.window_fg:alpha(p.wash.hover) end
        return p.window_fg:alpha(0)
      end,
      border_width = function() local p = P() return (p.strong and s.current()) and 1 or 0 end,
      border_color = function() return P().border end, behavior = { color = quick() } }
    local row = ui.Row { anchors = { fill = true, left_margin = 10, right_margin = 6 }, gap = 6, align = "center" }
    if s.icon then
      ui.reparent(draw.icon(s.icon, 16, function() local p = P() return s.current() and p.window_fg or p.dim_fg end), row)
    end
    ui.reparent(draw.text { text = s.title, font_size = theme.size.small,
      -- (A tab is made anew as its stack's current changes: read once.)
      font_weight = s.current() and 700 or 400,
      color = function() local p = P() return s.current() and p.window_fg or p.dim_fg end, elide = "right",
      width = function() return math.max(20, (tab.layout_width or 100) - (s.icon and 64 or 42)) end,
      -- (A narrow tab with an icon shows the icon alone.)
      visible = function() return not s.icon or (tab.layout_width or 0) >= 104 end }, row)
    ui.reparent(row, tab)
    if s.closable then
      close_area = ui.MouseArea { anchors = { right = true, vertical_center = true, right_margin = 4 },
        width = 22, height = 22, cursor = "pointer", accessible_role = "button", accessible_name = "Close " .. s.title,
        visible = function() return (s.current() or s.hovered()) and (tab.layout_width or 0) >= 76 end,
        on_clicked = function() s.close() end,
        ui.Rect { anchors = { fill = true }, radius = math.min(11, R.large),
          color = function() local p = P() return close_area and close_area.hovered and p.window_fg:alpha(p.wash.hover) or p.window_fg:alpha(0) end },
        draw.icon("close", 14, function() return P().dim_fg end, { anchors = { center_in = true } }) }
      ui.reparent(close_area, tab)
    end
    if look.underline ~= false then
      local function full() return math.max(0, (tab.layout_width or 0) - 16) end
      ui.reparent(ui.Rect { anchors = { bottom = true, horizontal_center = true }, height = 2, radius = 1,
        width = full, enter = { width = 0 }, behavior = { width = draw.spring(380, 30) },
        color = function() local p = P() return s.focused() and p.accent_bg or p.window_fg:alpha(0.3) end,
        visible = function() return s.current() and (look.underline == "always" or s.focused()) end }, tab)
    end
    return tab
  end

  --- A plate where a dragged tab would land, springing from zone to zone:
  --- `spec` (the dock's), `props` for the plate (a Rect's), `inset` (4).
  function draw.dock_drop(spec, props, inset)
    inset = inset or 4
    props.x = function() local x = spec.drop_box() return x + inset end
    props.y = function() local _, y = spec.drop_box() return y + inset end
    props.width = function() local _, _, w = spec.drop_box() return math.max(0, w - inset * 2) end
    props.height = function() local _, _, _, h = spec.drop_box() return math.max(0, h - inset * 2) end
    local motion = props.motion or draw.spring(520, 40)
    props.motion = nil
    props.behavior = props.behavior or {}
    for _, k in ipairs { "x", "y", "width", "height" } do props.behavior[k] = motion end
    return ui.Item { anchors = { fill = true }, ui.Rect(props) }
  end

  --- A dock: stacks on the view's ground under a strip of flat tabs,
  --- hairline dividers that turn blue and thicken under the pointer,
  --- floating panels as raised cards, a tinted plate where a dragged tab
  --- would land.
  function S.Dock(t, spec)
    local TAB = spec.tab_height or 34
    return {
      background = ui.Rect { anchors = { fill = true }, color = function() return P().window_bg end },
      tab = function(s) return draw.dock_tab(s) end,
      stack = function(s)
        return ui.Item { anchors = { fill = true },
          ui.Rect { anchors = { fill = true }, color = function() return P().sidebar_bg end },
          ui.Rect { anchors = { left = true, right = true, bottom = true, top = true, top_margin = TAB },
            color = function() return P().view_bg end },
          ui.Rect { y = TAB - 1, height = 1, anchors = { left = true, right = true }, color = function() return P().border end } }
      end,
      divider = function(s)
        local vertical = s.orientation == "vertical"
        local function on() return s.hovered() or s.dragging() end
        return ui.Rect { anchors = vertical and { left = true, right = true, vertical_center = true }
            or { top = true, bottom = true, horizontal_center = true },
          width = function() return (on() and 3 or 1) end,
          height = function() return (on() and 3 or 1) end,
          color = function() local p = P() return on() and p.accent_bg or p.border end,
          behavior = { color = quick() } }
      end,
      floating = function(s)
        return ui.Rect { anchors = { fill = true }, radius = R.large, color = function() return P().popover_bg end,
          border_width = 1, border_color = function() return P().border end, clip = true,
          ui.Rect { anchors = { left = true, right = true }, height = TAB, color = function() return P().headerbar_bg end },
          draw.text { x = 12, height = TAB, text = s.title, font_size = theme.size.small, font_weight = 700,
            color = function() return P().window_fg end, vertical_alignment = "center" } }
      end,
      drop_indicator = draw.dock_drop(spec, { radius = R.medium,
        color = function() return P().accent_bg:alpha(0.18) end,
        border_width = 2, border_color = function() return P().accent_bg end }),
    }
  end
  --- Documents: the strip in the header's tone, each tab a pill, the
  --- chosen one raised to the view with a hairline and a shadow; no
  --- underline -- the raised card says it.
  function S.document_tabs(t, spec)
    local TAB = spec.tab_height or 34
    return {
      tab = function(s)
        local tab = draw.dock_tab(s, { top = 4, radius = R.medium, underline = false,
          current = function() return P().view_bg end })
        ui.reparent(ui.Rect { anchors = { fill = true, top_margin = 1, bottom_margin = -1 }, z = -1, radius = R.medium,
          color = function() local p = P() return p.shade:alpha(p.dark and 0.6 or 0.5) end,
          visible = function() return s.current() end }, tab)
        return tab
      end,
      stack = function()
        return ui.Item { anchors = { fill = true },
          ui.Rect { anchors = { fill = true }, color = function() return P().headerbar_bg end },
          ui.Rect { anchors = { fill = true, top_margin = TAB }, color = function() return P().view_bg end },
          ui.Rect { y = TAB - 1, height = 1, anchors = { left = true, right = true }, color = function() return P().border end } }
      end,
    }
  end

  --- Tool windows: flat tabs on the sidebar's shade, the chosen one's
  --- name in the accent with its line always shown (dimmer while the
  --- stack has no focus), dividers a touch heavier.
  function S.tool_windows(t, spec)
    local TAB = spec.tab_height or 34
    return {
      tab = function(s)
        return draw.dock_tab(s, { top = 0, radius = 0, underline = "always",
          current = function() local p = P() return p.window_fg:alpha(p.wash.selected * 0.6) end })
      end,
      stack = function(s)
        return ui.Item { anchors = { fill = true },
          ui.Rect { anchors = { fill = true }, color = function() return P().sidebar_bg end },
          ui.Rect { anchors = { fill = true, top_margin = TAB }, color = function() return P().view_bg end },
          ui.Rect { y = TAB - 1, height = 1, anchors = { left = true, right = true }, color = function() return P().border end },
          -- The focused tool window: a line of the accent down its edge.
          ui.Rect { anchors = { left = true, top = true, bottom = true }, width = 2,
            color = function() return P().accent_bg end, opacity = function() return s.focused() and 1 or 0 end,
            behavior = { opacity = quick() } } }
      end,
    }
  end

  --- Shelves: each stack a rounded card a few pixels apart on the
  --- window's ground (the dividers are the gaps), tabs as pills inside its
  --- header, the chosen one filled with the selected wash.
  function S.shelf_dock(t, spec)
    local TAB = spec.tab_height or 34
    return {
      background = ui.Rect { anchors = { fill = true }, color = function() return P().window_bg end },
      tab = function(s)
        return draw.dock_tab(s, { top = 5, radius = 14, underline = false,
          current = function() local p = P() return p.window_fg:alpha(p.wash.selected + 0.04) end })
      end,
      stack = function(s)
        return ui.Rect { anchors = { fill = true, margins = 2 }, radius = R.large, clip = true,
          color = function() return P().view_bg end, border_width = 1,
          border_color = function() local p = P() return s.focused() and p.accent_bg:alpha(0.6) or p.border end,
          behavior = { border_color = quick() },
          ui.Rect { anchors = { left = true, right = true, top = true }, height = TAB,
            color = function() return P().sidebar_bg end } }
      end,
      divider = function(s)
        local vertical = s.orientation == "vertical"
        local function on() return s.hovered() or s.dragging() end
        return ui.Rect { anchors = { center_in = true }, radius = 2,
          width = function() return vertical and 32 or 4 end, height = function() return vertical and 4 or 32 end,
          color = function() local p = P() return on() and p.accent_bg or p.window_fg:alpha(0) end,
          behavior = { color = quick() } }
      end,
      drop_indicator = draw.dock_drop(spec, { radius = R.large,
        color = function() return P().accent_bg:alpha(0.16) end,
        border_width = 2, border_color = function() return P().accent_bg end }, 6),
    }
  end

  --- A tabbed container: one card with a hairline round it, its tabs flat
  --- with the accent line under the chosen one whether or not it has
  --- focus.
  function S.tabbed_container(t, spec)
    local TAB = spec.tab_height or 34
    return {
      tab = function(s)
        return draw.dock_tab(s, { top = 0, radius = 0, underline = "always", current = function() return P().view_bg:alpha(0) end })
      end,
      stack = function()
        return ui.Rect { anchors = { fill = true }, radius = R.large, clip = true, color = function() return P().view_bg end,
          border_width = 1, border_color = function() return P().border end,
          ui.Rect { y = TAB - 1, height = 1, anchors = { left = true, right = true }, color = function() return P().border end } }
      end,
      background = ui.Rect { anchors = { fill = true }, color = function() return P().window_bg end },
    }
  end
end
