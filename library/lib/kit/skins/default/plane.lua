-- The default kit's Plane skin: color_chooser's colour plane, saturation
-- across and value down of `spec.hue()`, with 6 px corners and a white
-- ring for the handle.
local morf = require("morf")
local ui = require("morf.ui")

return function(S, theme, draw)
  local R = theme.radius

  function S.Plane(t, spec)
    local W, Hh = spec.width or 160, spec.height or 160
    local function hue() return morf.color(("hsl(%d, 100%%, 50%%)"):format(math.floor(draw.get(spec.hue) or 0))) end
    return {
      track = ui.Item { width = W, height = Hh },
      field = ui.Item { width = W, height = Hh,
        ui.Rect { anchors = { fill = true }, radius = R.small,
          gradient = function() return { angle = 90, stops = { "#ffffff", hue() } } end },
        ui.Rect { anchors = { fill = true }, radius = R.small,
          gradient = { angle = 180, stops = { "#00000000", "#000000" } } } },
      handle = ui.Rect { width = 18, height = 18, radius = 9, color = "transparent", border_width = 3,
        border_color = "#ffffff",
        x = function() return t.visual_x * W - 9 end, y = function() return t.visual_y * Hh - 9 end },
      crosshair = draw.ring(t, R.small),
    }
  end
end
