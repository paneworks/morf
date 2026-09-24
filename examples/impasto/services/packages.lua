-- What is installed, what is pending, what could be: pacman and the AUR,
-- read here for the packages panel.
--
-- Port of PackagesService.qml and scripts/packages.py. Everything here only
-- reads: `pacman -Qs` (every installed package with its description),
-- `-Qqe` (installed on purpose) and `-Qqm` (from outside the repositories),
-- `pacman -Ss` and the AUR's RPC for a search. When examples/lib/packages.lua
-- is present its readers are not assumed: this file speaks to pacman itself.
--
-- Install, Remove and Update run in a terminal, so pacman's questions, the
-- password and the output stay in front of the person who asked, and only
-- when they click. They go through `act.spawn` as changes to the machine,
-- so a dry run (IMPASTO_DRY_RUN=1) logs them and starts nothing. There is
-- no per-package upgrade: Arch does not do partial upgrades, so Update is
-- always `-Syu`.

local act = require("services.act")
local updates = require("services.updates")

local M = {}

M.panel_width = 820
M.panel_height = 560
M.field_height = 36
M.row_height = 46
M.footer_height = 30

M.views = {
  { id = "updates", label = "Updates" },
  { id = "installed", label = "Installed" },
  { id = "find", label = "Find" },
}

local pacman = act.which("pacman")
M.helper = updates.helper

local s = {
  view = morf.signal("impasto.packages.view", "find"),
  query = morf.signal("impasto.packages.query", ""),
  filter = morf.signal("impasto.packages.filter", "mine"),
  loaded = morf.signal("impasto.packages.loaded", false),
  loading = morf.signal("impasto.packages.loading", false),
  installed_rev = morf.signal("impasto.packages.installed", 0),
  found_rev = morf.signal("impasto.packages.found", 0),
  searching = morf.signal("impasto.packages.searching", false),
  busy = morf.signal("impasto.packages.busy", false),
}
M.signals = s

local installed, by_name = {}, {}
local found = { term = "", results = {}, repos = 0, aur = 0, note = "" }

function M.available() return pacman ~= nil end
function M.view() return s.view:get() end
function M.set_view(id) s.view:set(id) end
function M.query() return s.query:get() end
function M.set_query(text) s.query:set(text or "") end
function M.term() return s.query:get():match("^%s*(.-)%s*$"):lower() end
function M.filter() return s.filter:get() end
function M.set_filter(id) s.filter:set(id) end
function M.loaded() return s.loaded:get() end
function M.searching() return s.searching:get() end
function M.busy() return s.busy:get() end

