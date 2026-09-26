-- Opening a drawer by touching the frame's edge where it lives.
--
-- A thin strip on the frame's border -- the bottom edge under the
-- launcher -- opens its drawer when the pointer reaches it. A drawer opened this way shuts again
-- once the pointer has left the strip and every panel that belongs to it,
-- after a moment's grace for the crossing from the edge onto the panel. One
-- opened by a key or a verb stays until it is shut.

local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local config = require("config")

local M = {}

-- How long the pointer may be off both the strip and the panel before a
-- drawer opened by hover shuts: enough to cross the frame's seam.
local GRACE_MS = 120

--- A strip on one edge that opens `opts.drawer` on hover. Options:
--- `name`, `drawer`, `edge` ("bottom" or "right"), `length` (a function:
--- the strip's length along the edge), `panels` (the panels that keep it
--- open; the drawer's own by default), `setting` (a boolean setting that
--- turns it off). Returns the node to place over the whole screen.
function M.edge(opts)
  local d = opts.drawer
  local panels = opts.panels or { d.panel }
  local strip
  if opts.edge == "bottom" then
    strip = ui.MouseArea {
      id = opts.name .. "-trigger",
      anchors = { bottom = true, horizontal_center = true },
      width = opts.length, height = theme.BORDER,
    }
  else
    strip = ui.MouseArea {
      id = opts.name .. "-trigger",
      anchors = { right = true, vertical_center = true },
      width = theme.BORDER, height = opts.length,
    }
  end

  local by_hover = false
  local closing
  local function over()
    if strip.hovered then return true end
    for _, panel in ipairs(panels) do
      if type(panel) == "function" then panel = panel() end
      if panel and panel.contains_pointer then return true end
    end
    return false
  end

  morf.effect("caelestia.hover." .. opts.name, function()
    if opts.setting and not config.get(opts.setting) then return end
    if over() then
      if closing then closing:cancel() closing = nil end
      if not d.open:get() then
        by_hover = true
        d.set(true)
      end
    elseif by_hover and d.open:get() then
      if closing then closing:cancel() end
      closing = morf.timer(GRACE_MS, function()
        closing = nil
        if not over() then
          by_hover = false
          d.set(false)
        end
      end, false)
    end
  end)
  -- Opened or shut some other way: it is no longer hover's to shut.
  morf.effect("caelestia.hover." .. opts.name .. ".other", function()
    if not d.open:get() then by_hover = false end
  end)

  -- The strip sits on the frame's border, inside the screen.
  return ui.Item {
    anchors = { fill = true, left_margin = theme.BAR },
    strip,
  }
end

return M
