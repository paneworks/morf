-- Gallery samples for the Selection widgets: switchers between views and
-- pages, and a carousel's indicators.
local ui = require("morf.ui")

local M = {}

local VIEWS = { { label = "Albums", icon = "photo_album" }, { label = "Artists", icon = "person" },
  { label = "Songs", icon = "music_note" } }

function M.toggle_group(_, w)
  return w.toggle_group { items = { "Day", "Week", "Month" }, current = 2, item_width = 80 }
end
function M.view_switcher(_, w) return w.view_switcher { items = VIEWS, current = 1, item_width = 90 } end
function M.inline_view_switcher(_, w) return w.inline_view_switcher { items = VIEWS, current = 2, item_width = 90 } end
function M.view_switcher_bar(_, w) return w.view_switcher_bar { items = VIEWS, current = 3, item_width = 90 } end
function M.stack_switcher(_, w) return w.stack_switcher { items = { "General", "Sound", "Display" }, current = 1, item_width = 90 } end
function M.carousel_indicator_dots(_, w)
  return w.carousel_indicator_dots { items = { "1", "2", "3", "4", "5" }, current = 2 }
end
function M.carousel_indicator_lines(_, w)
  return w.carousel_indicator_lines { items = { "1", "2", "3", "4", "5" }, current = 4 }
end

return M
