-- Taskwarrior tasks, with a full editor inside the left drawer.
local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local widgets = require("planner_widgets")
local planner = require("planner")
local client = planner.client
local C = theme.color
local M = {}
local editing = morf.signal("caelestia.tasks.editing", false)
local deleting = morf.signal("caelestia.tasks.deleting", false)
local query = morf.signal("caelestia.tasks.query", "")
local filter = morf.signal("caelestia.tasks.filter", "Open")
local rows = morf.list_model({})
local inputs, original, selected = {}, {}, nil

function M.edit(task, day)
  selected = task
  deleting:set(false)
  original = {}
  for name, input in pairs(inputs) do
    local value = task and task[name] or ""
    if name == "tags" or name == "depends" then value = type(value) == "table" and table.concat(value, ",") or value end
    if name == "due" or name == "scheduled" or name == "wait" or name == "until" then
      value = planner.dates.date(value)
    end
    if name == "scheduled" and day then value = day .. "T09:00" end
    original[name] = tostring(value or "")
    input.text = original[name]
  end
  client.error:set("")
  editing:set(true)
  if inputs.description then inputs.description.focus = true end
end

local function save()
  local fields = { description = inputs.description.text }
  for name, input in pairs(inputs) do
    if not selected or input.text ~= original[name] then fields[name] = input.text end
  end
  client.save(selected and selected.uuid, fields, function(ok) if ok then editing:set(false) end end)
end

