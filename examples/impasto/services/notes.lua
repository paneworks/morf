-- Notes: sticky notes, a title, a body and a tint, and the one that is open.
--
-- Port of NotesService.qml. No state and no date: those are tasks
-- (`services/tasks.lua`), and the two are independent. Shown by the notes
-- panel, the bar's notes module and the edge decks; edited only in the
-- island, since nothing else takes the keyboard.
--
-- Stored as one JSON file, `notes.json` in the state directory, written a
-- moment after the last change. A note is archived before it is deleted.
--
-- The collection is a plain Lua list behind one revision signal: a binding
-- that reads any of it (`notes.live()`, `notes.entry(key)`) follows every
-- change, and the list is small enough that re-reading it is nothing.

local theme = require("theme")

local json = morf.json
local fs = morf.fs

local M = {}

-- ----------------------------------------------------------------- paper --

-- A palette token (never a hex) washed towards white, so the paper follows
-- the wallpaper. The ink is fixed (`theme.color.paperInk`).
M.tints = { "yellow", "accent", "green", "blue", "red" }

local tint_names = { yellow = true, accent = true, green = true, blue = true, red = true }

--- The tint's palette colour, as a value; a binding follows the palette.
function M.tint_color(name)
  if name == "green" or name == "yellow" or name == "red" or name == "blue" then
    return theme.color[name]()
  end
  return theme.color.accent()
end

-- Qt.tint(colour, paperWash): the wash (white at 77 %) laid over the tint.
local wash = morf.color("#ffffffc4")
--- The note's paper: its tint under a white wash.
function M.paper_of(name)
  local base = M.tint_color(name or "yellow")
  local a = wash.a
  local function channel(c, w)
    return math.floor(math.max(0, math.min(1, c * (1 - a) + w * a)) * 255 + 0.5)
  end
  return morf.color(string.format("#%02x%02x%02x",
    channel(base.r, wash.r), channel(base.g, wash.g), channel(base.b, wash.b)))
end

-- ------------------------------------------------------------ collection --
--
--   key       unique id, e.g. "note-m2k9x1"
--   title
--   text      body
--   tint      one of `tints`
--   created   ms since epoch
--   edited    ms since epoch; the deck sorts by it
--   archived  hidden from the deck and the edges

M.path = fs.join(fs.dir("state") or (fs.home() .. "/.local/state"), "impasto-morf", "notes.json")

local rows = {}
local revision = morf.signal("impasto.notes.revision", 0)
local opened = morf.signal("impasto.notes.opened", "")
local direct = morf.signal("impasto.notes.direct", false)

-- morf.json hands back tagged tables; the rows are plain.
local function plain(value)
  if type(value) ~= "table" then return value end
  local out = {}
  for k, v in pairs(value) do out[k] = plain(v) end
  return out
end

