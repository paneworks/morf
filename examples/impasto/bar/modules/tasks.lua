-- The tasks module: how many are open, and the next three.
--
-- Port of TasksModule.qml. On the bar it is a chip -- a tick and the open
-- count, red while something is overdue -- whose click opens its detail in
-- the island: the next three tasks by due day with their ticks, New and
-- Open. A row opens the board on that task; editing happens in the board.
--
-- The original catalogue keeps tasks off the bar by default (it lives on
-- the desktop); here the chip is a piece like any other, so `barLeft` or
-- `barRight` can name "tasks".

local ui = require("morf.ui")
local theme = require("theme")
local island = require("bar.island")
local bar = require("bar.bar")
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
  local W = module.width - 28
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
    width = module.width, height = module.height,
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

--- The chip: a tick and the open count.
function module.chip()
  local hovered = kit.hover_signal("tasks.chip")
  local row = ui.Row {
    anchors = { center_in = true }, gap = 5, align = "center",
    kit.glyph { text = "󰄲", size = 13, color = function() return late() and C.red() or C.text() end },
    kit.text { text = function() return tostring(tasks.pending()) end, size = theme.size.small, weight = 600 },
  }
  return ui.Rect {
    width = function() return (row.layout_width or 0) + 16 end,
    height = function() return theme.capsule_height() - 6 end,
    radius = function() return (theme.capsule_height() - 6) / 2 end,
    color = function() return hovered:get() and C.islandSurfaceHover or "#00000000" end,
    behavior = { color = theme.behave("fast") },
    row,
    ui.MouseArea {
      anchors = { fill = true }, cursor = "pointer",
      on_entered = function() hovered:set(true) end,
      on_exited = function() hovered:set(false) end,
      on_clicked = function() island.toggle("module.tasks") end,
    },
  }
end

island.register("module.tasks", {
  size = function() return module.width, module.height end,
  padding = function() return 0 end,
  declared = true,
  build = module.detail,
})
bar.register("tasks", { build = module.chip })
morf.ipc["module.tasks"] = function()
  island.toggle("module.tasks")
  return island.state.open_panel()
end

return module
