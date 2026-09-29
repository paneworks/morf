-- Presentation helpers shared by notification views.
local morf = require("morf")
local M = {}
function M.plain(value)
  return (tostring(value or ""):gsub("<[^>]->", ""):gsub("&lt;", "<"):gsub("&gt;", ">"):gsub("&amp;", "&"))
end
function M.ago(time)
  morf.minute_clock:get()
  local seconds = math.max(0, morf.time.now() - (time or 0))
  if seconds < 60 then return "now" end
  if seconds < 3600 then return ("%dm"):format(seconds // 60) end
  if seconds < 86400 then return ("%dh"):format(seconds // 3600) end
  return ("%dd"):format(seconds // 86400)
end
return M
