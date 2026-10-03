-- A month and the selected day's Taskwarrior agenda. Work-calendar events
-- can be added here later; no account or meeting data is invented.
local ui = require("morf.ui")
local kit = require("kit")
local theme = require("theme")
local widgets = require("planner_widgets")
local C = theme.color
local M = {}

function M.build(model, w, h)
  local viewport, viewport_node, viewport_t, viewport_ctl
  local function heading(props)
    props.viewport = function() return viewport end
    return kit.heading(props)
  end
  local inner, cell = w - 32, (w - 32) / 7
  local days, agenda, title = model.days, model.agenda, model.month
  local weekdays = {}
  for _, name in ipairs(model.weekdays) do
    weekdays[#weekdays + 1] = kit.centred(cell, 26, widgets.label(name))
  end
  viewport_node, viewport, viewport_t, viewport_ctl = kit.scroll({ id = "planner-scroll", anchors = { fill = true, margins = 16 }, clip = true,
      ui.Column { width = inner, gap = 16,
        ui.Item { width = inner, height = 48,
          heading { id = "planner-title", scope = "leftbar.calendar", text = "A day at a time.", font_size = 24, font_weight = 700 },
          widgets.subtitle("Your plans, with space for what comes next.", { id = "planner-subtitle", y = 31 }),
        },
        ui.Item { width = inner, height = 36,
          widgets.button("planner-previous", "", "chevron_left", 36,
            function() model.step(-1) end),
          heading { id = "planner-month-title", scope = "leftbar.calendar", level = "section", anchors = { center_in = true }, text = function() return title:get() end,
            font_size = theme.size.large, font_weight = 600 },
          ui.Item { anchors = { right = true }, width = 36, height = 36,
            widgets.button("planner-next", "", "chevron_right", 36,
              function() model.step(1) end) },
        },
        ui.Column { gap = 4,
          ui.Row { table.unpack(weekdays) },
          ui.Repeater { model = days, as = "grid", columns = 7, gap = 0,
            delegate = function(day)
              local function selected() return model.selected_day:get() == day.key end
              return kit.action { id = "planner-day-" .. day.key, width = cell, height = 43, cursor = "pointer",
                on_clicked = function() model.select(day.key) end,
                kit.surface { anchors = { center_in = true }, width = cell - 6, height = 39, radius = 14,
                  color = function() return selected() and C.primary or day.today and C.primaryContainer or C.surfaceContainer end,
                  (function()
                    local mark = kit.decor("corners", { length = 5,
                      color = function() return selected() and C.onPrimary or kit.stroke("hot")() end })
                    if mark then mark.visible = function() return selected() or day.today end return mark end
                    return ui.Item {}
                  end)() },
                kit.text { anchors = { center_in = true }, text = ("%d"):format(day.day), font_weight = 600,
                  color = function() return selected() and C.onPrimary or day.current and kit.ink("hi")() or kit.stroke("mark")() end },
                kit.surface { anchors = { horizontal_center = true, bottom = true, bottom_margin = 5 },
                  width = 4, height = 4, radius = 2, visible = day.count > 0,
                  color = function() return selected() and C.onPrimary or C.primary end },
              }
            end,
          },
        },
        ui.Row { gap = 8,
          widgets.button("planner-today", "Today", "today", 100, model.today),
          widgets.button("planner-add", "Plan a task", "add", inner - 108, model.plan, function() return true end),
        },
        kit.surface { width = inner, height = 1, color = kit.stroke("quiet") },
        ui.Column { gap = 5,
          heading { id = "planner-day-title", scope = "leftbar.calendar", level = "section", text = model.day_title, font_size = theme.size.large, font_weight = 600 },
          widgets.subtitle(function() return tostring(agenda:len()) .. " tasks planned" end),
        },
        ui.Repeater { as = "column", gap = 8, width = inner, model = agenda,
          delegate = function(item)
            return kit.action { id = "planner-task-" .. item.uuid, width = inner, height = 78, cursor = "pointer",
              on_clicked = function() model.edit(item.uuid) end,
              kit.card { anchors = { fill = true }, radius = 16, color = function() return C.surfaceContainerHigh end },
              kit.surface { x = 0, y = 16, width = 3, height = 46, radius = 1.5, color = kit.signal("accent") },
              widgets.label(item.time, { x = 14, y = 17, width = 54, elide = "right", color = kit.ink("accent") }),
              kit.menu_label { x = 72, y = 14, width = inner - 88, elide = "right", text = item.description, font_weight = 600 },
              widgets.subtitle(item.kind .. (item.project ~= "" and (" · " .. item.project) or ""),
                { x = 72, y = 43, width = inner - 88, elide = "right" }),
            }
          end,
        },
        ui.Column { width = inner, gap = 10, visible = function() return agenda:len() == 0 end,
          kit.icon("event_available", 42, kit.ink("accent")),
          heading { id = "planner-empty-title", scope = "leftbar.calendar", level = "section", visible = function() return agenda:len() == 0 end, text = "Nothing planned yet", font_size = theme.size.large },
          widgets.message("Tasks scheduled or due on this day will appear here.", inner),
        },
        kit.card { width = inner, height = 88, radius = 18, color = function() return C.surfaceContainerHigh end,
          kit.icon("calendar_month", 24, kit.ink("lo"), { x = 16, y = 18 }),
          heading { id = "planner-work-title", scope = "leftbar.calendar", level = "section", x = 54, y = 16, text = "Work calendar", font_weight = 600 },
          kit.subtitle { x = 54, y = 39, width = inner - 70, height = 48, wrap = true,
            text = "Not connected yet. Meetings will appear here once an account is connected.",
            font_size = theme.size.small, color = kit.ink("lo") },
        },
        widgets.message(function() return model.error:get() end, inner, 64),
      },
    })
  return kit.card { id = "planner-calendar", width = w, height = h, viewport_node }
end
return M
