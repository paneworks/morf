-- A month grid from plain date arithmetic: CalendarCard.qml.
--
-- The control centre's calendar block, and the calendar module's detail
-- (bare: the island is the card). Always 42 cells, so the card does not
-- change height between five- and six-week months; days of the months
-- either side are dimmed, and today takes the accent in its own month.
-- The arrows and the wheel page through months; "Today" comes back.
--
-- The original marked days with tasks due and turned into that day's list
-- on a click; the tasks service is another port, so the days here are not
-- marked yet.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local controls = require("components.controls")

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

--- `width` x `height`, `bare`, `padding`.
function M.build(options)
  local width, height = options.width, options.height
  local padding = options.padding or 14
  local inner_w, inner_h = width - 2 * padding, height - 2 * padding
  local offset = controls.signal("calendar.offset", 0)
  local cells = function() M.today:get() return M.cells(offset:get()) end
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
  for index = 1, 42 do
    local cell = function() return cells()[index] end
    grid[#grid + 1] = ui.Item {
      width = cell_w, height = cell_h,
      ui.Rect {
        anchors = { center_in = true }, width = circle, height = circle, radius = circle / 2,
        color = function() return cell().today and C.accent() or "#00000000" end,
        behavior = { color = theme.behave("fast") },
        kit.text {
          anchors = { center_in = true },
          text = function() return tostring(cell().day) end,
          size = theme.size.small,
          weight = function() return cell().today and 600 or 400 end,
          color = function()
            local c = cell()
            if c.today then return C.accentText() end
            return c.in_month and C.text() or C.textMuted()
          end,
          opacity = function() return cell().in_month and 1 or 0.35 end,
        },
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
  return controls.card {
    bare = options.bare, padding = padding, width = width, height = height,
    ui.Column {
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
