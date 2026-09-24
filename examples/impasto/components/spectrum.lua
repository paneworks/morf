-- A row of bars that follows the playing audio: Spectrum.qml.
--
-- `height`, `bar_width` (2), `gap` (bar_width), `minimum` (2), `color`,
-- `bars` (up to the service's eight). Still bars when nothing plays.

local ui = require("morf.ui")
local theme = require("theme")
local spectrum = require("services.spectrum")

return function(values)
  local height = values.height or 14
  local bar = values.bar_width or 2
  local minimum = values.minimum or 2
  local count = math.min(values.bars or 8, spectrum.BANDS)
  local row = { gap = values.gap or bar, align = "center", height = height }
  for index = 1, count do
    row[#row + 1] = ui.Rect {
      width = bar, radius = bar / 2,
      color = values.color or theme.color.indicator,
      height = function()
        return math.max(minimum, height * spectrum.bands[index]:get())
      end,
      behavior = { height = { duration = 90, easing = "out_quad" } },
    }
  end
  return ui.Row(row)
end
