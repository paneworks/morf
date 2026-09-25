-- A month grid from plain date arithmetic: CalendarCard.qml.
--
-- The control centre's calendar block, and the calendar module's detail
-- (bare: the island is the card). Always 42 cells, so the card does not
-- change height between five- and six-week months; days of the months
-- either side are dimmed, and today takes the accent in its own month.
-- The arrows and the wheel page through months; "Today" comes back.
--
-- A dot under a day marks tasks due on it, in the accent while any is
-- open and muted once all are done; clicking such a day turns the card
-- into that day's list, with a way back in the corner. A row opens the
-- task on the board (`on_panel("board")`).

local ui = require("morf.ui")
local theme = require("theme")
local tasks = require("services.tasks")
local kit = require("components.kit")
local controls = require("components.controls")
local task_row = require("components.task_row")

local C = theme.color
local M = {}

M.WEEKDAYS = { "M", "T", "W", "T", "F", "S", "S" }

-- Today, as "YYYY-MM-DD": nothing here changes sooner than midnight, so
-- the grid follows this rather than the second hand.
M.today = morf.signal("impasto.calendar.today", morf.time.format("%Y-%m-%d"))
morf.timer(30000, function()
  local now = morf.time.format("%Y-%m-%d")
  if now ~= M.today:get() then M.today:set(now) end
end)

local function today()
  local y, m, d = M.today:get():match("^(%d+)-(%d+)-(%d+)$")
  return tonumber(y), tonumber(m), tonumber(d)
end
M.today_parts = today

local function days_in(year, month)
  local t = morf.time.time { year = year, month = month, day = 1 }
  return morf.time.date(t).days_in_month
end

--- The month `offset` months from today's: year, month.
function M.shown(offset)
  local y, m = today()
  local index = (y * 12 + (m - 1)) + offset
  return index // 12, index % 12 + 1
end

--- 42 cells: { day, in_month, today }.
function M.cells(offset)
  local year, month = M.shown(offset)
  local ty, tm, td = today()
  local leading = morf.time.weekday(year, month, 1) - 1
  local total = days_in(year, month)
  local py, pm = M.shown(offset - 1)
  local previous = days_in(py, pm)
  local out = {}
  for index = 0, 41 do
    local at = index - leading
    local cell
    if at < 0 then
      cell = { day = previous + at + 1, in_month = false }
    elseif at >= total then
      cell = { day = at - total + 1, in_month = false }
    else
      cell = { day = at + 1, in_month = true }
    end
    cell.today = cell.in_month and year == ty and month == tm and cell.day == td
    out[index + 1] = cell
  end
  return out
end

local MONTHS = { "January", "February", "March", "April", "May", "June", "July",
  "August", "September", "October", "November", "December" }
M.MONTHS = MONTHS

-- Every card's picked day, for `morf ipc call calendar.pick <day>`, which
-- does what a click on a day does.
local pickers = setmetatable({}, { __mode = "k" })
morf.ipc["calendar.pick"] = function(day)
  for signal in pairs(pickers) do signal:set(day or "") end
  return day or ""
end