local function normalise(list)
  local out = {}
  for _, kept in ipairs(list or {}) do
    if type(kept) == "table" and type(kept.key) == "string" and kept.key ~= "" then
      out[#out + 1] = {
        key = kept.key,
        title = type(kept.title) == "string" and kept.title or "",
        text = type(kept.text) == "string" and kept.text or "",
        tint = tint_names[kept.tint] and kept.tint or "yellow",
        created = tonumber(kept.created) or 0,
        edited = tonumber(kept.edited) or 0,
        archived = kept.archived == true,
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
    morf.log("warn", "impasto: notes.json is not JSON; starting empty")
    return
  end
  decoded = plain(decoded)
  rows = normalise(decoded.notes or decoded)
end

local saving = false
local function save()
  saving = false
  local list = {}
  for i, row in ipairs(rows) do list[i] = row end
  local ok, err = fs.write(M.path, json.encode({ notes = json.array(list) }, { pretty = true }))
  if not ok then morf.log("warn", "impasto: could not save notes: " .. tostring(err)) end
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
  return nil
end

local function newest_first(a, b)
  if a.edited ~= b.edited then return a.edited > b.edited end
  return a.key > b.key
end

--- Every note, archived or not; a binding that calls it follows the notes.
function M.all()
  revision:get()
  return rows
end

--- Notes on the deck, most recently edited first.
function M.live()
  revision:get()
  local out = {}
  for _, row in ipairs(rows) do if not row.archived then out[#out + 1] = row end end
  table.sort(out, newest_first)
  return out
end

function M.archived()
  revision:get()
  local out = {}
  for _, row in ipairs(rows) do if row.archived then out[#out + 1] = row end end
  table.sort(out, newest_first)
  return out
end

function M.count() return #M.live() end
function M.newest() return M.live()[1] end

--- The note with this key, or nil.
function M.entry(key)
  revision:get()
  if not key or key == "" then return nil end
  return (find(key))
end

-- ---------------------------------------------------------------- reading --

function M.first_line(text)
  for line in tostring(text or ""):gmatch("[^\n]+") do
    local trimmed = line:match("^%s*(.-)%s*$")
    if trimmed ~= "" then return trimmed end
  end
  return ""
end

--- Title, else first line, else "Untitled".
function M.title_of(note)
  if not note then return "" end
  local title = (note.title or ""):match("^%s*(.-)%s*$")
  if title ~= "" then return title end
  local line = M.first_line(note.text)
  return line ~= "" and line or "Untitled"
end

function M.is_empty(note)
  return not note or ((note.title or ""):match("^%s*(.-)%s*$") == ""
    and (note.text or ""):match("^%s*(.-)%s*$") == "")
end

--- Lines starting `[ ]` or `[x]` show as boxes. Display only.
function M.display(text)
  local out = {}
  for line in (tostring(text or "") .. "\n"):gmatch("(.-)\n") do
    line = line:gsub("^%[[xX]%] ?", "󰄲 "):gsub("^%[ %] ?", "󰄱 ")
    out[#out + 1] = line
  end
  return table.concat(out, "\n")
end

-- A minute's tick for ages, started by the first binding that shows one.
local minute = morf.signal("impasto.notes.minute", 0)
local ticking = false
local function tick()
  if ticking then return end
  ticking = true
  morf.timer(60000, function() minute:set(minute:get() + 1) end, true)
end

--- "just now", "5 min", "3 h", "2 d", else the date.
function M.age_of(when)
  tick()
  minute:get()
  local seconds = math.max(0, (morf.time.now_ms() - (when or 0)) / 1000)
  if seconds < 60 then return "just now" end
  if seconds < 3600 then return math.floor(seconds / 60) .. " min" end
  if seconds < 86400 then return math.floor(seconds / 3600) .. " h" end
  if seconds < 7 * 86400 then return math.floor(seconds / 86400) .. " d" end
  return morf.time.format("%-d %b", (when or 0) / 1000)
end

-- ---------------------------------------------------------------- writing --

M.on_added = {}

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
  local key = "note-" .. stamp
  local n = 2
  while find(key) do key = "note-" .. stamp .. "-" .. n n = n + 1 end
  return key
end

function M.add(title, text, tint)
  local now = morf.time.now_ms()
  local key = new_key()
  rows[#rows + 1] = {
    key = key, title = title or "", text = text or "",
    tint = tint_names[tint] and tint or "yellow",
    created = now, edited = now, archived = false,
  }
  write()
  for _, fn in ipairs(M.on_added) do fn(key) end
  return key
end

--- A blank note opened for writing; dropped if left empty (`leave`).
function M.create(tint, from_deck)
  local key = M.add("", "", tint or "yellow")
  opened:set(key)
  direct:set(not from_deck)
  return key
end

--- Only text changes bump `edited` and bring the note to the front.
function M.update(key, changes)
  local row = find(key)
  if not row then return end
  local written = (changes.text ~= nil and changes.text ~= row.text)
    or (changes.title ~= nil and changes.title ~= row.title)
  local moved = false
  for k, v in pairs(changes) do
    if row[k] ~= v then row[k] = v moved = true end
  end
  if written then row.edited = morf.time.now_ms() end
  if moved then write() end
end

function M.set_tint(key, tint)
  if tint_names[tint] then M.update(key, { tint = tint }) end
end

-- Whoever places notes (the edge decks) hears a note leave.
M.on_removed = {}

--- An archived note also leaves the edges.
function M.archive(key, on)
  if on == nil then on = true end
  if not find(key) then return end
  M.update(key, { archived = on })
  if on then for _, fn in ipairs(M.on_removed) do fn(key) end end
end

function M.remove(key)
  local _, index = find(key)
  if not index then return end
  for _, fn in ipairs(M.on_removed) do fn(key) end
  table.remove(rows, index)
  write()
  if opened:get() == key then opened:set("") end
end

-- ------------------------------------------------------------------ panel --
--
-- Declared here so the island reaches the panel's size before it exists.
M.panel_width = 560
M.panel_height = 520

--- The open note's key, or "" for the deck. Kept here because the panel is
--- destroyed on close, and the edges open straight onto a note.
function M.opened() return opened:get() end

--- Opened from outside the deck (an edge tab, the module, IPC): going back
--- then closes the island instead of returning to the deck.
function M.direct() return direct:get() end

function M.open(key, from_deck)
  local want = find(key) and key or ""
  direct:set(not from_deck)
  local now = opened:get()
  if now ~= "" and want ~= "" and now ~= want then
    -- From one note straight to another: through the deck for a tick, so
    -- the sheet is built afresh for the new one.
    M.leave()
    direct:set(not from_deck)
    morf.timer(30, function() opened:set(want) end, false)
    return
  end
  opened:set(want)
end

--- Back to the deck, discarding the note if it was never written on.
function M.leave()
  local note = find(opened:get())
  opened:set("")
  direct:set(false)
  if note and M.is_empty(note) then M.remove(note.key) end
end

load()

return M
