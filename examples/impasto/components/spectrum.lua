-- A row of bars that follows the playing audio: Spectrum.qml.
--
-- `height`, `bar_width` (2), `gap` (bar_width), `minimum` (2), `color`,
-- `bars` (up to the service's eight), `active` (a function or a boolean;
-- false flattens the bars to their floor), `curve` (0.55). The service
-- reports linear amplitude; a fractional power keeps quiet passages
-- visible. Still bars when nothing plays.

local ui = require("morf.ui")
local theme = require("theme")
local spectrum = require("services.spectrum")

return function(values)
  local height = values.height or 14
  local bar = values.bar_width or 2
  local minimum = values.minimum or 2
  local curve = values.curve or 0.55
  local count = math.min(values.bars or 8, spectrum.BANDS)
  local active = values.active
  local function on()
    if active == nil then return true end
    if type(active) == "function" then return active() and true or false end
    return active and true or false
  end
  local row = { gap = values.gap or bar, align = "center", height = height }
  for index = 1, count do
    row[#row + 1] = ui.Rect {
      width = bar, radius = bar / 2,
      color = values.color or theme.color.indicator,
      height = function()
        if not on() then return minimum end
        local value = math.max(0, spectrum.bands[index]:get())
        return math.max(minimum, height * value ^ curve)
      end,
      -- Short enough to follow the beat, long enough to hide a dropped frame.
      behavior = { height = { duration = 70, easing = "out_quad" } },
    }
  end
  return ui.Row(row)
end