function M.build(w, h)
  local PAD, inner = 16, w - 32
  local function by_uuid(id)
    for _, t in ipairs(client.tasks:get()) do if t.uuid == id then return t end end
  end
  morf.effect("caelestia.tasks.rows", function()
    morf.minute_clock:get()
    local found, q, f = {}, query:get():lower(), filter:get()
    local today = morf.time.format("%Y-%m-%d")
    for _, task in ipairs(client.tasks:get()) do
      local haystack = (task.description .. " " .. (task.project or "") .. " " .. table.concat(task.tags or {}, " ")):lower()
      local due, scheduled = planner.dates.day(task.due), planner.dates.day(task.scheduled)
      local matches = f == "Open" or (f == "Active" and task.start ~= nil)
        or (f == "Today" and ((due ~= "" and due <= today) or scheduled == today))
      if matches and (q == "" or haystack:find(q, 1, true)) then
        local details = {}
        if task.project and task.project ~= "" then details[#details + 1] = task.project end
        if task.priority and task.priority ~= "" then details[#details + 1] = "Priority " .. task.priority end
        if task.start then details[#details + 1] = "In progress" end
        if task.wait then details[#details + 1] = "Waiting" end
        local when = task.scheduled or task.due
        found[#found + 1] = { uuid = task.uuid, description = task.description,
          detail = table.concat(details, " · "),
          date = when and ((task.scheduled and "Scheduled " or "Due ") .. planner.dates.date(when, "%d %b · %H:%M")) or "No date · click to plan",
          overdue = due ~= "" and due < today }
      end
    end
    rows:replace(found, "uuid")
  end)

  local search = widgets.field("tasks-search", "Find a task", "Search tasks, projects or tags", inner,
    function(text) query:set(text) end)
  local filters = { gap = 6 }
  for _, name in ipairs { "Open", "Today", "Active" } do
    filters[#filters + 1] = widgets.button("tasks-filter-" .. name:lower(), name, nil, (inner - 12) / 3,
      function() filter:set(name) end, function() return filter:get() == name end)
  end
  local task_list = ui.Repeater {
    as = "column", gap = 8, width = inner, model = rows,
    delegate = function(row)
      return ui.Rect { id = "task-row-" .. row.uuid, width = inner, height = 100, radius = 18,
        color = function() return C.surfaceContainerHigh end,
        ui.MouseArea { id = "task-done-" .. row.uuid, x = 8, y = 12, width = 38, height = 38, cursor = "pointer",
          on_clicked = function() client.action(row.uuid, "done") end,
          kit.icon("radio_button_unchecked", 23, function() return row.overdue and C.error or C.primary end,
            { anchors = { center_in = true } }) },
        ui.MouseArea { id = "task-edit-" .. row.uuid, x = 52, y = 12, width = inner - 64, height = 78, cursor = "pointer",
          on_clicked = function() local task = by_uuid(row.uuid) if task then M.edit(task) end end,
          kit.text { text = row.description, width = inner - 64, height = 25, elide = "right", font_weight = 600 },
          widgets.label(row.detail, { y = 30, width = inner - 64, elide = "right" }),
          widgets.label(row.date, { y = 54, width = inner - 64, elide = "right",
            color = function() return row.overdue and C.error or C.onSurfaceVariant end }),
        },
      }
    end,
  }
  local browse = ui.Item { width = w, height = h, visible = function() return not editing:get() end,
    ui.Column { x = PAD, y = PAD, width = inner, gap = 12,
      ui.Item { width = inner, height = 52,
        kit.text { text = "Make room for today.", font_size = 23, font_weight = 700 },
        widgets.label(function() return tostring(#client.tasks:get()) .. " open tasks · Taskwarrior" end, { y = 31 }),
      },
      ui.Row { gap = 8,
        widgets.button("tasks-add", "New task", "add", inner - 104, function() M.edit() end, function() return true end),
        widgets.button("tasks-refresh", "Refresh", "refresh", 96, client.refresh),
      },
      search, ui.Row(filters),
      ui.Flickable { id = "tasks-list", width = inner, height = function() return math.max(100, h() - 310) end, clip = true,
        task_list,
        ui.Column { width = inner, gap = 8, visible = function() return rows:len() == 0 end,
          kit.icon("task_alt", 40, function() return C.primary end),
          kit.text { text = function() return client.loaded:get() and "A little breathing room." or "Your tasks, right here." end,
            font_size = theme.size.large },
          widgets.message(function() return client.busy:get() and "Loading tasks…" or "Add a task, or choose another view." end, inner),
        },
      },
    },
  }

  local fields = { gap = 12, width = inner }
  local specs = {
    { "description", "Task", "What needs doing?" },
    { "project", "Project", "work.website" },
    { "priority", "Priority", "H, M or L — leave blank for none" },
    { "scheduled", "Scheduled · start date & time", "tomorrow or 2026-10-01T09:00" },
    { "due", "Due · deadline", "friday or 2026-10-02T17:00" },
    { "wait", "Hide until", "tomorrow — leave blank to show now" },
    { "tags", "Tags", "work, calls" },
    { "recur", "Repeat", "daily, weekly, monthly — needs a due date" },
    { "until", "Stop repeating after", "2026-12-31" },
    { "depends", "Depends on tasks", "Task IDs or UUIDs, separated by commas" },
  }
  for _, spec in ipairs(specs) do
    local node, input = widgets.field("task-" .. spec[1], spec[2], spec[3], inner)
    fields[#fields + 1], inputs[spec[1]] = node, input
  end
  fields[#fields + 1] = widgets.message("Dates accept Taskwarrior expressions or a date and time. Editing a repeating occurrence changes only that occurrence.", inner, 60)
  local actions = ui.Row { gap = 6, visible = function() return editing:get() and selected ~= nil end,
    widgets.button("task-complete", "Done", "check", 88, function()
      if selected then client.action(selected.uuid, "done", function(ok) if ok then editing:set(false) end end) end
    end),
    widgets.button("task-start", function() editing:get() return selected and selected.start and "Stop" or "Start" end, "timer", 88, function()
      if selected then client.action(selected.uuid, selected.start and "stop" or "start",
        function(ok) if ok then editing:set(false) end end) end
    end),
    widgets.button("task-delete", function() return deleting:get() and "Confirm delete" or "Delete" end, "delete", inner - 188, function()
      if not selected then return end
      if not deleting:get() then deleting:set(true) return end
      client.action(selected.uuid, "delete", function(ok) if ok then editing:set(false) end end)
    end),
  }
  fields[#fields + 1] = actions
  local editor = ui.Item { width = w, height = h, visible = function() return editing:get() end,
    ui.Item { x = PAD, y = PAD, width = inner, height = 40,
      kit.text { text = function() editing:get() return selected and "Edit task" or "New task" end, font_size = 24, font_weight = 700 },
      ui.Item { anchors = { right = true }, width = 76, height = 36,
        widgets.button("task-cancel", "Back", "arrow_back", 76, function() editing:set(false) end) },
    },
    ui.Flickable { id = "task-editor-scroll", x = PAD, y = 70, width = inner,
      height = function() return math.max(100, h() - 182) end, clip = true, ui.Column(fields) },
    ui.Item { x = PAD, y = function() return h() - 100 end, width = inner, height = 40,
      widgets.button("task-save", function() return client.busy:get() and "Saving…" or "Save task" end,
        "check", inner, save, function() return true end) },
  }
  return kit.card { id = "tasks-page", width = w, height = h, browse, editor,
    kit.text { id = "tasks-error", x = PAD, y = function() return h() - 50 end,
      width = inner, height = 44, wrap = true, text = function() return client.error:get() end,
      font_size = theme.size.small, color = function() return C.error end },
  }
end
return M
