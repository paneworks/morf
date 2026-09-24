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

--- The time, and beside it the date when Settings asks for it, smaller
--- and muted (ClockModule.qml:46-67): a row to centre wherever it goes.
function clock.face()
  return ui.Row {
    gap = 8, align = "center",
    kit.text { text = clock.text, size = theme.size.regular, weight = 600 },
    kit.text {
      visible = function() return settings.clockShowsDate end,
      text = function()
        morf.clock:get()
        return morf.time.format("%a %-d %b")
      end,
      size = theme.size.small, color = theme.color.textMuted,
    },
  }
end

function clock.build()
  local face = clock.face()
  return ui.Item { anchors = { fill = true },
    ui.Item {
      anchors = { center_in = true },
      width = function() return face.layout_width or 0 end,
      height = function() return face.layout_height or 16 end,
      face,
    },
  }
end

return clock
