-- The control centre's tasks block: TasksBlock.qml.
--
-- Square (1x2): the count and the next task's day. Larger: a header with
-- Open, then the next tasks by due day, two more with each extra row. A row
-- opens the board on that task; the square opens the board.

local ui = require("morf.ui")
local theme = require("theme")
local tasks = require("services.tasks")
local kit = require("components.kit")
local controls = require("components.controls")
local task_row = require("components.task_row")

local C = theme.color
local M = {}

local function late() return #tasks.overdue() > 0 end
local function tint() return late() and C.red() or C.accent() end

local function open_on(o, key)
  if key ~= "" then tasks.open(key) end
  if o.on_panel then o.on_panel("board") end
end

--- The accent's tint behind a glyph, a rounded 34 px square.
function M.badge(glyph, color)
  return ui.Rect {
    width = 34, height = 34, radius = 10,
    color = function() return (type(color) == "function" and color() or color):alpha(0.2) end,
    kit.glyph { anchors = { center_in = true }, glyph = glyph, size = 17, color = color },
  }
end

local function square(o, w, h)
  local hovered = controls.signal("tasks.block", false)
  return ui.Item {
    width = w, height = h,
    kit.glyph { anchors = { left = true, top = true }, glyph = "󰄲", size = 20, color = tint },
    kit.text { anchors = { right = true, top = true, top_margin = 3 }, text = "Tasks",
      size = theme.size.small, weight = 600, color = C.textMuted },
    ui.Column {
      anchors = { left = true, bottom = true }, gap = 0,
      kit.text { text = function() return tostring(tasks.pending()) end,
        size = theme.size.widget, weight = 600 },
      kit.text {
        width = w, elide = "right", size = theme.size.label,
        color = function() return late() and C.red() or C.textMuted() end,
        text = function()
          if tasks.pending() == 0 then return tasks.count() == 0 and "nothing yet" or "all done" end
          return tasks.summary()
        end,
      },
    },
    controls.hit { hovered = hovered, on_click = function() open_on(o, "") end },
  }
end

local function list(o, w, h)
  -- Two grid rows hold the header and two tasks; each further row two more.
  local capacity = math.max(0, o.rows * 2 - 2)
  local open = controls.pill { text = "Open", height = 26, on_click = function() open_on(o, "") end }
  local column = {
    gap = 6,
    ui.Item {
      width = w, height = 34,
      ui.Row {
        anchors = { left = true, vertical_center = true }, gap = 12, align = "center",
        M.badge("󰄲", tint),
        ui.Column {
          gap = 1,
          kit.text { text = "Tasks", size = theme.size.small, weight = 600 },
          kit.text {
            width = function() return w - 34 - 12 - (open.layout_width or 56) - 12 end, elide = "right",
            size = theme.size.label,
            color = function() return late() and C.red() or C.textMuted() end,
            text = function()
              local pending = tasks.pending()
              if pending == 0 then return tasks.count() == 0 and "Nothing on the board" or "All done" end
              return pending .. " to do · " .. tasks.summary()
            end,
          },
        },
      },
      ui.Item { anchors = { right = true, vertical_center = true }, height = 26,
        width = function() return open.layout_width or 56 end, open },
    },
  }
  for index = 1, capacity do
    -- A row being taken down keeps its last task, so its bindings never
    -- read nothing on the way out.
    local last
    local task = function() local t = tasks.queue()[index] if t then last = t end return t or last end
    column[#column + 1] = ui.Loader {
      active = function() return tasks.queue()[index] ~= nil end,
      source = function()
        return task_row.build {
          task = task, width = w,
          on_open = function() local t = task() if t then open_on(o, t.key) end end,
        }
      end,
    }
  end
  return ui.Item { width = w, height = h, ui.Column(column) }
end

function M.build(o)
  local w, h = o.width - 28, o.height - 28
  return controls.card {
    width = o.width, height = o.height,
    o.cols == 1 and square(o, w, h) or list(o, w, h),
  }
end

return M
