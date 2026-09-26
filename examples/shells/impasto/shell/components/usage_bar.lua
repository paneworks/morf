-- A thin horizontal gauge: a track and how much of it has gone.
--
-- Port of UsageBar.qml. `progress` 0..1 and `fill_color` may be functions.
-- The fill is never narrower than it is tall, so its end stays round; at
-- zero it is gone.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")

local function read(v)
  if type(v) == "function" then return v() end
  return v
end

return function(values)
  local height = values.height or 6
  -- The fill reads the track's laid-out width, and the track is built
  -- after its child; this says when it exists, so the binding re-runs then.
  local built = kit.hover_signal("usage")
  local track
  local fill = ui.Rect {
    height = height,
    radius = height / 2,
    width = function()
      local p = math.max(0, math.min(1, read(values.progress) or 0))
      if p <= 0 or not built:get() then return 0 end
      return math.max(height, (track.layout_width or 0) * p)
    end,
    visible = function() return (read(values.progress) or 0) > 0 end,
    color = function() return read(values.fill_color) or theme.color.accent() end,
    behavior = { width = theme.behave("medium"), color = theme.behave("medium") },
  }
  track = ui.Rect {
    width = values.width,
    height = height,
    layout = values.layout,
    visible = values.visible,
    radius = height / 2,
    color = values.track_color or theme.color.islandSurfaceHover,
    fill,
  }
  built:set(true)
  return track
end
