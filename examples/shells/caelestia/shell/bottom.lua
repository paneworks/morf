-- The wide bottom workspace. Add future pages to TABS.
local morf = require("morf")
local theme = require("theme")
local tabbed = require("tabbed")
local drawer = require("drawer")
local M = {}
local screen = morf.screens and morf.screens[1] or { width = 1920, height = 1080 }
M.WIDTH = math.floor(math.min(2120, screen.width - theme.LEFT - theme.BORDER - 20) * 0.8)
local HEIGHT = math.floor(math.min(1252, screen.height - 2 * theme.BORDER - 40) * 0.6)
M.TABS = {
  { key = "assistant", name = "Assistant", icon = "auto_awesome", build = require("assistant").build },
  { key = "drop", name = "Drop", icon = "forum", build = require("drop").build },
}
function M.height() return HEIGHT end
M.panel = tabbed.new { id = "bottom", width = M.WIDTH, height = M.height, tabs = M.TABS }
M.drawer = drawer.new { name = "bottom", edge = "bottom", width = M.WIDTH, height = M.height, content = M.panel.content }
morf.effect("caelestia.bottom.shown", function() M.panel.shown(M.drawer.open:get()) end)
return M
