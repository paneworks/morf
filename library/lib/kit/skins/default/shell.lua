-- The default kit's Shell skin, in the libadwaita manner: the window's
-- ground, a header in the header bar's tone over a hairline, a sidebar in
-- the sidebar tone with a hairline at its inner edge (a drawer's edge
-- carries the shade line instead), an inspector in the secondary
-- sidebar's tone, toolbars and a bottom bar in the header tone with their
-- rule toward the page. Each ground is the skin's slot of that region,
-- laid under the region by lib.kit.archetypes.shell so it moves with it.
-- The behaviour -- breakpoints, the drawer, F9, F6 -- is the archetype's.
local ui = require("morf.ui")

return function(S, theme, draw)
  local P = theme.P
  -- A hairline along one edge of the box it fills: `side` "start"/"end"
  -- (mirrored right to left), "top" or "bottom".
  local function rule(t, side, color)
    local function at_right() return (side == "end") ~= (t.mirrored == true) end
    if side == "top" or side == "bottom" then
      return ui.Rect { anchors = { left = true, right = true, [side] = true }, height = 1, color = color }
    end
    return ui.Item { anchors = { fill = true },
      ui.Rect { anchors = { right = true, top = true, bottom = true }, width = 1, color = color,
        visible = at_right },
      ui.Rect { anchors = { left = true, top = true, bottom = true }, width = 1, color = color,
        visible = function() return not at_right() end } }
  end

  --- The window's grounds; `o.content` a page ground of its own (the view
  --- tone).
  local function frame(o)
    o = o or {}
    return function(t)
      local function bar(edge)
        return ui.Item { anchors = { fill = true },
          ui.Rect { anchors = { fill = true }, color = function() return P().headerbar_bg end },
          rule(t, edge, function() return P().headerbar_shade end) }
      end
      return {
        background = ui.Rect { anchors = { fill = true }, color = function() return P().window_bg end },
        -- The window's edge, over its regions (in the toasts' layer, the
        -- topmost): a hairline, solid in high contrast.
        toasts = ui.Rect { anchors = { fill = true }, z = 60, color = "transparent", border_width = 1,
          border_color = function() local p = P() return p.strong and p.border or p.border:alpha(0.9) end },
        content = o.content and ui.Rect { anchors = { fill = true }, color = function() return P().view_bg end } or nil,
        header_bar = bar("bottom"),
        toolbar_top = bar("bottom"),
        toolbar_bottom = bar("top"),
        bottom_bar = bar("top"),
        sidebar = ui.Item { anchors = { fill = true },
          ui.Rect { anchors = { fill = true }, color = function() return P().sidebar_bg end },
          -- As a drawer the edge is the shade line: darker, it lifts the
          -- drawer off the page it covers.
          rule(t, "end", function()
            local p = P()
            return t.collapsed and p.sidebar_shade or p.sidebar_border
          end) },
        inspector = ui.Item { anchors = { fill = true },
          ui.Rect { anchors = { fill = true }, color = function() return P().secondary_sidebar_bg end },
          rule(t, "start", function() return P().secondary_sidebar_border end) },
      }
    end
  end

  S.Shell = frame()
  S.application_window = frame()
  S.toolbar_view = frame()
  S.breakpoint_bin = frame()
  S.clamp = frame()
  -- A split view's page is the view tone beside the sidebar's.
  S.navigation_split_view = frame { content = true }
  S.overlay_split_view = frame { content = true }
  S.multi_layout_view = frame { content = true }
end
