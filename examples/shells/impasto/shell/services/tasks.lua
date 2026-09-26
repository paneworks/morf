-- Tasks: a line, a state (to do, doing, done) and an optional due day.
--
-- Port of TasksService.qml. The board lays them out by state and the
-- calendar marks them by day; both read the same list. Unlike notes, tasks
-- have a state and a date.
--
-- Stored as `tasks.json` in the state directory, written a moment after the
-- last change. Days are `YYYY-MM-DD` strings, so they compare as text.

local json = morf.json
local fs = morf.fs
local T = morf.time

local M = {}

-- ----------------------------------------------------------------- states --

M.states = {
  { id = "todo", label = "To do", icon = "󰄰" },
  { id = "doing", label = "Doing", icon = "󰪡" },
  { id = "done", label = "Done", icon = "󰄲" },
}

local state_ids = { todo = 1, doing = 2, done = 3 }

function M.state_entry(id) return M.states[state_ids[id] or 1] end

function M.state_after(id)
  local at = state_ids[id] or 1
  return M.states[math.min(#M.states, at + 1)].id
end

-- ------------------------------------------------------------------ today --

-- The day changes at midnight, and a binding on `today()` has to hear it:
-- a minute's tick, started by the first reader.
local minute = morf.signal("impasto.tasks.minute", 0)
local ticking = false
local function tick()
  if ticking then return end
  ticking = true
  morf.timer(60000, function() minute:set(minute:get() + 1) end, true)
end

function M.day_key(seconds) return T.format("%Y-%m-%d", seconds) end

--- Seconds at the start of a day key, or nil.
function M.date_of(key)
  local y, m, d = tostring(key or ""):match("^(%d%d%d%d)%-(%d%d)%-(%d%d)$")
  if not y then return nil end
  local ok, at = pcall(T.time, { year = tonumber(y), month = tonumber(m), day = tonumber(d) })
  if not ok then return nil end
  -- A day that does not exist (31 Feb) rolls over; it is not that day.
  if M.day_key(at) ~= key then return nil end
  return at
end

function M.today()
  tick()
  minute:get()
  return M.day_key(T.now())
end

function M.shifted(days)
  tick()
  minute:get()
  return M.day_key(T.add(T.now(), { days = days }))
end

-- ------------------------------------------------------------- collection --
--
--   key       unique id, e.g. "task-m2k9x1"
--   text      the line
--   body      details, or ""
--   state     one of `states`
--   due       a day key, or "" for none
--   rank      order within its lane, lowest first
--   created   ms since epoch
--   finished  when it reached done, or 0

M.path = fs.join(fs.dir("state") or (fs.home() .. "/.local/state"), "impasto-morf", "tasks.json")

local rows = {}
local revision = morf.signal("impasto.tasks.revision", 0)
local opened = morf.signal("impasto.tasks.opened", "")
local direct = morf.signal("impasto.tasks.direct", false)

local function plain(value)
  if type(value) ~= "table" then return value end
  local out = {}
  for k, v in pairs(value) do out[k] = plain(v) end
  return out
end

local function normalise(list)
  local out = {}
  for index, kept in ipairs(list or {}) do
    if type(kept) == "table" and type(kept.key) == "string" and kept.key ~= "" then
      out[#out + 1] = {
        key = kept.key,
        text = type(kept.text) == "string" and kept.text or "",
        body = type(kept.body) == "string" and kept.body or "",
        state = state_ids[kept.state] and kept.state or "todo",
        due = type(kept.due) == "string" and kept.due or "",
        rank = tonumber(kept.rank) or index,
        created = tonumber(kept.created) or 0,
        finished = tonumber(kept.finished) or 0,
      }
    end
  end
  return out
end

local function load()
  local text = fs.read(M.path)
  if not text or text == "" then return end
  local ok, decoded = pcall(json.decode, text)
  if not ok or type(decoded) ~= "table" then
    morf.log("warn", "impasto: tasks.json is not JSON; starting empty")
    return
  end
  decoded = plain(decoded)
  rows = normalise(decoded.tasks or decoded)
end

local saving = false
local function save()
  saving = false
  local list = {}
  for i, row in ipairs(rows) do list[i] = row end
  local ok, err = fs.write(M.path, json.encode({ tasks = json.array(list) }, true))
  if not ok then morf.log("warn", "impasto: could not save tasks: " .. tostring(err)) end
end

local function write()
  revision:set(revision:get() + 1)
  if not saving then
    saving = true
    morf.timer(120, save, false)
  end
end

local function find(key)
  for i, row in ipairs(rows) do
    if row.key == key then return row, i end
  end
end

local function by_rank(a, b)
  if a.rank ~= b.rank then return a.rank < b.rank end
  if a.created ~= b.created then return a.created < b.created end
  return a.key < b.key
end

local function filtered(keep, order)
  revision:get()
  local out = {}
  for _, row in ipairs(rows) do if keep(row) then out[#out + 1] = row end end
  table.sort(out, order or by_rank)
  return out
end

function M.all() revision:get() return rows end

function M.entry(key)
  revision:get()
  if not key or key == "" then return nil end
  return (find(key))
end

--- One lane of the board, top to bottom.
function M.in_state(state)
  return filtered(function(row) return row.state == state end)
end

function M.count() revision:get() return #rows end

function M.pending()
  return #filtered(function(row) return row.state ~= "done" end)
end

-- ----------------------------------------------------------------- by day --

--- Due on a day: unfinished first, then in board order.
function M.on(day)
  return filtered(function(row) return row.due == day end, function(a, b)
    local da, db = a.state == "done" and 1 or 0, b.state == "done" and 1 or 0
    if da ~= db then return da < db end
    return by_rank(a, b)
  end)
end

function M.count_on(day) return #M.on(day) end

function M.pending_on(day)
  return #filtered(function(row) return row.due == day and row.state ~= "done" end)
end

function M.is_overdue(task)
  return task and task.due ~= "" and task.state ~= "done" and task.due < M.today() or false
end

local function by_due(a, b)
  if a.due ~= b.due then return a.due < b.due end
  return by_rank(a, b)
end

function M.overdue()
  local today = M.today()
  return filtered(function(row) return row.due ~= "" and row.state ~= "done" and row.due < today end, by_due)
end

--- Unfinished dated tasks from today on.
function M.upcoming()
  local today = M.today()
  return filtered(function(row) return row.due ~= "" and row.state ~= "done" and row.due >= today end, by_due)
end

function M.next() return M.upcoming()[1] end

--- Unfinished tasks: dated ones by day (overdue first), then undated ones
--- in board order.
function M.queue()
  return filtered(function(row) return row.state ~= "done" end, function(a, b)
    if a.due ~= "" and b.due ~= "" then return by_due(a, b) end
    if a.due ~= "" then return true end
    if b.due ~= "" then return false end
    return by_rank(a, b)
  end)
end

--- For the calendar: `{ [day] = { count = n, pending = m } }` for every day
--- of that month that has tasks, so a month grid can put a dot under it
--- (accent while any is open, muted once all are done). A binding that
--- calls it follows the tasks.
function M.days_with_tasks(year, month)
  revision:get()
  local prefix = string.format("%04d-%02d-", year, month)
  local out = {}
  for _, row in ipairs(rows) do
    if row.due:sub(1, 8) == prefix then
      local day = tonumber(row.due:sub(9, 10))
      if day then
        local entry = out[day] or { count = 0, pending = 0 }
        entry.count = entry.count + 1
        if row.state ~= "done" then entry.pending = entry.pending + 1 end
        out[day] = entry
      end
    end
  end
  return out
end

--- today, tomorrow, yesterday, a weekday within a week, else the date.
function M.due_label(day)
  if not day or day == "" then return "" end
  local today = M.today()
  if day == today then return "today" end
  if day == M.shifted(1) then return "tomorrow" end
  if day == M.shifted(-1) then return "yesterday" end
  local at = M.date_of(day)
  if not at then return day end
  if day > today and day <= M.shifted(6) then return T.format("%A", at) end
  if day:sub(1, 4) == today:sub(1, 4) then return T.format("%a %-d %b", at) end
  return T.format("%-d %b %Y", at)
end

--- "Pay rent · tomorrow", "2 overdue", "3 due today", ...
function M.summary()
  local late = #M.overdue()
  if late > 0 then return late .. " overdue" end
  local today = M.pending_on(M.today())
  if today > 0 then return today .. " due today" end
  local next = M.next()
  if next then return next.text .. " · " .. M.due_label(next.due) end
  return M.pending() > 0 and "nothing dated" or "nothing to do"
end

-- ------------------------------------------------------------ parsing days --
--
-- Accepted after `@`: today, tomorrow, a weekday (the next one, today
-- included), `+3`, `12/9`, `12 sep`, `sep 12` or a day key, in English and
-- Spanish.

local day_names = {
  { "mon", "monday", "lun", "lunes" },
  { "tue", "tuesday", "mar", "martes" },
  { "wed", "wednesday", "mie", "mié", "miercoles", "miércoles" },
  { "thu", "thursday", "jue", "jueves" },
  { "fri", "friday", "vie", "viernes" },
  { "sat", "saturday", "sab", "sáb", "sabado", "sábado" },
  { "sun", "sunday", "dom", "domingo" },
}

local month_names = {
  { "jan", "january", "ene", "enero" }, { "feb", "february", "febrero" },
  { "mar", "march", "marzo" }, { "apr", "april", "abr", "abril" },
  { "may", "mayo" }, { "jun", "june", "junio" }, { "jul", "july", "julio" },
  { "aug", "august", "ago", "agosto" }, { "sep", "sept", "september", "septiembre" },
  { "oct", "october", "octubre" }, { "nov", "november", "noviembre" },
  { "dec", "december", "dic", "diciembre" },
}

local function index_in(lists, word)
  for i, names in ipairs(lists) do
    for _, name in ipairs(names) do if name == word then return i end end
  end
end

local function build(day, month, year)
  local given = year ~= nil
  local now = T.date()
  year = year or now.year
  local key = string.format("%04d-%02d-%02d", year, month, day)
  if not M.date_of(key) then return "" end
  -- A day and a month with no year is the next one of those.
  if not given and key < M.today() then key = string.format("%04d-%02d-%02d", year + 1, month, day) end
  return M.date_of(key) and key or ""
end

--- A day key for what was typed, or "" when it is not a day.
function M.parse_due(text)
  local word = tostring(text or ""):match("^%s*(.-)%s*$"):lower()
  if word == "" then return "" end
  if word:match("^%d%d%d%d%-%d%d%-%d%d$") then return M.date_of(word) and word or "" end
  if word == "today" or word == "tod" or word == "hoy" then return M.today() end
  if word == "tomorrow" or word == "tom" or word == "mañana" or word == "manana" then return M.shifted(1) end
  if word == "next week" or word == "nextweek" then return M.shifted(7) end
  local plus = word:match("^%+(%d%d?%d?)$")
  if plus then return M.shifted(tonumber(plus)) end
  local weekday = index_in(day_names, word)
  if weekday then
    local now = T.date().weekday -- 1 Monday .. 7 Sunday
    return M.shifted((weekday - now + 7) % 7)
  end
  local d, m, y = word:match("^(%d%d?)[/.](%d%d?)[/.](%d%d%d?%d?)$")
  if not d then d, m = word:match("^(%d%d?)[/.](%d%d?)$") end
  if d then
    y = y and tonumber(y)
    if y and y < 100 then y = 2000 + y end
    return build(tonumber(d), tonumber(m), y)
  end
  local parts = {}
  for part in word:gmatch("%S+") do parts[#parts + 1] = part end
  if #parts == 2 or #parts == 3 then
    local y = parts[3] and tonumber(parts[3])
    if parts[3] and not y then return "" end
    local a, b = tonumber(parts[1]), tonumber(parts[2])
    if a and index_in(month_names, parts[2]) then return build(a, index_in(month_names, parts[2]), y) end
    if b and index_in(month_names, parts[1]) then return build(b, index_in(month_names, parts[1]), y) end
  end
  return ""
end

--- "Pay rent @fri" -> text and due day. An `@` that is not a day stays.
function M.split(line)
  local text = tostring(line or ""):match("^%s*(.-)%s*$")
  local at = text:match(".*()@")
  if at and at > 1 then
    local due = M.parse_due(text:sub(at + 1))
    if due ~= "" then return { text = text:sub(1, at - 1):match("^%s*(.-)%s*$"), due = due } end
  end
  return { text = text, due = "" }
end

-- ---------------------------------------------------------------- writing --

local function base36(n)
  local digits = "0123456789abcdefghijklmnopqrstuvwxyz"
  local out = ""
  n = math.floor(n)
  repeat
    local d = n % 36
    out = digits:sub(d + 1, d + 1) .. out
    n = n // 36
  until n == 0
  return out
end

local function new_key()
  local stamp = base36(morf.time.now_ms())
  local key = "task-" .. stamp
  local n = 2
  while find(key) do key = "task-" .. stamp .. "-" .. n n = n + 1 end
  return key
end

local function last_rank(state)
  local lane = M.in_state(state)
  return #lane == 0 and 0 or lane[#lane].rank + 1
end

function M.add(text, due, state, body)
  state = state_ids[state] and state or "todo"
  local key = new_key()
  rows[#rows + 1] = {
    key = key, text = (text or ""):match("^%s*(.-)%s*$"), body = body or "",
    state = state, due = due or "", rank = last_rank(state),
    created = morf.time.now_ms(), finished = 0,
  }
  write()
  return key
end

--- A blank task opened for editing; dropped if left without a line.
function M.create(from_board)
  local key = M.add("", "", "todo", "")
  opened:set(key)
  direct:set(not from_board)
  return key
end

function M.is_empty(task)
  return not task or (task.text or ""):match("^%s*(.-)%s*$") == ""
end

function M.update(key, changes)
  local row = find(key)
  if not row then return end
  local moved = false
  for k, v in pairs(changes) do
    if row[k] ~= v then row[k] = v moved = true end
  end
  if moved then write() end
end

--- To the bottom of the lane; `finished` is kept only for done.
function M.set_state(key, state)
  local row = find(key)
  if not row or not state_ids[state] or row.state == state then return end
  M.update(key, {
    state = state, rank = last_rank(state),
    finished = state == "done" and morf.time.now_ms() or 0,
  })
end

--- Puts a task at `index` (1-based) in a lane, re-ranking the lane.
function M.place(key, state, index)
  local task = find(key)
  if not task or not state_ids[state] then return end
  local lane = {}
  for _, other in ipairs(M.in_state(state)) do
    if other.key ~= key then lane[#lane + 1] = other end
  end
  index = math.max(1, math.min(#lane + 1, index or (#lane + 1)))
  table.insert(lane, index, task)
  if task.state ~= state then
    task.state = state
    task.finished = state == "done" and morf.time.now_ms() or 0
  end
  for position, other in ipairs(lane) do other.rank = position - 1 end
  write()
end

function M.set_due(key, due) M.update(key, { due = due or "" }) end

--- Done, or back to the first lane.
function M.toggle(key)
  local task = find(key)
  if task then M.set_state(key, task.state == "done" and "todo" or "done") end
end

function M.remove(key)
  local _, index = find(key)
  if not index then return end
  table.remove(rows, index)
  write()
  if opened:get() == key then opened:set("") end
end

-- ------------------------------------------------------------------ panel --

M.panel_width = 760
M.panel_height = 520

function M.opened() return opened:get() end
function M.direct() return direct:get() end

function M.open(key, from_board)
  local want = find(key) and key or ""
  local now = opened:get()
  if now ~= "" and want ~= "" and now ~= want then
    -- From one task straight to another: through the board for a moment,
    -- so the sheet is built afresh for the new one.
    M.leave()
    direct:set(not from_board)
    morf.timer(30, function() opened:set(want) end, false)
    return
  end
  opened:set(want)
  direct:set(not from_board)
end

--- Back to the board, discarding a task that never got a line.
function M.leave()
  local task = find(opened:get())
  opened:set("")
  direct:set(false)
  if task and M.is_empty(task) then M.remove(task.key) end
end

load()

return M
