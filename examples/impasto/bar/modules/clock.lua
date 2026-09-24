-- The time, which is what the island shows at rest.
--
-- Port of ClockModule.qml: the time in the UI face, semibold, with the
-- date beside it when Settings asks for it.

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local kit = require("components.kit")

local clock = {}

local function format()
  local pattern = settings.clockFormat
  if settings.clockShowsSeconds then pattern = pattern:gsub("%%M", "%%M:%%S", 1) end
  return pattern
end

--- The clock text, following the second or the minute.
function clock.text()
  morf.clock:get()
  return morf.time.format(format())
end

function clock.build()
  -- One text, the date appended when shown, so the time and the date are
  -- centred as one line.
  return ui.Item { anchors = { fill = true },
    kit.text {
      anchors = { center_in = true },
      text = function()
        local text = clock.text()
        if settings.clockShowsDate then text = text .. "   " .. morf.time.format("%a %-d %b") end
        return text
      end,
      size = theme.size.regular, weight = 600,
    },
  }
end

return clock
