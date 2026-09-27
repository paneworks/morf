-- Tasks and a daily agenda down the frame's left edge. Both pages share
-- planner.lua's Taskwarrior connection; the assistant lives at the bottom.
--
-- It opens over IPC (`leftbar`), and when the pointer reaches the left
-- edge above the workspace rail.

local morf = require("morf")
local theme = require("theme")
local drawer = require("drawer")
local tabbed = require("tabbed")

local M = {}

-- The pages, and the strip on the far side the workspace rail rides out to.
M.WIDTH = theme.SIDE_W + theme.STRIP

-- The desk's height: the screen, less the bar when it is up.
local function screen_height()
  local _, _, _, h = require("bar").desk()
  return h
end

function M.height() return screen_height() - 2 * theme.BORDER end

M.TABS = {
  { key = "tasks", name = "Tasks", icon = "checklist", build = require("tasks_page").build },
  { key = "calendar", name = "Calendar", icon = "calendar_month", build = require("calendar_page").build },
}

local panel = tabbed.new { id = "leftbar", width = theme.SIDE_W, height = M.height, tabs = M.TABS }
M.panel = panel

M.drawer = drawer.new {
  name = "leftbar",
  edge = "left",
  width = M.WIDTH,
  height = M.height,
  content = panel.content,
  props = { anchors = { top = true, left = true } },
}

morf.effect("caelestia.leftbar.bud", function()
  local open = M.drawer.open:get()
  panel.shown(open)
  -- Create polling outside the effect's lifetime, as the VPN pages do.
  morf.timer(1, function() require("planner").client.watch(open) end, false)
end)

return M
