-- What is installed, what is pending, what could be: pacman and the AUR,
-- read here for the packages panel.
--
-- Port of PackagesService.qml and scripts/packages.py. Everything here only
-- reads. What is installed is read from pacman's own database with
-- `morf.fs` -- one `desc` file per package under /var/lib/pacman/local,
-- giving the version, the description, the size and whether it was
-- installed on purpose (%REASON%) -- a slice at a time, since a machine has
-- thousands. `pacman -Qqm` says which came from outside the repositories.
-- A search is `pacman -Ss` (the term escaped, as it is a regular
-- expression, and the rows kept to names that contain it) beside the AUR's
-- RPC, ranked exact, prefix, anywhere, the repositories first in a tier and
-- the AUR by popularity.
--
-- Install, Remove and Update run `sudo pacman` (or the AUR helper) in a
-- terminal window of the shell's own (services/terminal.lua), so pacman's
-- questions, the password and the output stay in front of the person who
-- asked, and only when they click. A dry run (IMPASTO_DRY_RUN=1) logs them
-- and starts nothing. There is no per-package upgrade: Arch does not do
-- partial upgrades, so Update is always `-Syu`.

local act = require("services.act")
local updates = require("services.updates")

local fs = morf.fs

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

-- pacman's local database; a test bench may point it elsewhere.
M.db = morf.env("IMPASTO_PACMAN_DB") or "/var/lib/pacman/local"

local function set_of(text)
  local out = {}
  for name in tostring(text or ""):gmatch("[^\n]+") do out[name] = true end
  return out
end

--- Bytes as pacman prints them: "291.08 KiB".
function M.size_text(bytes)
  bytes = tonumber(bytes) or 0
  local units = { "B", "KiB", "MiB", "GiB", "TiB" }
  local value, unit = bytes, 1
  while value >= 1024 and unit < #units do
    value = value / 1024
    unit = unit + 1
  end
  return ("%.2f %s"):format(value, units[unit])
end

--- One `desc` file: `%FIELD%` then its lines, blocks apart. %REASON% 1 is
--- "installed as a dependency"; absent (or 0) is "explicitly".
function M.parse_desc(text)
  local fields, key = {}, nil
  for line in (tostring(text or "") .. "\n"):gmatch("([^\n]*)\n") do
    local name = line:match("^%%(%u+)%%$")
    if name then
      key = name
      fields[key] = fields[key] or ""
    elseif key and line ~= "" then
      fields[key] = fields[key] == "" and line or (fields[key] .. " " .. line)
    elseif line == "" then
      key = nil
    end
  end
  if not fields.NAME or fields.NAME == "" then return nil end
  local bytes = tonumber(fields.SIZE or "") or 0
  return {
    name = fields.NAME, version = fields.VERSION or "", description = fields.DESC or "",
    bytes = bytes, size = bytes > 0 and M.size_text(bytes) or "",
    explicit = (fields.REASON or "0") ~= "1", installed = true,
  }
end

