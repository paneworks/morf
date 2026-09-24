-- A clipboard with the tasks down it: the checkbox completes a task. A
-- title along the top when there is room.
--
-- Port of Clipboard.qml. The rows are the Modern faces' task rows
-- (desktop/faces/day_tasks.lua), over the board's tasks, which are empty
-- until that port lands.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local common = require("desktop.faces.common")
local day_tasks = require("desktop.faces.day_tasks")
local S = require("desktop.sources")

local M = {}

--- `values`: `x`, `y`, `width`, `height`, `ctx`, `count` (rows), `title`
--- (string or function; empty for none).
function M.build(values)
  local w, h, ctx = values.width, values.height, values.ctx
  local ink = ctx.ink
  local title = values.title
  local rows = {}
  if title then
    rows[#rows + 1] = kit.text { width = w - 20, elide = "right",
      text = function() return tostring(common.read(title) or "") end,
      size = theme.size.small, weight = 600, color = ink.text }
    rows[#rows + 1] = ui.Item { width = 1, height = 4 }
  end
  rows[#rows + 1] = day_tasks.rows(ctx, w - 20, values.count or 3, function(count) return S.tasks.queue(count) end)
  return ui.Item {
    x = values.x, y = values.y, width = w, height = h,
    ui.Rect { width = w, height = h, radius = 7, color = ink.raised, border_color = ink.border, border_width = 1 },
    ui.Rect { x = w / 2 - 20, y = -6, width = 40, height = 16, radius = 5, color = ink.muted },
    ui.Rect { x = w / 2 - 8, y = -10, width = 16, height = 8, radius = 3, color = ink.text },
    ui.Column { x = 10, y = 18, width = w - 20, gap = 2, table.unpack(rows) },
    kit.text { x = 10, y = title and 42 or 20, width = w - 20, horizontal_alignment = "center",
      visible = function() return #S.tasks.queue(1) == 0 end,
      text = function() return S.tasks.available() and "Nothing to do" or "No board yet" end,
      size = theme.size.small, color = ink.muted },
  }
end

return M
