-- A month and the selected day's Taskwarrior agenda. Work-calendar events
-- can be added here later; no account or meeting data is invented.
local morf = require("morf")
local ui = require("morf.ui")
local kit = require("kit")
local theme = require("theme")
local planner = require("planner")
local widgets = require("planner_widgets")
local C = theme.color
local M = {}

function M.build(w, h)
  local inner, cell = w - 32, (w - 32) / 7
  local days, agenda = morf.list_model({}), morf.list_model({})
  local title = morf.signal("caelestia.calendar.title", "")
  morf.effect("caelestia.calendar.month", function()
    morf.minute_clock:get()
    local today = morf.time.date()
    local first = morf.time.time { year = today.year, month = today.month, day = 1, hour = 12 }
    first = morf.time.add(first, { months = planner.month_offset:get() })
    local date = morf.time.date(first)
    title:set(morf.time.format("%B %Y", first))
    local out = {}
    for _, week in ipairs(morf.time.month(date.year, date.month, 1)) do
      for _, day in ipairs(week) do
        local key = ("%04d-%02d-%02d"):format(day.year, day.month, day.day)
        out[#out + 1] = { key = key, day = day.day, current = day.current,
          today = key == morf.time.format("%Y-%m-%d"), count = #planner.agenda(key) }
      end
    end
    days:replace(out, "key")
  end)
  morf.effect("caelestia.calendar.agenda", function()
    agenda:replace(planner.agenda(planner.selected_day:get()), "uuid")
  end)
  local function today()
    planner.month_offset:set(0)
    planner.selected_day:set(morf.time.format("%Y-%m-%d"))
  end
  local function add()
    require("leftbar").panel.select("tasks")
    require("tasks_page").edit(nil, planner.selected_day:get())
  end
  local weekdays = {}
  for _, name in ipairs { "M", "T", "W", "T", "F", "S", "S" } do
    weekdays[#weekdays + 1] = kit.centred(cell, 26, widgets.label(name))
  end
  return kit.card { id = "planner-calendar", width = w, height = h,
    ui.Flickable { anchors = { fill = true, margins = 16 }, clip = true,
      ui.Column { width = inner, gap = 16,
        ui.Item { width = inner, height = 48,
          kit.text { text = "A day at a time.", font_size = 24, font_weight = 700 },
          widgets.label("Your plans, with space for what comes next.", { y = 31 }),
        },
        ui.Item { width = inner, height = 36,
          widgets.button("planner-previous", "", "chevron_left", 36,
            function() planner.month_offset:set(planner.month_offset:get() - 1) end),
          kit.text { anchors = { center_in = true }, text = function() return title:get() end,
            font_size = theme.size.large, font_weight = 600 },
          ui.Item { anchors = { right = true }, width = 36, height = 36,
            widgets.button("planner-next", "", "chevron_right", 36,
              function() planner.month_offset:set(planner.month_offset:get() + 1) end) },
        },
        ui.Column { gap = 4,
          ui.Row { table.unpack(weekdays) },
          ui.Repeater { model = days, as = "grid", columns = 7, gap = 0,
            delegate = function(day)
              local function selected() return planner.selected_day:get() == day.key end
              return ui.MouseArea { id = "planner-day-" .. day.key, width = cell, height = 43, cursor = "pointer",
                on_clicked = function() planner.selected_day:set(day.key) end,
                ui.Rect { anchors = { center_in = true }, width = cell - 6, height = 39, radius = 14,
                  color = function() return selected() and C.primary or day.today and C.primaryContainer or C.surfaceContainer end },
                kit.text { anchors = { center_in = true }, text = ("%d"):format(day.day), font_weight = 600,
                  color = function() return selected() and C.onPrimary or day.current and C.onSurface or C.outline end },
                ui.Rect { anchors = { horizontal_center = true, bottom = true, bottom_margin = 5 },
                  width = 4, height = 4, radius = 2, visible = day.count > 0,
                  color = function() return selected() and C.onPrimary or C.primary end },
              }
            end,
          },
        },
        ui.Row { gap = 8,
          widgets.button("planner-today", "Today", "today", 100, today),
          widgets.button("planner-add", "Plan a task", "add", inner - 108, add, function() return true end),
        },
        ui.Rect { width = inner, height = 1, color = function() return C.outlineVariant end },
        ui.Column { gap = 5,
          kit.text { id = "planner-day-title", text = function()
            local t = morf.time.parse(planner.selected_day:get())
            return t and morf.time.format("%A, %d %B", t) or planner.selected_day:get()
          end, font_size = theme.size.large, font_weight = 600 },
          widgets.label(function() return tostring(agenda:len()) .. " tasks planned" end),
        },
        ui.Repeater { as = "column", gap = 8, width = inner, model = agenda,
          delegate = function(item)
            return ui.MouseArea { id = "planner-task-" .. item.uuid, width = inner, height = 78, cursor = "pointer",
              on_clicked = function()
                for _, task in ipairs(planner.client.tasks:get()) do
                  if task.uuid == item.uuid then
                    require("leftbar").panel.select("tasks")
                    require("tasks_page").edit(task)
                    break
                  end
                end
              end,
              ui.Rect { anchors = { fill = true }, radius = 16, color = function() return C.surfaceContainerHigh end },
              ui.Rect { x = 0, y = 16, width = 3, height = 46, radius = 1.5, color = function() return C.primary end },
              widgets.label(item.time, { x = 14, y = 17, color = function() return C.primary end }),
              kit.text { x = 72, y = 14, width = inner - 88, elide = "right", text = item.description, font_weight = 600 },
              widgets.label(item.kind .. (item.project ~= "" and (" · " .. item.project) or ""),
                { x = 72, y = 43, width = inner - 88, elide = "right" }),
            }
          end,
        },
        ui.Column { width = inner, gap = 10, visible = function() return agenda:len() == 0 end,
          kit.icon("event_available", 42, function() return C.primary end),
          kit.text { text = "Nothing planned yet", font_size = theme.size.large },
          widgets.message("Tasks scheduled or due on this day will appear here.", inner),
        },
        ui.Rect { width = inner, height = 88, radius = 18, color = function() return C.surfaceContainerHigh end,
          kit.icon("calendar_month", 24, function() return C.onSurfaceVariant end, { x = 16, y = 18 }),
          kit.text { x = 54, y = 16, text = "Work calendar", font_weight = 600 },
          kit.text { x = 54, y = 39, width = inner - 70, height = 48, wrap = true,
            text = "Not connected yet. Meetings will appear here once an account is connected.",
            font_size = theme.size.small, color = function() return C.onSurfaceVariant end },
        },
        widgets.message(function() return planner.client.error:get() end, inner, 64),
      },
    },
  }
end
return M