function M.step(delta)
  local at = 1
  for index, entry in ipairs(M.views) do if entry.id == s.view:get() then at = index end end
  s.view:set(M.views[(at - 1 + delta) % #M.views + 1].id)
end

function M.installed() s.installed_rev:get() return installed end
function M.mine_count()
  local n = 0
  for _, row in ipairs(M.installed()) do if row.explicit then n = n + 1 end end
  return n
end
function M.aur_count()
  local n = 0
  for _, row in ipairs(M.installed()) do if row.aur then n = n + 1 end end
  return n
end

function M.found() s.found_rev:get() return found end

-- ------------------------------------------------------------- installed --

-- Long outputs are read a slice at a time from a timer: a handler has a
-- fixed budget of instructions, and a machine has thousands of packages.
local function chunked(text, per, each, finished)
  local position = 1
  local function step()
    local lines = 0
    while lines < per do
      local stop = text:find("\n", position, true)
      if not stop then
        if position <= #text then each(text:sub(position)) end
        position = #text + 1
        break
      end
      each(text:sub(position, stop - 1))
      position = stop + 1
      lines = lines + 1
    end
    if position > #text then finished() else morf.timer(1, step, false) end
  end
  step()
end

local function set_of(text)
  local out = {}
  for name in tostring(text or ""):gmatch("[^\n]+") do out[name] = true end
  return out
end

function M.load()
  if s.loading:get() or not pacman then return end
  s.loading:set(true)
  local pending, listing, explicit, foreign = 3, "", {}, {}
  local function all_in()
    pending = pending - 1
    if pending > 0 then return end
    local rows, current = {}, nil
    chunked(listing, 300, function(line)
      local name, version = line:match("^local/(%S+)%s+(%S+)")
      if name then
        current = { name = name, version = version, description = "",
          explicit = explicit[name] == true, aur = foreign[name] == true, installed = true }
        rows[#rows + 1] = current
      elseif current and line:match("^%s+") then
        current.description = line:match("^%s+(.-)%s*$")
      end
    end, function()
      installed = rows
      by_name = {}
      for _, row in ipairs(rows) do by_name[row.name] = row end
      s.loading:set(false)
      s.loaded:set(true)
      s.installed_rev:set(s.installed_rev:get() + 1)
    end)
  end
  morf.run({ pacman, "-Qs" }, { timeout_ms = 30000, max_output = 32 * 1024 * 1024 }, function(result)
    listing = result.stdout or ""
    all_in()
  end)
  morf.run({ pacman, "-Qqe" }, { timeout_ms = 30000 }, function(result)
    explicit = set_of(result.stdout)
    all_in()
  end)
  morf.run({ pacman, "-Qqm" }, { timeout_ms = 30000 }, function(result)
    foreign = set_of(result.stdout)
    all_in()
  end)
end

-- ------------------------------------------------------------------ search --

local search_generation = 0

local function aur_search(term, generation, callback)
  if not morf.http then return callback(nil, "offline") end
  local url = "https://aur.archlinux.org/rpc/v5/search/" .. morf.http.url_encode(term) .. "?by=name"
  morf.http.get(url, { timeout_ms = 10000 }, function(response)
    if generation ~= search_generation then return end
    local data = response.ok and response.json and response.json() or nil
    if type(data) ~= "table" then return callback(nil, "offline") end
    if data.type == "error" then
      -- Too many matches: the names that start with it, from the suggester.
      morf.http.get("https://aur.archlinux.org/rpc/v5/suggest/" .. morf.http.url_encode(term),
        { timeout_ms = 10000 }, function(more)
          if generation ~= search_generation then return end
          local names = more.ok and more.json and more.json() or nil
          local rows = {}
          for _, name in ipairs(type(names) == "table" and names or {}) do
            rows[#rows + 1] = { name = tostring(name), version = "", description = "", source = "aur" }
          end
          callback(rows, "prefix")
        end)
      return
    end
    local rows = {}
    for _, item in ipairs(type(data.results) == "table" and data.results or {}) do
      rows[#rows + 1] = {
        name = tostring(item.Name or ""), version = tostring(item.Version or ""),
        description = type(item.Description) == "string" and item.Description or "",
        source = "aur", votes = tonumber(item.NumVotes) or 0,
        out_of_date = type(item.OutOfDate) == "number",
      }
    end
    table.sort(rows, function(a, b) return (a.votes or 0) > (b.votes or 0) end)
    while #rows > 60 do table.remove(rows) end
    callback(rows, "ok")
  end)
end

function M.search()
  local term = M.term()
  search_generation = search_generation + 1
  local generation = search_generation
  if #term < 2 then
    found = { term = term, results = {}, repos = 0, aur = 0, note = term == "" and "" or "short" }
    s.searching:set(false)
    s.found_rev:set(s.found_rev:get() + 1)
    return
  end
  s.searching:set(true)
  local repo_rows, aur_rows, note, pending = {}, {}, "ok", 2
  local function done()
    pending = pending - 1
    if pending > 0 or generation ~= search_generation then return end
    local results = {}
    for _, row in ipairs(repo_rows) do results[#results + 1] = row end
    local seen = {}
    for _, row in ipairs(repo_rows) do seen[row.name] = true end
    for _, row in ipairs(aur_rows) do
      if not seen[row.name] then
        row.installed = by_name[row.name] ~= nil
        results[#results + 1] = row
      end
    end
    found = { term = term, results = results, repos = #repo_rows, aur = #aur_rows, note = note }
    s.searching:set(false)
    s.found_rev:set(s.found_rev:get() + 1)
  end
  if pacman then
    morf.run({ pacman, "-Ss", "--", term }, { timeout_ms = 20000 }, function(result)
      if generation ~= search_generation then return end
      local current
      for line in tostring(result.stdout or ""):gmatch("[^\n]+") do
        local repo, name, version, rest = line:match("^(%S+)/(%S+)%s+(%S+)(.*)$")
        if repo and #repo_rows < 80 then
          current = { name = name, version = version, description = "", source = repo,
            installed = rest:find("%[installed") ~= nil }
          repo_rows[#repo_rows + 1] = current
        elseif current and line:match("^%s+") then
          current.description = line:match("^%s+(.-)%s*$")
          current = nil
        end
      end
      done()
    end)
  else
    done()
  end
  aur_search(term, generation, function(rows, why)
    aur_rows = rows or {}
    note = why or "ok"
    done()
  end)
end

-- Each search is a pacman query and an AUR request: 250 ms of quiet first.
local debounce_generation = 0
morf.effect("impasto.packages.debounce", function()
  s.query:get()
  local view = s.view:get()
  debounce_generation = debounce_generation + 1
  local mine = debounce_generation
  if view ~= "find" then return end
  morf.timer(250, function()
    if mine ~= debounce_generation then return end
    if M.term() ~= found.term then M.search() end
  end, false)
end)

-- --------------------------------------------------------------------- rows --

local function matches(row, term)
  return term == "" or row.name:lower():find(term, 1, true) ~= nil
end

--- One shape for the three lists: `{ name, version, description, source,
--- installed, from, to, votes, out_of_date }`, with what that list has.
function M.rows()
  local view, term = s.view:get(), M.term()
  local out = {}
  if view == "updates" then
    M.installed()
    for _, row in ipairs(updates.updates()) do
      if matches(row, term) then
        local known = by_name[row.name]
        out[#out + 1] = { name = row.name, from = row.from, to = row.to, source = row.source,
          description = known and known.description or "", installed = true }
      end
    end
    return out
  end
  if view == "installed" then
    local filter = s.filter:get()
    for _, row in ipairs(M.installed()) do
      local kept = term ~= "" or filter == "all" or (filter == "aur" and row.aur) or (filter == "mine" and row.explicit)
      if kept and matches(row, term) then out[#out + 1] = row end
    end
    return out
  end
  local f = M.found()
  if f.term == term then return f.results end
  return out
end

-- ------------------------------------------------------------------ actions --

-- A terminal that keeps its window after the command ends, so the outcome
-- stays readable; the flag that does it, per terminal.
local TERMINALS = {
  { name = "kitty", argv = function(p, title, cmd) return { p, "--hold", "--class", "impasto-packages", "--title", title, table.unpack(cmd) } end },
  { name = "foot", argv = function(p, title, cmd) return { p, "--hold", "--app-id", "impasto-packages", "--title", title, table.unpack(cmd) } end },
  { name = "alacritty", argv = function(p, title, cmd) return { p, "--hold", "--class", "impasto-packages", "--title", title, "-e", table.unpack(cmd) } end },
  { name = "konsole", argv = function(p, _, cmd) return { p, "--hold", "-e", table.unpack(cmd) } end },
  { name = "xterm", argv = function(p, title, cmd) return { p, "-hold", "-T", title, "-e", table.unpack(cmd) } end },
}

local function terminal_argv(cmd)
  local title = table.concat(cmd, " ")
  local wanted = morf.env("TERMINAL")
  for _, t in ipairs(TERMINALS) do
    if wanted and wanted ~= "" and (wanted == t.name or wanted:match("/" .. t.name .. "$")) then
      local p = act.which(t.name) or wanted
      return t.argv(p, title, cmd)
    end
  end
  for _, t in ipairs(TERMINALS) do
    local p = act.which(t.name)
    if p then return t.argv(p, title, cmd) end
  end
  if wanted and wanted ~= "" then return { wanted, "-e", table.unpack(cmd) } end
  return nil
end

function M.can_open() return terminal_argv({ "true" }) ~= nil end

--- Runs `cmd` in a terminal; when it closes, every list is read again.
function M.open(cmd)
  if s.busy:get() then return false end
  local argv = terminal_argv(cmd)
  if not argv then
    morf.log("warn", "impasto: no terminal to run " .. table.concat(cmd, " ") .. " in")
    return false
  end
  local what = "running " .. table.concat(cmd, " ") .. " in a terminal"
  if act.dry then
    morf.log("info", "impasto: dry run, not " .. what)
    return true
  end
  local child = morf.spawn {
    command = argv,
    on_exit = function()
      s.busy:set(false)
      M.load()
      updates.refresh()
      if found.term ~= "" then M.search() end
    end,
  }
  if child then s.busy:set(true) end
  return child ~= nil
end

function M.installable(row)
  return row ~= nil and not row.installed and (row.source ~= "aur" or M.helper ~= "")
end

function M.install(row)
  if not row then return end
  if M.helper ~= "" then M.open { M.helper, "-S", row.name }
  elseif row.source ~= "aur" then M.open { "sudo", "pacman", "-S", row.name } end
end

--- -Rns: orphaned dependencies and kept configuration go too; pacman
--- refuses, with its reason, if something still depends on it.
function M.remove(row)
  if row then M.open { "sudo", "pacman", "-Rns", row.name } end
end

--- Everything, the only upgrade Arch supports.
function M.upgrade()
  M.open(M.helper ~= "" and { M.helper, "-Syu" } or { "sudo", "pacman", "-Syu" })
end

return M
