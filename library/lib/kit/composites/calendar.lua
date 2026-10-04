-- A calendar (composite: Selection days + Navigation months), inline.
--
--     local node, cal = composites.calendar {
--       id = "planner", width = 280,
--       value = function() return day:get() end,        -- "YYYY-MM-DD"
--       on_changed = function(date) day:set(date) end,  -- the current day moved
--       on_picked = function(date) end,                 -- a press or Return chose it
--       marked = function(date) return count(date) > 0 end,
--     }
--     cal.step(1) ; cal.show("2026-12") ; cal.month() --> "2026-10"
--
-- The days are a kit `day_grid` (a Selection: the arrows walk the days,
-- typing a number jumps to it, Return picks), one per month, in a kit
-- Navigation that slides one month in as the other goes. The arrows walk
-- across a month's edge into the next; Page Up and Page Down turn the
-- month (Shift: the year), keeping the day; Home and End go to the first
-- and last of the month.
--
-- `month` (a month index -- year * 12 + month - 1 -- or "YYYY-MM", or a
-- binding to either) makes the shown month the configuration's: the
-- arrows by the title then only ask, through `on_month(month, delta)`.
-- `first_weekday` (1 Monday .. 7 Sunday), `cell_height`, `header_height`,
-- `band` (a binding to `from, to` dates drawn as a run, for a range) and
-- `press_activates` (a press picks, not only moves) are optional; the
-- calendar is as tall as the month's weeks unless `fit = false`. Ids:
-- `<id>-previous`, `<id>-next`, `<id>-month-title`, `<id>-day-<date>`.
local ui = require("morf.ui")
local widgets = require("lib.kit.widgets")

local M = {}

-- ------------------------------------------------------------- dates --

local function leap(y) return (y % 4 == 0 and y % 100 ~= 0) or y % 400 == 0 end
local LENGTHS = { 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31 }
function M.days_in(y, m) return (m == 2 and leap(y)) and 29 or LENGTHS[m] end

