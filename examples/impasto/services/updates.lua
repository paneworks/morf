-- Pending package updates: the repositories and the AUR.
--
-- Port of UpdatesService.qml and scripts/updates.py. `checkupdates`
-- (pacman-contrib, which syncs a copy of the database) answers when it is
-- installed and works; when it is missing or fails, `pacman -Qu` reads the
-- database as last synced. The AUR needs no helper: the foreign packages
-- (`pacman -Qm`) are looked up over the AUR's RPC with `morf.http`, in
-- batches, and compared the way pacman compares versions
-- (lib/packages.lua `vercmp`); offline, the AUR part is left out. All of
-- this only reads. Nothing is installed from here: the packages panel runs
-- pacman in a terminal window for that.
--
-- The first check runs the first time something asks for a count, then
-- every thirty minutes while a module or the panel is watching.

local act = require("services.act")

local M = {}

M.POLL_MS = 30 * 60 * 1000

local pacman = act.which("pacman")
local checkupdates = act.which("checkupdates")
local versions = require("lib.packages")

M.AUR_RPC = "https://aur.archlinux.org/rpc/v5/info"
M.AUR_BATCH = 150
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

--- The repositories: `callback(tool, rows)`, or (nil, nil) when neither
--- checkupdates nor pacman answered.
local function repositories(callback)
  local function with_pacman()
    morf.run({ pacman, "-Qu" }, { timeout_ms = 120000, env = { LANG = "C", LC_ALL = "C" } }, function(result)
      -- 1 with a silent stderr is pacman's "nothing to report".
      if result.ok or (result.code == 1 and (result.stderr or ""):match("^%s*$")) then
        local rows = {}
        parse(result.stdout, "repo", rows)
        return callback("pacman", rows)
      end
      callback(nil, nil)
    end)
  end
  if not checkupdates then return with_pacman() end
  morf.run({ checkupdates, "--nocolor" }, { timeout_ms = 120000, env = { LANG = "C", LC_ALL = "C" } },
    function(result)
      -- 2 is checkupdates for "no updates", which is an answer.
      if result.ok or result.code == 2 then
        local rows = {}
        parse(result.stdout, "repo", rows)
        return callback("checkupdates", rows)
      end
      morf.log("warn", "impasto: checkupdates failed: " .. tostring(result.stderr or result.error or ""):sub(1, 200))
      with_pacman()
    end)
end

--- Pairs of "name version" lines (`pacman -Qm`), as a map and sorted names.
function M.parse_foreign(text)
  local mine, names = {}, {}
  for line in tostring(text or ""):gmatch("[^\n]+") do
    local name, version = line:match("^(%S+)%s+(%S+)")
    if name then
      mine[name] = version
      names[#names + 1] = name
    end
  end
  table.sort(names)
  return mine, names
end

--- Foreign packages with a newer version on the AUR: `callback(rows)`, or
--- `callback(nil)` when the AUR did not answer. One the AUR has never heard
--- of is a local build, with nothing to compare against.
local function aur(callback)
  morf.run({ pacman, "-Qm" }, { timeout_ms = 30000 }, function(result)
    if not (result.ok or result.code == 1) then return callback({}) end
    local mine, names = M.parse_foreign(result.stdout)
    if #names == 0 then return callback({}) end
    if not morf.http then return callback(nil) end
    local remote, start = {}, 1
    local function ask()
      if start > #names then
        local rows = {}
        for _, name in ipairs(names) do
          local theirs = remote[name]
          if theirs and theirs ~= mine[name] and versions.vercmp(theirs, mine[name]) > 0 then
            rows[#rows + 1] = { name = name, from = mine[name], to = theirs, source = "aur" }
          end
        end
        return callback(rows)
      end
      local parts = {}
      for index = start, math.min(#names, start + M.AUR_BATCH - 1) do
        parts[#parts + 1] = "arg[]=" .. morf.http.url_encode(names[index])
      end
      start = start + M.AUR_BATCH
      local ok = pcall(morf.http.get, M.AUR_RPC .. "?" .. table.concat(parts, "&"),
        { timeout_ms = 8000, headers = { ["User-Agent"] = "impasto" } }, function(response)
          local data = response.ok and response.json and response.json() or nil
          if type(data) ~= "table" or type(data.results) ~= "table" then return callback(nil) end
          for _, row in ipairs(data.results) do
            if type(row.Name) == "string" and type(row.Version) == "string" then
              remote[row.Name] = row.Version
            end
          end
          ask()
        end)
      if not ok then callback(nil) end
    end
    ask()
  end)
end

function M.refresh()
  asked = true
  if s.checking:get() or not pacman then return end
  s.checking:set(true)
  local rows, pending = {}, 2
  local aur_answered, tool = false, ""
  local function done()
    pending = pending - 1
    if pending > 0 then return end
    table.sort(rows, function(a, b) return a.name < b.name end)
    state.updates, state.aur, state.tool = rows, aur_answered, tool
    s.checking:set(false)
    s.checked_at:set(morf.time.now_ms())
    s.revision:set(s.revision:get() + 1)
  end
  repositories(function(used, found)
    tool = used or ""
    for _, row in ipairs(found or {}) do rows[#rows + 1] = row end
    done()
  end)
  aur(function(found)
    aur_answered = found ~= nil
    for _, row in ipairs(found or {}) do rows[#rows + 1] = row end
    done()
  end)
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
  state.aur, state.tool = true, checkupdates and "checkupdates" or "pacman"
  asked = true
  s.checked_at:set(morf.time.now_ms())
  s.revision:set(s.revision:get() + 1)
end

return M
