-- The default kit's looks for each Dock widget, over the archetype's skin
-- (canvas_dock.lua), in the Adwaita manner: documents as a tab bar whose
-- chosen tab is a raised card (GNOME's tab bar), tool windows with flat
-- tabs underlined in the accent on the sidebar's shade, shelves as
-- rounded cards apart on the window's ground with pill tabs, a tabbed
-- container framed as one card. Only what differs is given.
local ui = require("morf.ui")

return function(S, theme, M)
  local P = theme.P
  local R = theme.radius
  local function quick() return { duration = theme.duration.small, easing = theme.ease.standard } end

  --- Documents: the strip in the header's tone, each tab a pill, the
  --- chosen one raised to the view with a hairline and a shadow; no
  --- underline -- the raised card says it.
  function S.document_tabs(t, spec)
    local TAB = spec.tab_height or 34
    return {
      tab = function(s)
        local tab = M.dock_tab(s, { top = 4, radius = R.medium, underline = false,
          current = function() return P().view end })
        ui.reparent(ui.Rect { anchors = { fill = true, top_margin = 1, bottom_margin = -1 }, z = -1, radius = R.medium,
          color = function() local p = P() return p.shade:alpha(p.dark and 0.6 or 0.5) end,
          visible = function() return s.current() end }, tab)
        return tab
      end,
      stack = function()
        return ui.Item { anchors = { fill = true },
          ui.Rect { anchors = { fill = true }, color = function() return P().header end },
          ui.Rect { anchors = { fill = true, top_margin = TAB }, color = function() return P().view end },
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
        return M.dock_tab(s, { top = 0, radius = 0, underline = "always",
          current = function() local p = P() return p.ink:alpha(p.wash.selected * 0.6) end })
      end,
      stack = function(s)
        return ui.Item { anchors = { fill = true },
          ui.Rect { anchors = { fill = true }, color = function() return P().sidebar end },
          ui.Rect { anchors = { fill = true, top_margin = TAB }, color = function() return P().view end },
          ui.Rect { y = TAB - 1, height = 1, anchors = { left = true, right = true }, color = function() return P().border end },
          -- The focused tool window: a line of the accent down its edge.
          ui.Rect { anchors = { left = true, top = true, bottom = true }, width = 2,
            color = function() return P().accent end, opacity = function() return s.focused() and 1 or 0 end,
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
      background = ui.Rect { anchors = { fill = true }, color = function() return P().window end },
      tab = function(s)
        return M.dock_tab(s, { top = 5, radius = 14, underline = false,
          current = function() local p = P() return p.ink:alpha(p.wash.selected + 0.04) end })
      end,
      stack = function(s)
        return ui.Rect { anchors = { fill = true, margins = 2 }, radius = R.large, clip = true,
          color = function() return P().view end, border_width = 1,
          border_color = function() local p = P() return s.focused() and p.accent:alpha(0.6) or p.border end,
          behavior = { border_color = quick() },
          ui.Rect { anchors = { left = true, right = true, top = true }, height = TAB,
            color = function() return P().sidebar end } }
      end,
      divider = function(s)
        local vertical = s.orientation == "vertical"
        local function on() return s.hovered() or s.dragging() end
        return ui.Rect { anchors = { center_in = true }, radius = 2,
          width = function() return vertical and 32 or 4 end, height = function() return vertical and 4 or 32 end,
          color = function() local p = P() return on() and p.accent or p.ink:alpha(0) end,
          behavior = { color = quick() } }
      end,
      drop_indicator = M.dock_drop(spec, { radius = R.large,
        color = function() return P().accent:alpha(0.16) end,
        border_width = 2, border_color = function() return P().accent end }, 6),
    }
  end

  --- A tabbed container: one card with a hairline round it, its tabs flat
  --- with the accent line under the chosen one whether or not it has
  --- focus.
  function S.tabbed_container(t, spec)
    local TAB = spec.tab_height or 34
    return {
      tab = function(s)
        return M.dock_tab(s, { top = 0, radius = 0, underline = "always", current = function() return P().view:alpha(0) end })
      end,
      stack = function()
        return ui.Rect { anchors = { fill = true }, radius = R.large, clip = true, color = function() return P().view end,
          border_width = 1, border_color = function() return P().border end,
          ui.Rect { y = TAB - 1, height = 1, anchors = { left = true, right = true }, color = function() return P().border end } }
      end,
      background = ui.Rect { anchors = { fill = true }, color = function() return P().window end },
    }
  end
end
