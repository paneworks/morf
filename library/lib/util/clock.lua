-- The time of day as text, read in a binding: `clock.format("%H:%M")`
-- wakes the binding once a minute, a pattern that shows seconds every
-- second, and one with neither (a weekday, a date) once an hour.
--
--     local clock = require("lib.util.clock")
--     ui.Text { text = function() return clock.format("%a %H:%M") end }
local M = {}

local SECONDS = "%%[STrsXc]"
local MINUTES = "%%[MRl]"

--- `morf.time.format(pattern)`, following the clock at the grain it shows.
function M.format(pattern)
  if pattern:find(SECONDS) then
    morf.time.clock:get()
  elseif pattern:find(MINUTES) then
    morf.time.minute_clock:get()
  else
    morf.time.hour_clock:get()
  end
  return morf.time.format(pattern)
end

return M
