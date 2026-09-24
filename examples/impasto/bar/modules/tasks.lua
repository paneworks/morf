-- The tasks module: how many are open, and the next three.
--
-- Port of TasksModule.qml. Its detail is the next three tasks by due day
-- with their ticks, New and Open. A row opens the board on that task;
-- editing happens in the board. The chip -- a tick and the open count, red
-- while something is overdue -- is `bar.modules.chip`'s, though the
-- original catalogue keeps tasks off the bar (it lives on the desktop).

local ui = require("morf.ui")
local theme = require("theme")
local island = require("bar.island")
local modules = require("services.modules")
local tasks = require("services.tasks")
local kit = require("components.kit")
local pill = require("components.pill")
local task_row = require("components.task_row")

local C = theme.color
local module = {}

module.width, module.height = 356, 150

local function late() return #tasks.overdue() > 0 end

local function open_on(key)
  if key and key ~= "" then tasks.open(key) end
  island.open("board")
end

--- The detail, at the module's declared size.
function module.detail()
  local W = module.width - 8 - 28
  local rows = {}
  for i = 1, 3 do
    local task = function() return tasks.queue()[i] end
    rows[#rows + 1] = task_row.build {
      task = task, width = W,
      on_open = function() local t = task() if t then open_on(t.key) end end,
    }
  end
  local holders = {}
  for i, row in ipairs(rows) do
    holders[i] = ui.Item {
      width = W, height = 24,
      visible = function() return tasks.queue()[i] ~= nil end,
      row,
    }
  end
  return ui.Item {
    anchors = { fill = true },
    ui.Column {
      x = 14, y = 12, gap = 6,
      ui.Item {
        width = W, height = 36,
        ui.Row {
          anchors = { left = true, vertical_center = true }, gap = 12, align = "center",
          kit.glyph { text = "󰄲", size = 20, color = function() return late() and C.red() or C.accent() end },
          ui.Column {
            gap = 1,
            kit.text { text = "Tasks", size = theme.size.medium, weight = 600 },
            kit.text {
              width = W - 32 - 150, elide = "right", size = theme.size.small,
              color = function() return late() and C.red() or C.textMuted() end,
              text = function()
                local pending = tasks.pending()
                if pending == 0 then return tasks.count() == 0 and "Nothing on the board" or "All done" end
                return pending .. " to do · " .. tasks.summary()
              end,
            },
          },
        },
        ui.Row {
          anchors = { right = true, vertical_center = true }, gap = 6,
          pill.button { text = "New", icon = "󰐕", on_click = function()
            tasks.create(false)
            island.open("board")
          end },
          pill.button { text = "Open", icon = "󰄲", on_click = function() open_on("") end },
        },
      },
      -- Soonest first. The tick completes a task in place; the rest of the
      -- row opens it.
      table.unpack(holders),
    },
  }
end

modules.define("tasks", {
  glyph = function() return "󰄲" end,
  value = function() return tostring(tasks.pending()) end,
  -- Red while something is overdue.
  tint = function() return late() and C.red() or C.text() end,
  has = function() return true end,
  detail = module.detail,
})

-- `morf ipc call module.tasks` opens the detail.
morf.ipc["module.tasks"] = function()
  modules.activate("tasks")
  return modules.open_id:get()
end

return module

