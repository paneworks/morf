-- Task rows for the faces that list them: a box to tick, the task, and its
-- day when `dated`.
--
-- Port of the TaskRow the faces draw and the DayTasks list a pressed day
-- opens, over the board's tasks (`services.tasks`, another port's). The rows
-- are read through `desktop.sources`, which is empty until that port lands.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local S = require("desktop.sources")

local M = {}

--- `count` rows `width` wide; `list(n)` returns the tasks to show.
function M.rows(ctx, width, count, list)
  local ink = ctx.ink
  local nodes = {}
  for i = 1, count do
    local function task() return list(count)[i] end
    local function done()
      local t = task()
      return t ~= nil and (t.done == true or t.state == "done")
    end
    nodes[i] = ui.Item {
      width = width, height = 24,
      visible = function() return task() ~= nil end,
      ui.Rect {
        x = 2, y = 5, width = 14, height = 14, radius = 7,
        color = function() return done() and ink.accent() or morf.color("transparent") end,
        border_width = 1.5,
        border_color = function() return done() and ink.accent() or ink.muted() end,
        kit.glyph { anchors = { center_in = true }, size = 9, glyph = "󰄬",
          visible = done, color = ink.accentText },
      },
      kit.text {
        x = 24, y = 4, width = width - 24, elide = "right", size = theme.size.small,
        color = function() return done() and ink.muted() or ink.text() end,
        text = function() local t = task() return t and (t.text or t.title or "") or "" end,
      },
      ui.MouseArea {
        x = 0, y = 0, width = 20, height = 24, cursor = "pointer",
        on_clicked = function() local t = task() if t and t.key then S.tasks.toggle(t.key) end end,
      },
    }
  end
  return ui.Column { width = width, gap = 2, table.unpack(nodes) }
end

return M
