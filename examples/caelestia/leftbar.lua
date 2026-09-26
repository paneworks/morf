-- The left panel: the sidebar's twin down the frame's left edge, the same
-- tabbed shell (tabbed.lua), the frame's full height. It holds one page
-- for now, a place for what comes (an assistant, say); a tab is one more
-- entry in `TABS`.
--
-- It opens over IPC (`leftbar`), and when the pointer reaches the left
-- edge above the workspace rail.

local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local drawer = require("drawer")
local tabbed = require("tabbed")

local C = theme.color
local M = {}

-- The pages, and the strip on the far side the workspace rail rides out to.
M.WIDTH = theme.SIDE_W + theme.STRIP

local function screen_height()
  morf.screens_revision()
  local s = morf.screens[1]
  return (s and s.height) or 1080
end

function M.height() return screen_height() - 2 * theme.BORDER end

--- A page with nothing on it yet: a quiet mark and a line saying so.
local function placeholder(icon, text)
  return function(w, h)
    return kit.card {
      width = w, height = h,
      color = function() return C.surfaceContainerLow end,
      ui.Column {
        anchors = { center_in = true }, gap = 16, align = "center",
        kit.icon(icon, 72, function() return C.outlineVariant end),
        kit.text {
          text = text, font_size = theme.size.large,
          color = function() return C.outlineVariant end,
        },
      },
    }
  end
end

M.TABS = {
  { key = "assistant", name = "Assistant", icon = "forum", build = placeholder("forum", "Nothing here yet") },
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
  panel.shown(M.drawer.open:get())
end)

return M
