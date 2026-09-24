-- One task on one line: the tick, the words, the day.
--
-- Port of TaskRow.qml, for a task outside the board (the module detail,
-- the control centre, a calendar day). The tick completes the task, or
-- sends it back to the first lane, and takes the press; the rest of the row
-- calls `on_open`. Colours come in as an ink so a desktop face can pass its
-- own.
--
--     task_row.build { task = function() return row end, width = 300, on_open = fn, dated = true }

local ui = require("morf.ui")
local theme = require("theme")
local tasks = require("services.tasks")
local kit = require("components.kit")

local C = theme.color
local task_row = {}

local function default_ink()
  return {
    text = C.text, muted = C.textMuted, accent = C.accent,
    accent_text = C.accentText, raised = C.islandSurfaceHover, red = C.red,
  }
end

local function read(v) if type(v) == "function" then return v() end return v end

function task_row.build(values)
  local task = values.task
  local ink = values.ink or default_ink()
  local dated = values.dated ~= false
  local width = values.width or 300
  local hovered = kit.hover_signal("task_row")
  local done = function() local t = task() return t and t.state == "done" end
  local late = function() return tasks.is_overdue(task()) end
  local day = kit.text {
    anchors = { right = true, right_margin = 6, vertical_center = true },
    visible = function() local t = task() return dated and t ~= nil and t.due ~= "" end,
    text = function() local t = task() return t and tasks.due_label(t.due) or "" end,
    mono = true, size = theme.size.label,
    color = function() return late() and read(ink.red) or read(ink.muted) end,
  }
  return ui.Item {
    width = width, height = values.height or 24, layout = values.layout,
    ui.Rect {
      anchors = { fill = true }, radius = theme.radius_small,
      color = function() return hovered:get() and read(ink.raised) or "#00000000" end,
      behavior = { color = theme.behave("fast") },
    },
    ui.MouseArea {
      anchors = { fill = true }, cursor = "pointer",
      on_entered = function() hovered:set(true) end,
      on_exited = function() hovered:set(false) end,
      on_clicked = function() if values.on_open then values.on_open() end end,
    },
    -- The tick, over the row's area so its press is its own.
    ui.Item {
      x = 4, width = 18, height = 18, anchors = { vertical_center = true },
      ui.Rect {
        anchors = { center_in = true }, width = 14, height = 14, radius = 7,
        color = function() return done() and read(ink.accent) or "#00000000" end,
        border_width = 1.5,
        border_color = function() return done() and read(ink.accent) or read(ink.muted) end,
        behavior = { color = theme.behave("fast") },
        kit.glyph {
          anchors = { center_in = true }, text = "󰄬", size = 8,
          visible = done, color = function() return read(ink.accent_text) end,
        },
      },
      ui.MouseArea {
        anchors = { fill = true }, cursor = "pointer",
        on_clicked = function() local t = task() if t then tasks.toggle(t.key) end end,
      },
    },
    kit.text {
      x = 30, anchors = { vertical_center = true },
      width = function()
        return width - 30 - 6 - ((dated and task() and task().due ~= "") and ((day.layout_width or 0) + 8) or 0)
      end,
      elide = "right",
      text = function() local t = task() return t and t.text or "" end,
      size = theme.size.small,
      decoration = function() return done() and { line = "through" } or {} end,
      color = function() return done() and read(ink.muted) or read(ink.text) end,
    },
    day,
  }
end

return task_row
