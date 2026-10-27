-- The default kit's Scroll skin: a GtkScrolledWindow's overlay scroll bar
-- along its right edge while there is somewhere to scroll, and with the
-- `undershoot_top` and `undershoot_bottom` classes a shade along the edge
-- the content goes on past.
local ui = require("morf.ui")

return function(S, theme, draw)
  local P = theme.P
  local has = require("lib.kit.archetypes.press").has

  local function undershoot(t, top)
    return ui.Rect { anchors = top and { left = true, right = true, top = true } or { left = true, right = true, bottom = true },
      height = 12, z = 5,
      gradient = function()
        local c = P().shade
        return { angle = top and 180 or 0, stops = { c:alpha(P().dark and 0.6 or 0.4), c:alpha(0) } }
      end,
      visible = function() if top then return not t.at_start end return not t.at_end end }
  end

  function S.Scroll(t, spec)
    local flick = spec.flick
    local function room() return math.max(0, t.content_height - t.viewport_height) end
    local slots = {
      scroll_bar_y = require("lib.kit.archetypes.range").make("scrollbar", { width = 10, orientation = "vertical",
        inverted = true, accessible_name = "Scroll position",
        anchors = { right = true, top = true, bottom = true, right_margin = 2, top_margin = 4, bottom_margin = 4 },
        visible = function() return t.bar_y end,
        value = function() return t.position_y end,
        size = function() return t.size_y end,
        handle_size = function() return math.max(24, t.size_y * t.viewport_height) end,
        on_moved = function(v) if flick then flick.content_y = v * room() end end }),
    }
    local top, bottom = has(spec, "undershoot_top"), has(spec, "undershoot_bottom")
    if top or bottom then
      slots.edge_fade = ui.Item { anchors = { fill = true }, z = 5,
        top and undershoot(t, true) or nil, bottom and undershoot(t, false) or nil }
    end
    return slots
  end
end
