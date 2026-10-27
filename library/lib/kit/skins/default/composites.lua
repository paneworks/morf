-- The default kit's looks for what a composite adds over its parts: a
-- boxed-list row's press (lib.kit.composites.rows: a wash under the
-- pointer and the keyboard's ring, the row's own content over it).
local ui = require("morf.ui")

return function(S, theme, draw)
  local P = theme.P
  local R = theme.radius

  --- An activatable row: no ground until the pointer, then the wash.
  function S.action_row(t)
    return {
      background = ui.Rect { anchors = { fill = true }, radius = R.small,
        color = function()
          local p = P()
          if t.down then return p.window_fg:alpha(p.wash.active) end
          return t.hovered and p.window_fg:alpha(p.wash.hover) or p.window_fg:alpha(0)
        end, behavior = { color = draw.quick() } },
      indicator = draw.ring(t, R.small),
    }
  end
end