-- Days since 1970-01-01 of a civil date, and back (Howard Hinnant's).
local function days_from_civil(y, m, d)
  y = m <= 2 and y - 1 or y
  local era = (y >= 0 and y or y - 399) // 400
  local yoe = y - era * 400
  local doy = (153 * (m + (m > 2 and -3 or 9)) + 2) // 5 + d - 1
  local doe = yoe * 365 + yoe // 4 - yoe // 100 + doy
  return era * 146097 + doe - 719468
end
local function civil_from_days(z)
  z = z + 719468
  local era = (z >= 0 and z or z - 146096) // 146097
  local doe = z - era * 146097
  local yoe = (doe - doe // 1460 + doe // 36524 - doe // 146096) // 365
  local y = yoe + era * 400
  local doy = doe - (365 * yoe + yoe // 4 - yoe // 100)
  local mp = (5 * doy + 2) // 153
  local d = doy - (153 * mp + 2) // 5 + 1
  local m = mp + (mp < 10 and 3 or -9)
  return m <= 2 and y + 1 or y, m, d
end

--- 1 Monday .. 7 Sunday.
function M.weekday(y, m, d) return (days_from_civil(y, m, d) + 3) % 7 + 1 end
function M.key(y, m, d) return ("%04d-%02d-%02d"):format(y, m, d) end
function M.parse(date)
  if type(date) ~= "string" then return nil end
  local y, m, d = date:match("^(%d%d%d%d)%-(%d%d)%-(%d%d)")
  y, m, d = tonumber(y), tonumber(m), tonumber(d)
  if not y or m < 1 or m > 12 or d < 1 or d > M.days_in(y, m) then return nil end
  return y, m, d
end
function M.add_days(date, n)
  local y, m, d = M.parse(date)
  if not y then return date end
  return M.key(civil_from_days(days_from_civil(y, m, d) + n))
end
--- A month index: year * 12 + month - 1.
function M.month_index(y, m) return y * 12 + m - 1 end
function M.month_of(index) return index // 12, index % 12 + 1 end
function M.add_months(date, n)
  local y, m, d = M.parse(date)
  if not y then return date end
  local ny, nm = M.month_of(M.month_index(y, m) + n)
  return M.key(ny, nm, math.min(d, M.days_in(ny, nm)))
end
function M.today() return morf.time.format("%Y-%m-%d") end
--- "October 2026", in the locale's words.
function M.month_title(index)
  local y, m = M.month_of(index)
  return morf.time.format("%B %Y", morf.time.time { year = y, month = m, day = 1, hour = 12 })
end
--- "3 Oct 2026" (or `format`, strftime's).
function M.format(date, format)
  local y, m, d = M.parse(date)
  if not y then return "" end
  return morf.time.format(format or "%d %b %Y", morf.time.time { year = y, month = m, day = d, hour = 12 })
end
local function as_index(v)
  if type(v) == "number" then return math.floor(v) end
  if type(v) == "string" then
    local y, m = v:match("^(%d%d%d%d)%-(%d%d)")
    if y then return M.month_index(tonumber(y), tonumber(m)) end
  end
  return nil
end
local function get(v) if type(v) == "function" then return v() end return v end

-- ------------------------------------------------------------ the panel --

local PAGES = { "a", "b", "c" }
-- A month always shows on the same one of three pages (its index mod 3),
-- so the month before and after are always on the other two and the
-- Navigation's next and previous are the month's.
local function page_of(index) return PAGES[index % 3 + 1] end

function M.make(spec)
  spec = spec or {}
  local kit = require("kit")
  local id = spec.id
  local first = spec.first_weekday or 1
  local W = spec.width or 280
  local cw = math.floor(W / 7)
  local ch = spec.cell_height or math.min(cw, 36)
  local HEAD = spec.header_height or 36
  local WEEK = 22
  local GRID = 6 * ch
  local H = HEAD + 4 + WEEK + GRID
  local sy, sm = M.parse(M.parse(get(spec.value)) and get(spec.value) or M.today())
  local st = morf.state { selected = M.parse(get(spec.value)) and get(spec.value) or "",
    month = as_index(get(spec.month)) or M.month_index(sy, sm), today = M.today() }
  local page_month = morf.state { a = 0, b = 0, c = 0 }
  page_month[page_of(st.month)] = st.month
  local nav, grids = nil, {}

  -- One month's cells: blanks before the first, then its days.
  local cache = {}
  local function cells(index)
    local hit = cache[index]
    if hit then return hit end
    local y, m = M.month_of(index)
    local out, blanks, at = {}, {}, {}
    for _ = 1, (M.weekday(y, m, 1) - first) % 7 do
      out[#out + 1] = { label = "" }
      blanks[#blanks + 1] = #out
    end
    for d = 1, M.days_in(y, m) do
      local date = M.key(y, m, d)
      out[#out + 1] = { label = tostring(d), date = date }
      at[date] = #out
    end
    hit = { list = out, blanks = blanks, at = at }
    cache[index] = hit
    return hit
  end

  local function choose(date, picked)
    if not M.parse(date) then return end
    if st.selected ~= date then
      st.selected = date
      if spec.on_changed then spec.on_changed(date) end
    end
    if picked and spec.on_picked then spec.on_picked(date) end
  end
  -- Shows month `index`: the neighbouring page slides in.
  local function show(index)
    local now = st.month
    if index == now then return end
    local page = page_of(index)
    page_month[page] = index
    st.month = index
    if not nav then return end
    local old = grids[page_of(now)]
    local had_focus = old and old.focused
    if index == now + 1 then nav.next()
    elseif index == now - 1 then nav.previous()
    elseif page ~= page_of(now) then nav.go(page) end
    -- The keys go on to the month in sight, not the one leaving.
    if had_focus and grids[page] and grids[page] ~= old then morf.focus.set(grids[page], old.visual_focus) end
  end
  local function step(delta)
    local target = st.month + delta
    if spec.on_month then spec.on_month(("%04d-%02d"):format(M.month_of(target)), delta) end
    if spec.month == nil then show(target) end
  end
  local function focus_grid()
    local grid = grids[page_of(st.month)]
    if grid then morf.focus.set(grid, true) end
  end
  -- The keyboard across a month's edge: the day and its month.
  local function go_to(date)
    local y, m = M.parse(date)
    if not y then return end
    choose(date, false)
    local index = M.month_index(y, m)
    if index ~= st.month then
      if spec.on_month then spec.on_month(("%04d-%02d"):format(y, m), index - st.month) end
      if spec.month == nil then show(index) end
    end
    focus_grid()
  end

  local function grid_page(name)
    local function now() return cells(page_month[name]) end
    local look = ui.Item { width = W, height = GRID }
    -- A range's run under the days.
    if spec.band then
      for i = 1, 42 do
        local col, row = (i - 1) % 7, (i - 1) // 7
        ui.reparent(kit.surface { x = col * cw, y = row * ch + 2, width = cw, height = ch - 4,
          color = function() local c = kit.signal("accent")() return c:alpha(0.16) end,
          visible = function()
            local cell = now().list[i]
            if not (cell and cell.date) then return false end
            local from, to = spec.band()
            if not from or from == "" then return false end
            to = (to and to ~= "") and to or from
            if to < from then from, to = to, from end
            return cell.date >= from and cell.date <= to
          end }, look)
      end
    end
    local grid = widgets.day_grid {
      id = id and (id .. "-grid-" .. name), accessible_name = spec.accessible_name or "Days",
      items = function() return now().list end,
      columns = 7, gap = 0, item_width = cw, item_height = ch,
      press_activates = spec.press_activates,
      current = function() return now().at[st.selected] or 0 end,
      disabled = function() return now().blanks end,
      item_id = function(_, cell) return (id and cell.date) and (id .. "-day-" .. cell.date) or nil end,
      on_current_changed = function(i)
        local cell = now().list[i]
        if cell and cell.date then choose(cell.date, false) end
      end,
      on_activated = function(i)
        local cell = now().list[i]
        if cell and cell.date then choose(cell.date, true) end
      end,
    }
    grids[name] = grid
    ui.reparent(grid, look)
    -- Today, and the days the configuration marks.
    ui.reparent(kit.surface { width = cw - 4, height = 3, radius = kit.round(1.5),
      color = kit.signal("accent"),
      x = function() local i = now().at[st.today] return i and ((i - 1) % 7) * cw + 2 or 0 end,
      y = function() local i = now().at[st.today] return i and ((i - 1) // 7 + 1) * ch - 3 or 0 end,
      visible = function() return now().at[st.today] ~= nil end }, look)
    if spec.marked then
      for i = 1, 42 do
        local col, row = (i - 1) % 7, (i - 1) // 7
        ui.reparent(kit.surface { x = col * cw + cw / 2 - 2, y = row * ch + ch - 9, width = 4, height = 4,
          radius = kit.round(2), color = kit.ink("accent"),
          visible = function()
            local cell = now().list[i]
            return cell ~= nil and cell.date ~= nil and spec.marked(cell.date) and true or false
          end }, look)
      end
    end
    return look
  end

  local pages = {}
  for _, name in ipairs(PAGES) do pages[name] = function() return grid_page(name) end end
  local stack
  stack, nav = widgets.view_stack { id = id and (id .. "-months"), width = W, height = GRID,
    mode = "switcher", order = PAGES, wrap = true, current = page_of(st.month), pages = pages }
  local function arrow(days)
    return function()
      local y, m = M.parse(st.selected)
      if not y or M.month_index(y, m) ~= st.month then return false end
      local target = M.add_days(st.selected, days)
      local ty, tm = M.parse(target)
      -- Within the month the day grid walks itself.
      if ty == y and tm == m then return false end
      go_to(target)
      return true
    end
  end
  local function turn(months)
    return function()
      local y = M.parse(st.selected)
      if y then go_to(M.add_months(st.selected, months)) else step(months) end
      return true
    end
  end
  local days = ui.Item { y = HEAD + 4 + WEEK, width = W, height = GRID, stack,
    shortcuts = { Left = arrow(-1), Right = arrow(1), Up = arrow(-7), Down = arrow(7),
      Page_Up = turn(-1), Page_Down = turn(1), ["shift+Page_Up"] = turn(-12), ["shift+Page_Down"] = turn(12) } }

  local week = { y = HEAD + 4, gap = 0 }
  for i = 0, 6 do
    -- 2024-01-01 was a Monday.
    local name = morf.time.format("%a", morf.time.time { year = 2024, month = 1, day = 1 + (first - 1 + i) % 7, hour = 12 })
    -- Its first two characters, not bytes.
    local cut = utf8 and utf8.offset(name, 3)
    week[#week + 1] = kit.centred(cw, WEEK, kit.label { text = cut and name:sub(1, cut - 1) or name, color = kit.ink("lo") })
  end
  local header = ui.Item { width = W, height = HEAD,
    widgets.icon { id = id and (id .. "-previous"), accessible_name = "Previous month", width = 32, height = 32,
      size = 18, icon_off = "chevron_left", anchors = { left = true, vertical_center = true },
      on_clicked = function() step(-1) end },
    kit.text { id = id and (id .. "-month-title"), anchors = { center_in = true }, font_weight = 600,
      text = function() return M.month_title(st.month) end },
    widgets.icon { id = id and (id .. "-next"), accessible_name = "Next month", width = 32, height = 32,
      size = 18, icon_off = "chevron_right", anchors = { right = true, vertical_center = true },
      on_clicked = function() step(1) end },
  }
  -- As tall as the month's weeks (`fit = false`: always six).
  local function height()
    if spec.fit == false then return H end
    local y, m = M.month_of(st.month)
    local weeks = math.ceil(((M.weekday(y, m, 1) - first) % 7 + M.days_in(y, m)) / 7)
    return HEAD + 4 + WEEK + weeks * ch
  end
  local root = ui.Item { id = id, x = spec.x, y = spec.y, width = W, height = height, header, ui.Row(week), days }
  -- The configuration's day and month, when it keeps them.
  if type(spec.value) == "function" then
    morf.effect("kit.calendar.value." .. tostring(root), function()
      local v = spec.value()
      v = M.parse(v) and v or ""
      if v == st.selected then return end
      st.selected = v
      if spec.month == nil and v ~= "" then
        local y, m = M.parse(v)
        show(M.month_index(y, m))
      end
    end, { owner = root })
  end
  if type(spec.month) == "function" then
    morf.effect("kit.calendar.month." .. tostring(root), function()
      local index = as_index(spec.month())
      if index then show(index) end
    end, { owner = root })
  end
  local handle = { node = root, height = H, width = W }
  --- The height now, for a binding (`height` is the tallest, six weeks).
  function handle.height_now() return height() end
  function handle.step(delta) step(delta) end
  function handle.show(month) local index = as_index(month) if index then show(index) end end
  function handle.month() return ("%04d-%02d"):format(M.month_of(st.month)) end
  function handle.selected() return st.selected end
  function handle.select(date) if M.parse(date) then choose(date, false) local y, m = M.parse(date) show(M.month_index(y, m)) end end
  function handle.focus() focus_grid() end
  function handle.refresh_today() st.today = M.today() end
  return root, handle
end

return M
