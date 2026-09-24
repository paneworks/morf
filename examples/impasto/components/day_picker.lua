-- A month grid for picking a day.
--
-- Port of DayPicker.qml: drawn inside the surface that owns it rather than
-- as a popup; the owner places and dismisses it. 42 cells so the height
-- never changes, Monday first. Today is ringed, the selection filled, and a
-- day with tasks gets a dot under it -- accent while any is open, muted once
-- all are done (`tasks.days_with_tasks`). The wheel and the arrows page the
-- month.
--
--     day_picker.build {
--       selected = "2026-09-24",       -- the day it opens on, or ""
--       on_pick = function(day) end,   -- a day key, or "" for none
--       on_dismiss = function() end,   -- Escape
--     }

local ui = require("morf.ui")
local theme = require("theme")
local tasks = require("services.tasks")
local kit = require("components.kit")
local pill = require("components.pill")

local C = theme.color
local T = morf.time
local day_picker = {}

local CELL, GAP, PAD = 30, 2, 12
day_picker.width = 7 * CELL + 6 * GAP + 2 * PAD
day_picker.height = PAD + 22 + 6 + 16 + GAP + 6 * (CELL + GAP) - GAP + 8 + 22 + PAD

local KEY = { escape = 0xff1b, left = 0xff51, right = 0xff53 }

local made = 0

function day_picker.build(values)
  made = made + 1
  local selected = values.selected or ""
  -- Months from the current one; opens on the selected day's month.
  local start = 0
  do
    local today = T.date()
    local y, m = selected:match("^(%d%d%d%d)%-(%d%d)")
    if y then start = (tonumber(y) - today.year) * 12 + tonumber(m) - today.month end
  end
  local offset = morf.signal("impasto.daypicker.offset." .. made, start)

  local function shown()
    tasks.today() -- the month follows midnight
    local today = T.date()
    local index = today.year * 12 + (today.month - 1) + offset:get()
    return index // 12, index % 12 + 1
  end

  local function cell_key(i)
    local y, m = shown()
    local first = T.time { year = y, month = m, day = 1 }
    local leading = T.weekday(y, m, 1) - 1
    return tasks.day_key(T.add(first, { days = i - leading - 1 }))
  end

  local weekdays = {}
  for _, letter in ipairs { "M", "T", "W", "T", "F", "S", "S" } do
    weekdays[#weekdays + 1] = kit.text {
      width = CELL, height = 16, horizontal_alignment = "center",
      text = letter, size = theme.size.label, weight = 600, color = C.textMuted,
    }
  end

  local cells = {}
  for i = 1, 42 do
    local hovered = kit.hover_signal("daypicker.cell")
    local key = function() return cell_key(i) end
    local in_month = function()
      local y, m = shown()
      return key():sub(1, 7) == string.format("%04d-%02d", y, m)
    end
    local chosen = function() return key() == selected end
    local is_today = function() return key() == tasks.today() end
    local marks = function()
      local k = key()
      local y, m, d = tonumber(k:sub(1, 4)), tonumber(k:sub(6, 7)), tonumber(k:sub(9, 10))
      return tasks.days_with_tasks(y, m)[d]
    end
    cells[#cells + 1] = ui.Item {
      width = CELL, height = CELL,
      ui.Rect {
        anchors = { fill = true }, radius = CELL / 2,
        color = function()
          if chosen() then return C.accent() end
          return hovered:get() and C.islandSurfaceHover or "#00000000"
        end,
        border_width = 1,
        border_color = function() return (is_today() and not chosen()) and C.accent() or "#00000000" end,
        behavior = { color = theme.behave("fast") },
      },
      kit.text {
        anchors = { center_in = true },
        text = function() return tostring(tonumber(key():sub(9, 10))) end,
        size = theme.size.small,
        font_weight = function() return (chosen() or is_today()) and 600 or 400 end,
        color = function()
          if chosen() then return C.accentText() end
          return in_month() and C.text() or C.textMuted()
        end,
        opacity = function() return (in_month() or chosen()) and 1 or 0.5 end,
      },
      ui.Rect {
        anchors = { horizontal_center = true, bottom = true, bottom_margin = 3 },
        width = 3, height = 3, radius = 1.5,
        visible = function() return marks() ~= nil end,
        color = function()
          local mark = marks()
          if chosen() then return C.accentText() end
          return (mark and mark.pending > 0) and C.accent() or C.textMuted()
        end,
      },
      ui.MouseArea {
        anchors = { fill = true }, cursor = "pointer",
        on_entered = function() hovered:set(true) end,
        on_exited = function() hovered:set(false) end,
        on_clicked = function() values.on_pick(key()) end,
      },
    }
  end

  local function page(delta) offset:set(offset:get() + delta) end

  return ui.Rect {
    x = values.x, y = values.y, z = values.z,
    width = day_picker.width, height = day_picker.height,
    radius = theme.radius_medium, color = C.islandSurface,
    border_width = 1, border_color = C.islandBorder,
    enter = { opacity = 0, scale = 0.96 },
    opacity = 1, scale = 1,
    transform_origin_x = 0, transform_origin_y = 1,
    behavior = { opacity = theme.behave("fast"), scale = theme.behave("fast") },
    -- Keys and the wheel, under everything else.
    ui.MouseArea {
      anchors = { fill = true }, z = -1, focus = true,
      on_wheel = function(_, _, _, pixel_y, _, steps_y)
        local dy = (steps_y and steps_y ~= 0) and steps_y or pixel_y or 0
        if dy ~= 0 then page(dy > 0 and 1 or -1) end
      end,
      on_key_pressed = function(keysym)
        if keysym == KEY.escape then if values.on_dismiss then values.on_dismiss() end
        elseif keysym == KEY.left then page(-1)
        elseif keysym == KEY.right then page(1) end
      end,
    },
    ui.Column {
      x = PAD, y = PAD, gap = 6,
      ui.Item {
        width = day_picker.width - 2 * PAD, height = 22,
        ui.Row {
          anchors = { left = true, vertical_center = true }, gap = 6,
          kit.text {
            text = function() local y, m = shown() return T.format("%B", T.time { year = y, month = m, day = 1 }) end,
            size = theme.size.regular, weight = 600,
          },
          kit.text { text = function() local y = shown() return tostring(y) end, size = theme.size.regular, color = C.textMuted },
        },
        ui.Row {
          anchors = { right = true, vertical_center = true }, gap = 2,
          kit.icon_button { glyph = "󰅁", glyph_size = 12, diameter = 22, color = "#00000000",
            hover_color = C.islandSurfaceHover, on_click = function() page(-1) end },
          kit.icon_button { glyph = "󰅂", glyph_size = 12, diameter = 22, color = "#00000000",
            hover_color = C.islandSurfaceHover, on_click = function() page(1) end },
        },
      },
      ui.Grid { columns = 7, gap = GAP, table.unpack(weekdays) },
      ui.Grid { columns = 7, gap = GAP, table.unpack(cells) },
      ui.Item {
        width = day_picker.width - 2 * PAD, height = 22 + 2,
        ui.Row {
          anchors = { left = true, bottom = true }, gap = 6,
          pill.button { text = "Today", height = 22, padding = 9, on_click = function() values.on_pick(tasks.today()) end },
          pill.button { text = "Tomorrow", height = 22, padding = 9, on_click = function() values.on_pick(tasks.shifted(1)) end },
        },
        pill.button {
          anchors = { right = true, bottom = true },
          visible = selected ~= "", text = "No day", height = 22, padding = 9,
          on_click = function() values.on_pick("") end,
        },
      },
    },
  }
end

return day_picker
