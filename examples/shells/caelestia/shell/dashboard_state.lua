-- What the dashboard's tabs share: which tab is chosen, whether the drawer
-- is open, and the pointer areas on the panel (the panel counts as hovered
-- while any of them is; see NEEDS.md, "hover that contains its children").
-- Each tab's page is its own module, built as it loads, so each gets its
-- own instruction budget.

local morf = require("morf")
local ui = require("morf.ui")

local M = {}

M.tab = morf.signal("caelestia.dashboard.tab", 1)
M.opened = morf.signal("caelestia.dashboard.shown", false)

M.areas = {}

--- A MouseArea the panel counts as its own for hover.
function M.area(props)
  local a = ui.MouseArea(props)
  M.areas[#M.areas + 1] = a
  return a
end

--- What page `i` is given: whether it is on screen (the drawer open and its
--- tab chosen -- the pages read their services only then), whether its tab
--- is chosen, and `area`.
function M.context(i)
  return {
    opened = function() return M.opened:get() and M.tab:get() == i end,
    current = function() return M.tab:get() == i end,
    area = M.area,
  }
end

return M