function M.load()
  if s.loading:get() or not pacman then return end
  s.loading:set(true)
  local poll = require("lib.poll")
  local foreign, listed = nil, nil
  local function all_in()
    if not foreign or not listed then return end
    for _, row in ipairs(listed) do row.aur = foreign[row.name] == true end
    table.sort(listed, function(a, b) return a.name < b.name end)
    installed = listed
    by_name = {}
    for _, row in ipairs(listed) do by_name[row.name] = row end
    s.loading:set(false)
    s.loaded:set(true)
    s.installed_rev:set(s.installed_rev:get() + 1)
  end
  -- A few hundred files a slice: a handler has a fixed budget.
  poll.job(function(spend)
    local rows = {}
    for _, entry in ipairs(fs.list(M.db) or {}) do
      spend(2)
      if entry.is_dir then
        local row = M.parse_desc(fs.read(fs.join(M.db, entry.name, "desc")))
        spend(40)
        if row then rows[#rows + 1] = row end
      end
    end
    return rows
  end, function(rows)
    listed = rows or {}
    all_in()
  end)
  morf.run({ pacman, "-Qqm" }, { timeout_ms = 30000 }, function(result)
    foreign = set_of(result.stdout)
    all_in()
  end)
end

-- ------------------------------------------------------------------ search --

local search_generation = 0
local AUR_RPC = "https://aur.archlinux.org/rpc/v5"
-- Rows a search keeps (packages.py LISTED).
M.LISTED = 160

--- `term` as a POSIX extended regular expression that matches it literally:
--- `pacman -Ss` takes a regex, and `c++` or `.net` are package names.
function M.regex_escape(term)
  return (tostring(term or ""):gsub("[%^%$%.%|%?%*%+%(%)%[%]%{%}\\]", "\\%0"))
end

--- 0 exact, 1 prefix, 2 anywhere in the name.
function M.rank(name, term)
  if name == term then return 0 end
  return name:sub(1, #term) == term and 1 or 2
end

--- Match quality first, whichever the source, so `hyprland` ranks above
--- repository packages that merely contain it; within a tier the
--- repositories lead, and the AUR follows by popularity.
function M.order(rows, term)
  table.sort(rows, function(a, b)
    local ra, rb = M.rank(a.name:lower(), term), M.rank(b.name:lower(), term)
    if ra ~= rb then return ra < rb end
    local aa, ab = a.source == "aur", b.source == "aur"
    if aa ~= ab then return ab end
    local pa, pb = a.popularity or 0, b.popularity or 0
    if pa ~= pb then return pa > pb end
    return a.name < b.name
  end)
  return rows
end

local function aur_rows(data)
  local rows = {}
  for _, item in ipairs(type(data) == "table" and type(data.results) == "table" and data.results or {}) do
    rows[#rows + 1] = {
      name = tostring(item.Name or ""), version = tostring(item.Version or ""),
      description = type(item.Description) == "string" and item.Description or "",
      source = "aur", votes = tonumber(item.NumVotes) or 0,
      popularity = tonumber(item.Popularity) or 0,
      out_of_date = type(item.OutOfDate) == "number",
    }
  end
  return rows
end

local function aur_search(term, generation, callback)
  if not morf.http then return callback(nil, "offline") end
  local quoted = morf.http.url_encode(term)
  local headers = { ["User-Agent"] = "impasto" }
  morf.http.get(AUR_RPC .. "/search/" .. quoted .. "?by=name", { timeout_ms = 8000, headers = headers },
    function(response)
      if generation ~= search_generation then return end
      local data = response.ok and response.json and response.json() or nil
      if type(data) ~= "table" then return callback(nil, "offline") end
      if data.type ~= "error" then return callback(aur_rows(data), "ok") end
      -- Too many results: the names that START with it, which the RPC
      -- answers in order, and then their details (packages.py).
      morf.http.get(AUR_RPC .. "/suggest/" .. quoted, { timeout_ms = 8000, headers = headers }, function(more)
        if generation ~= search_generation then return end
        local names = more.ok and more.json and more.json() or nil
        if type(names) ~= "table" then return callback(nil, "offline") end
        if #names == 0 then return callback({}, "prefix") end
        local parts = {}
        for _, name in ipairs(names) do parts[#parts + 1] = "arg[]=" .. morf.http.url_encode(tostring(name)) end
        morf.http.get(AUR_RPC .. "/info?" .. table.concat(parts, "&"), { timeout_ms = 8000, headers = headers },
          function(info)
            if generation ~= search_generation then return end
            local detail = info.ok and info.json and info.json() or nil
            if type(detail) ~= "table" then return callback(nil, "offline") end
            callback(aur_rows(detail), "prefix")
          end)
      end)
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
  local repo_rows, remote, note, pending = {}, {}, "ok", 2
  local function done()
    pending = pending - 1
    if pending > 0 or generation ~= search_generation then return end
    -- A name in both is the repository's: that is the one pacman installs.
    local taken, rows = {}, {}
    for _, row in ipairs(repo_rows) do
      taken[row.name] = true
      rows[#rows + 1] = row
    end
    local aur_count = 0
    for _, row in ipairs(remote) do
      if not taken[row.name] then
        row.installed = by_name[row.name] ~= nil
        rows[#rows + 1] = row
        aur_count = aur_count + 1
      end
    end
    M.order(rows, term)
    local total = #rows
    while #rows > M.LISTED do table.remove(rows) end
    found = { term = term, results = rows, repos = #repo_rows, aur = aur_count, total = total, note = note }
    s.searching:set(false)
    s.found_rev:set(s.found_rev:get() + 1)
  end
  if pacman then
    morf.run({ pacman, "-Ss", "--", M.regex_escape(term) },
      { timeout_ms = 20000, env = { LANG = "C", LC_ALL = "C" } }, function(result)
        if generation ~= search_generation then return end
        local current
        for line in tostring(result.stdout or ""):gmatch("[^\n]+") do
          local repo, name, version, rest = line:match("^(%S+)/(%S+)%s+(%S+)(.*)$")
          if repo and not line:match("^%s") then
            -- `-Ss` matches descriptions too; the name must contain it.
            current = { name = name, version = version, description = "", source = repo,
              installed = rest:find("%[installed") ~= nil }
            if name:lower():find(term, 1, true) then repo_rows[#repo_rows + 1] = current end
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
    remote = rows or {}
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

function M.can_open() return true end

--- Runs `cmd` in a terminal window of the shell's own (services/terminal.lua),
--- which stays up when it ends so the outcome can be read; once it has
--- ended, every list is read again.
function M.open(cmd)
  if s.busy:get() then return false end
  local what = "running " .. table.concat(cmd, " ") .. " in a terminal"
  if act.dry then
    morf.log("info", "impasto: dry run, not " .. what)
    return true
  end
  local term, why = require("services.terminal").run(cmd, {
    title = table.concat(cmd, " "),
    hold = true,
    on_exit = function()
      s.busy:set(false)
      M.load()
      updates.refresh()
      if found.term ~= "" then M.search() end
    end,
  })
  if not term then
    morf.log("warn", "impasto: could not open a terminal for " .. table.concat(cmd, " ") .. ": " .. tostring(why))
    return false
  end
  s.busy:set(true)
  return true
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
