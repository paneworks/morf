-- Pending updates and installed counts, from the package managers present.
--
-- A panel's "12 updates" is a question for the package manager, and the
-- package manager is a program; so this runs the programs, directly with an
-- argument list and never through a shell, and reads what they print. Only
-- ever queries: nothing here syncs, installs or asks for privileges.
--
--   pacman   -- `checkupdates` when pacman-contrib is there (it syncs a copy
--               of the database in /tmp, so the answer is current without
--               touching the system's), else `pacman -Qu` against the last
--               sync; `pacman -Q` for the count
--   AUR      -- foreign packages from `pacman -Qm`, their versions asked of
--               the AUR's RPC over `morf.http`, compared the way pacman does
--   flatpak  -- `flatpak remote-ls --updates` and `flatpak list`
--
--   local packages = require("lib.integrations.packages")
--   local updates = packages.new {}
--   ui.Text { text = function() return updates:get().total .. " updates" end }

local morf = require("morf")
local poll = require("lib.util.poll")

local packages = {}

-- ---------------------------------------------------------------------------
-- Versions, compared as pacman compares them (alpm_pkg_vercmp): an epoch,
-- then the version by runs of digits and letters, then the release.

local function split_evr(text)
  local epoch, rest = text:match("^(%d+):(.*)$")
  if not epoch then epoch, rest = "0", text end
  local version, release = rest:match("^(.*)%-([^%-]*)$")
  if not version then version, release = rest, nil end
  return tonumber(epoch), version, release
end

-- rpmvercmp: compare segment by segment; numbers beat letters, longer
-- numbers win, a trailing segment makes a version newer unless it is a
-- letter segment (1.0a < 1.0).
local function rpmvercmp(a, b)
  if a == b then return 0 end
  local i, j = 1, 1
  while true do
    local sa = a:match("^[^%w]*()", i)
    local sb = b:match("^[^%w]*()", j)
    i, j = sa, sb
    if i > #a or j > #b then break end
    local seg_a, seg_b, numeric
    if a:sub(i, i):match("%d") then
      seg_a = a:match("^%d+", i)
      seg_b = b:match("^%d+", j)
      numeric = true
    else
      seg_a = a:match("^%a+", i)
      seg_b = b:match("^%a+", j)
      numeric = false
    end
    if not seg_b then return numeric and 1 or -1 end
    i, j = i + #seg_a, j + #seg_b
    if numeric then
      local ta, tb = seg_a:gsub("^0+", ""), seg_b:gsub("^0+", "")
      if #ta ~= #tb then return #ta > #tb and 1 or -1 end
      if ta ~= tb then return ta > tb and 1 or -1 end
    elseif seg_a ~= seg_b then
      return seg_a > seg_b and 1 or -1
    end
  end
  local rest_a, rest_b = i <= #a, j <= #b
  if not rest_a and not rest_b then return 0 end
  if not rest_a then
    -- b has more: newer unless what follows is letters.
    return b:sub(j, j):match("%a") and 1 or -1
  end
  return a:sub(i, i):match("%a") and -1 or 1
end

--- -1, 0 or 1 as `a` is older than, the same as or newer than `b`.
function packages.vercmp(a, b)
  local ea, va, ra = split_evr(a)
  local eb, vb, rb = split_evr(b)
  if ea ~= eb then return ea > eb and 1 or -1 end
  local by_version = rpmvercmp(va, vb)
  if by_version ~= 0 or not ra or not rb then return by_version end
  return rpmvercmp(ra, rb)
end

-- ---------------------------------------------------------------------------
-- Output

local function count_lines(text)
  if text == "" then return 0 end
  local _, newlines = text:gsub("\n", "")
  return text:sub(-1) == "\n" and newlines or newlines + 1
end

--- `name old -> new` lines, as checkupdates and pacman -Qu print them.
function packages.parse_updates(text, spend)
  local list = {}
  for name, old, new in text:gmatch("([^%s]+) ([^%s]+) %-> ([^%s]+)") do
    if spend then spend(3) end
    list[#list + 1] = { name = name, old = old, new = new }
  end
  return list
end

-- ---------------------------------------------------------------------------
-- A checker

local Checker = {}
Checker.__index = Checker

--- Options:
---   interval   -- ms between checks while read (default an hour)
---   aur        -- ask the AUR about foreign packages (default true)
---   flatpak    -- check flatpak when it is installed (default true)
---   aur_url    -- the RPC (default https://aur.archlinux.org/rpc/v5/info)
---   run(argv, on_done)  -- how programs are run; `poll.run` unless a test
---                          says otherwise; on_done gets {ok, code, stdout, stderr}
---   which(name)         -- how programs are found; `poll.which` by default
function packages.new(options)
  options = options or {}
  local self = setmetatable({
    aur = options.aur ~= false,
    flatpak = options.flatpak ~= false,
    aur_url = options.aur_url or "https://aur.archlinux.org/rpc/v5/info",
    run = options.run or function(argv, on_done) poll.run(argv, on_done, { timeout_ms = 120000 }) end,
    which = options.which or poll.which,
  }, Checker)
  self.source = poll.source {
    name = "packages",
    interval = options.interval or 60 * 60 * 1000,
    linger = 1,
    initial = { checking = true, total = 0, managers = {} },
    sample = function(done) self:_check(done) end,
  }
  return self
end

--- What is known, read so a binding follows it:
--- `{ total, managers = { "pacman", ... }, pacman = { installed, updates,
--- count, via }, aur = { foreign, updates, count }, flatpak = { installed,
--- updates, count }, errors, checking, updated }`. Each `updates` is a list
--- of `{ name, old, new }` (flatpak's have no versions). A manager that is
--- not installed is absent.
function Checker:get() return self.source:get() end

--- Checks again now.
function Checker:refresh() self.source:refresh() end

-- Runs the steps one after another, each calling `next` when its program has
-- answered, and hands the result on when the last has.
function Checker:_check(done)
  local result = { total = 0, managers = {}, errors = {}, checking = false }
  local steps = {}
  local pacman = self.which("pacman")
  if pacman then
    result.pacman = { installed = 0, updates = {}, count = 0 }
    result.managers[#result.managers + 1] = "pacman"
    steps[#steps + 1] = function(next)
      self.run({ pacman, "-Q" }, function(answer)
        if answer.ok then
          result.pacman.installed = count_lines(answer.stdout)
        else
          result.errors[#result.errors + 1] = "pacman -Q: " .. (answer.error or answer.stderr)
        end
        next()
      end)
    end
    local checkupdates = self.which("checkupdates")
    steps[#steps + 1] = function(next)
      -- checkupdates exits 2 for "none"; pacman -Qu exits 1 for "none". An
      -- empty answer with an error on stderr is an error.
      local argv = checkupdates and { checkupdates } or { pacman, "-Qu" }
      result.pacman.via = checkupdates and "checkupdates" or "pacman -Qu"
      self.run(argv, function(answer)
        if answer.ok or (answer.code and answer.stdout ~= "") or (answer.code and answer.stderr == "") then
          poll.job(function(spend) return packages.parse_updates(answer.stdout, spend) end, function(list)
            result.pacman.updates = list or {}
            result.pacman.count = #result.pacman.updates
            next()
          end)
        else
          result.errors[#result.errors + 1] = result.pacman.via .. ": " .. (answer.error or answer.stderr)
          next()
        end
      end)
    end
    if self.aur then
      result.aur = { foreign = 0, updates = {}, count = 0 }
      result.managers[#result.managers + 1] = "aur"
      steps[#steps + 1] = function(next) self:_aur(result, next) end
    end
  end
  local flatpak = self.flatpak and self.which("flatpak")
  if flatpak then
    result.flatpak = { installed = 0, updates = {}, count = 0 }
    result.managers[#result.managers + 1] = "flatpak"
    steps[#steps + 1] = function(next)
      self.run({ flatpak, "list", "--columns=application" }, function(answer)
        if answer.ok then result.flatpak.installed = count_lines(answer.stdout) end
        next()
      end)
    end
    steps[#steps + 1] = function(next)
      self.run({ flatpak, "remote-ls", "--updates", "--columns=application,version" }, function(answer)
        if answer.ok then
          for line in answer.stdout:gmatch("[^\n]+") do
            local name, version = line:match("^(%S+)%s*(%S*)")
            if name then
              result.flatpak.updates[#result.flatpak.updates + 1] =
                { name = name, new = version ~= "" and version or nil }
            end
          end
          result.flatpak.count = #result.flatpak.updates
        else
          result.errors[#result.errors + 1] = "flatpak remote-ls: " .. (answer.error or answer.stderr)
        end
        next()
      end)
    end
  end
  local index = 0
  local function next()
    index = index + 1
    local step = steps[index]
    if step then
      step(next)
      return
    end
    for _, name in ipairs(result.managers) do result.total = result.total + result[name].count end
    result.updated = morf.time.now()
    done(result)
  end
  next()
end

-- The AUR: what `pacman -Qm` lists as foreign, asked about a hundred at a
-- time (the RPC takes many names per request; a URL has a length).
function Checker:_aur(result, next)
  self.run({ self.which("pacman"), "-Qm" }, function(answer)
    local installed, names = {}, {}
    for name, version in (answer.stdout or ""):gmatch("([^%s]+) ([^%s]+)") do
      installed[name] = version
      names[#names + 1] = name
    end
    result.aur.foreign = #names
    if #names == 0 then next() return end
    local batch_start = 1
    local function ask()
      if batch_start > #names then
        result.aur.count = #result.aur.updates
        next()
        return
      end
      local parts = {}
      for index = batch_start, math.min(#names, batch_start + 99) do
        parts[#parts + 1] = "arg[]=" .. morf.http.url_encode(names[index])
      end
      batch_start = batch_start + 100
      morf.http.get(self.aur_url .. "?" .. table.concat(parts, "&"), { timeout_ms = 20000 }, function(response)
        local data = response.ok and response.json() or nil
        if not data or type(data.results) ~= "table" then
          result.errors[#result.errors + 1] = "aur: " .. (response.error or ("answered " .. response.status))
          result.aur.count = #result.aur.updates
          next()
          return
        end
        poll.job(function(spend)
          for _, package in ipairs(data.results) do
            -- A comparison is a few dozen calls.
            spend(40)
            local mine = installed[package.Name]
            if mine and type(package.Version) == "string" and packages.vercmp(package.Version, mine) > 0 then
              result.aur.updates[#result.aur.updates + 1] = { name = package.Name, old = mine, new = package.Version }
            end
          end
        end, ask)
      end)
    end
    ask()
  end)
end

return packages
