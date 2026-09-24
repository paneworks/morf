-- Pending package updates: the repositories and the AUR.
--
-- Port of UpdatesService.qml and scripts/updates.py. `checkupdates`
-- (pacman-contrib, which syncs a copy of the database) answers when it is
-- installed, else `pacman -Qu` reads the database as last synced; an AUR
-- helper (paru or yay) adds `-Qua`. All of these only read. Nothing is
-- installed from here: the packages panel opens a terminal for that.
--
-- The first check runs the first time something asks for a count, then
-- every thirty minutes while a module or the panel is watching.

local act = require("services.act")

local M = {}

M.POLL_MS = 30 * 60 * 1000

local pacman = act.which("pacman")
local checkupdates = act.which("checkupdates")
local helper_name, helper = nil, nil
for _, name in ipairs { "paru", "yay" } do
  local path = act.which(name)
  if path then helper_name, helper = name, path break end
end
M.helper = helper_name or ""

local s = {
  revision = morf.signal("impasto.updates.revision", 0),
  checking = morf.signal("impasto.updates.checking", false),
  checked_at = morf.signal("impasto.updates.checked_at", 0),
  minute = morf.signal("impasto.updates.minute", 0),
}
M.signals = s

local state = { updates = {}, aur = false, tool = "" }
local asked = false

function M.available() return pacman ~= nil end
function M.checking() return s.checking:get() end
function M.tool() s.revision:get() return state.tool end
function M.aur() s.revision:get() return state.aur end

local function ask()
  if asked then return end
  asked = true
  morf.timer(1, function() M.refresh() end, false)
end

function M.updates() ask() s.revision:get() return state.updates end
function M.count() return #M.updates() end

--- The first few names, for the module's one line.
function M.names(limit)
  local out = {}
  for index, row in ipairs(M.updates()) do
    if index > (limit or 8) then break end
    out[#out + 1] = row.name
  end
  return out
end

--- "just now", "5 min ago", "1 h 20 min ago".
function M.age()
  s.minute:get()
  local at = s.checked_at:get()
  if at <= 0 then return "" end
  local minutes = math.max(0, math.floor((morf.time.now_ms() - at) / 60000 + 0.5))
  if minutes < 1 then return "just now" end
  if minutes < 60 then return minutes .. " min ago" end
  return ("%d h %d min ago"):format(minutes // 60, minutes % 60)
end

-- "name 1.0-1 -> 1.1-1", as both checkupdates and -Qu print it.
local function parse(text, source, into)
  for line in tostring(text or ""):gmatch("[^\n]+") do
    local name, from, to = line:match("^(%S+)%s+(%S+)%s+%->%s+(%S+)")
    if name then into[#into + 1] = { name = name, from = from, to = to, source = source } end
  end
end

function M.refresh()
  asked = true
  if s.checking:get() or not pacman then return end
  s.checking:set(true)
  local rows, pending = {}, helper and 2 or 1
  local aur_answered = false
  local tool = checkupdates and "checkupdates" or "pacman"
  local function done()
    pending = pending - 1
    if pending > 0 then return end
    table.sort(rows, function(a, b) return a.name < b.name end)
    state.updates, state.aur, state.tool = rows, aur_answered, tool
    s.checking:set(false)
    s.checked_at:set(morf.time.now_ms())
    s.revision:set(s.revision:get() + 1)
  end
  local argv = checkupdates and { checkupdates, "--nocolor" } or { pacman, "-Qu" }
  morf.run(argv, { timeout_ms = 120000 }, function(result)
    parse(result.stdout, "repo", rows)
    done()
  end)
  if helper then
    morf.run({ helper, "-Qua" }, { timeout_ms = 120000 }, function(result)
      -- Exit 1 with nothing printed is "nothing to update", not offline.
      aur_answered = result.ok or (result.code == 1 and (result.stderr or "") == "")
      parse(result.stdout, "aur", rows)
      done()
    end)
  end
end

-- Watchers: the module on the bar, the panel. While any is there, the
-- count is checked every half hour and the age moves every minute.
local watchers, poller, ager = 0, nil, nil
function M.subscribe()
  watchers = watchers + 1
  ask()
  if not poller then
    poller = morf.timer(M.POLL_MS, function() M.refresh() end, true)
    ager = morf.timer(60000, function() s.minute:set(s.minute:get() + 1) end, true)
  end
end
function M.release()
  watchers = math.max(0, watchers - 1)
  if watchers == 0 and poller then
    poller:cancel() ager:cancel()
    poller, ager = nil, nil
  end
end

--- A test bench's list, so the panel and module can be looked at on a
--- machine with nothing pending (or no pacman at all).
function M.sample()
  state.updates = {
    { name = "linux", from = "6.16.7.arch1-1", to = "6.16.8.arch1-1", source = "repo" },
    { name = "mesa", from = "1:25.2.2-1", to = "1:25.2.3-1", source = "repo" },
    { name = "firefox", from = "143.0-1", to = "143.0.1-1", source = "repo" },
    { name = "hyprland", from = "0.51.0-1", to = "0.51.1-1", source = "repo" },
    { name = "python", from = "3.13.7-1", to = "3.13.7-2", source = "repo" },
    { name = "visual-studio-code-bin", from = "1.104.0-1", to = "1.104.1-1", source = "aur" },
  }
  state.aur, state.tool = helper ~= nil, checkupdates and "checkupdates" or "pacman"
  asked = true
  s.checked_at:set(morf.time.now_ms())
  s.revision:set(s.revision:get() + 1)
end

return M