--- `width` x `height`, `bare`, `padding`, `on_panel(name)` for a task row.
function M.build(options)
  local width, height = options.width, options.height
  local padding = options.padding or 14
  local inner_w, inner_h = width - 2 * padding, height - 2 * padding
  local offset = controls.signal("calendar.offset", 0)
  -- Worked out once per day and month shown, not once per reader: every
  -- cell's half dozen bindings ask for the grid, and building it for each
  -- was 42 x 42 tables and a pass over the calendar per binding -- twenty
  -- milliseconds of the control centre opening. Both signals are still read
  -- on every call, so the bindings still follow them.
  local cached_key, cached = nil, nil
  local cells = function()
    local key = M.today:get() .. "|" .. offset:get()
    if key ~= cached_key then cached_key, cached = key, M.cells(offset:get()) end
    return cached
  end
  local header_h, weekday_h, gap = 22, 16, 9
  local cell_w = (inner_w - 6 * 2) / 7
  local cell_h = (inner_h - header_h - gap - weekday_h - 2 - 5 * 2) / 6
  local grid = { columns = 7, gap = 2 }
  for _, name in ipairs(M.WEEKDAYS) do
    grid[#grid + 1] = ui.Item {
      width = cell_w, height = weekday_h,
      kit.text { anchors = { center_in = true }, text = name, size = theme.size.label,
        weight = 600, color = C.textMuted },
    }
  end
  local circle = math.min(cell_w, cell_h)
  -- The day shown instead of the month, as a day key; "" for the month.
  local picked = controls.signal("calendar.picked", "")
  pickers[picked] = true
  for index = 1, 42 do
    local cell = function() return cells()[index] end
    -- Tasks due that day, for days in the month shown.
    local key = function()
      local c = cell()
      if not c.in_month then return "" end
      local year, month = M.shown(offset:get())
      return ("%04d-%02d-%02d"):format(year, month, c.day)
    end
    local due = function() local k = key() return k ~= "" and tasks.count_on(k) or 0 end
    local pending = function() local k = key() return k ~= "" and tasks.pending_on(k) or 0 end
    local label = kit.text {
      text = function() return tostring(cell().day) end,
      size = theme.size.small,
      weight = function() return cell().today and 600 or 400 end,
      color = function()
        local c = cell()
        if c.today then return C.accentText() end
        return c.in_month and C.text() or C.textMuted()
      end,
      opacity = function() return cell().in_month and 1 or 0.35 end,
    }
    grid[#grid + 1] = ui.Item {
      width = cell_w, height = cell_h,
      ui.Rect {
        anchors = { center_in = true }, width = circle, height = circle, radius = circle / 2,
        color = function() return cell().today and C.accent() or "#00000000" end,
        behavior = { color = theme.behave("fast") },
        -- Lifted a pixel over a dot.
        ui.Item {
          anchors = { horizontal_center = true },
          y = function() return (circle - (label.layout_height or 13)) / 2 - (due() > 0 and 1 or 0) end,
          width = function() return label.layout_width or 0 end,
          height = function() return label.layout_height or 13 end,
          label,
        },
        -- Tasks due: accent while any is open, muted once all are done.
        ui.Rect {
          anchors = { horizontal_center = true, bottom = true, bottom_margin = 3 },
          width = 3, height = 3, radius = 1.5,
          visible = function() return due() > 0 end,
          color = function()
            if cell().today then return C.accentText() end
            return pending() > 0 and C.accent() or C.textMuted()
          end,
        },
      },
      -- Only days with tasks can be clicked: they turn the card into the
      -- day's list.
      ui.MouseArea {
        anchors = { fill = true },
        visible = function() return due() > 0 end,
        cursor = "pointer",
        on_clicked = function() picked:set(key()) end,
      },
    }
  end
  local shown_month = function()
    M.today:get()
    local _, month = M.shown(offset:get())
    return MONTHS[month]
  end
  local shown_year = function()
    M.today:get()
    local year = M.shown(offset:get())
    return tostring(year)
  end
  -- The day: a way back, the date, how many are left, and a row per task;
  -- a row opens the task on the board.
  local due_list = function() return tasks.on(picked:get()) end
  -- Built only while a day is picked; each row only while a task fills it.
  local day_page = function()
  local rows = { gap = 6 }
  local max_rows = math.max(1, math.floor((inner_h - 24 - 1 - 12) / 30))
  for index = 1, max_rows do
    -- A row being taken down keeps its last task, so its bindings never
    -- read nothing on the way out.
    local last
    local task = function() local t = due_list()[index] if t then last = t end return t or last end
    rows[#rows + 1] = ui.Loader {
      active = function() return due_list()[index] ~= nil end,
      source = function()
        return task_row.build {
          task = task, width = inner_w, dated = false,
          on_open = function()
            local t = task()
            if not t then return end
            tasks.open(t.key)
            if options.on_panel then options.on_panel("board") end
          end,
        }
      end,
    }
  end
  local count = kit.text {
    anchors = { right = true, vertical_center = true },
    text = function()
      local list = due_list()
      if #list == 0 then return "" end
      local left = 0
      for _, t in ipairs(list) do if t.state ~= "done" then left = left + 1 end end
      return left .. " of " .. #list
    end,
    mono = true, size = theme.size.label, color = C.textMuted,
  }
  return ui.Column {
    gap = 6,
    ui.Item {
      width = inner_w, height = 24,
      ui.Row {
        anchors = { left = true, vertical_center = true }, gap = 6, align = "center",
        controls.icon_button { icon = "󰁍", icon_size = 13, width = 26, height = 24,
          on_click = function() picked:set("") end },
        kit.text {
          width = function() return inner_w - 32 - (count.layout_width or 0) - 8 end, elide = "right",
          text = function()
            local at = tasks.date_of(picked:get())
            return at and morf.time.format("%A %-d %B", at) or ""
          end,
          size = theme.size.small, weight = 600,
        },
      },
      count,
    },
    controls.hairline { width = inner_w },
    ui.Column(rows),
  } end

  return controls.card {
    bare = options.bare, padding = padding, width = width, height = height,
    ui.Loader { active = function() return picked:get() ~= "" end, source = day_page },
    ui.Column {
      visible = function() return picked:get() == "" end,
      gap = gap,
      ui.Item {
        width = inner_w, height = header_h,
        ui.Row {
          anchors = { left = true, vertical_center = true }, gap = 6, align = "center",
          kit.text { text = shown_month, size = theme.size.medium, weight = 600 },
          kit.text { text = shown_year, size = theme.size.medium, color = C.textMuted },
        },
        ui.Row {
          anchors = { right = true, vertical_center = true }, gap = 4, align = "center",
          -- Today's weekday in its own month, otherwise a way back to it.
          -- Loaders, not hidden nodes: a hidden child keeps its room in a Row.
          ui.Loader {
            active = function() return offset:get() == 0 end,
            source = function()
              return kit.text {
                text = function() M.today:get() return morf.time.format("%A") end,
                size = theme.size.small, color = C.accent,
              }
            end,
          },
          ui.Loader {
            active = function() return offset:get() ~= 0 end,
            source = function()
              return controls.pill { text = "Today", height = 22, width = 56, padding = 9,
                on_click = function() offset:set(0) end }
            end,
          },
          controls.icon_button { icon = "󰅁", icon_size = 12, width = 24, height = 22,
            on_click = function() offset:set(offset:get() - 1) end },
          controls.icon_button { icon = "󰅂", icon_size = 12, width = 24, height = 22,
            on_click = function() offset:set(offset:get() + 1) end },
        },
      },
      ui.Item {
        width = inner_w, height = inner_h - header_h - gap,
        ui.Grid(grid),
        -- The wheel pages the month.
        ui.MouseArea {
          anchors = { fill = true }, z = -1,
          on_wheel = function(_, _, _, _, _, steps)
            if steps and steps ~= 0 then offset:set(offset:get() + (steps > 0 and 1 or -1)) end
          end,
        },
      },
    },
  }
end

return M
