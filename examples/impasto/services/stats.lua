-- The machine's load, for the desk's System faces.
--
-- Port of StatsService.qml over `lib.sysinfo` (/proc and /sys). The library
-- lands in lua-stdlib separately; until then a small reader here takes the
-- processor and memory from /proc itself, every two seconds and only while
-- something reads them -- enough for the faces, not a replacement.

local M = {}

local ok, lib = pcall(require, "lib.sysinfo")
if not ok then lib = nil end

M.HISTORY = 40

-- ---------------------------------------------------------------- fallback --

local fallback = {}
if not lib then
  local revision = morf.signal("impasto.stats.revision", 0)
  local readers = 0
  local last_total, last_idle = nil, nil
  local cpu, mem_used, mem_total, load1 = 0, 0, 1, 0
  local cpu_history, mem_history = {}, {}
  local ticking = false

  local function push(list, value)
    list[#list + 1] = value
    while #list > M.HISTORY do table.remove(list, 1) end
  end

  local function sample()
    local stat = morf.fs.read("/proc/stat") or ""
    local fields = {}
    for n in (stat:match("^cpu%s+([^\n]+)") or ""):gmatch("%d+") do fields[#fields + 1] = tonumber(n) end
    if #fields >= 4 then
      local idle = fields[4] + (fields[5] or 0)
      local total = 0
      for _, v in ipairs(fields) do total = total + v end
      if last_total and total > last_total then
        cpu = 100 * (1 - (idle - last_idle) / (total - last_total))
      end
      last_total, last_idle = total, idle
    end
    local info = morf.fs.read("/proc/meminfo") or ""
    local total = tonumber(info:match("MemTotal:%s+(%d+)")) or 1
    local available = tonumber(info:match("MemAvailable:%s+(%d+)")) or total
    mem_total, mem_used = total * 1024, (total - available) * 1024
    load1 = tonumber(((morf.fs.read("/proc/loadavg") or ""):match("^(%S+)"))) or 0
    -- New lists, so a binding that holds the old one sees a change.
    local c, m = {}, {}
    for i, v in ipairs(cpu_history) do c[i] = v end
    for i, v in ipairs(mem_history) do m[i] = v end
    push(c, cpu / 100)
    push(m, mem_used / mem_total)
    cpu_history, mem_history = c, m
    revision:set(revision:get() + 1)
  end

  -- Samples while anything read since the last tick; stops otherwise, and
  -- the next read starts it again.
  local function tick()
    sample()
    if readers > 0 then
      readers = 0
      morf.timer(2000, tick, false)
    else
      ticking = false
    end
  end

  local function read()
    readers = readers + 1
    if not ticking then
      ticking = true
      morf.timer(1, tick, false)
    end
    revision:get()
  end

  fallback.cpu = function() read() return cpu end
  fallback.memory = function() read() return mem_used, mem_total end
  fallback.load = function() read() return load1 end
  fallback.history = function(name)
    read()
    if name == "memory" then return mem_history end
    if name == "cpu" then return cpu_history end
    return {}
  end
end

-- ------------------------------------------------------------------ readers --

local function section(name)
  local okr, value = pcall(function() return lib[name]() end)
  return okr and type(value) == "table" and value or {}
end

--- Processor use, 0..100.
function M.cpu()
  if not lib then return fallback.cpu() end
  return tonumber(section("cpu").usage) or 0
end

--- Memory in use, as a fraction.
function M.memory_fraction()
  if not lib then
    local used, total = fallback.memory()
    return used / math.max(1, total)
  end
  return (tonumber(section("memory").percent) or 0) / 100
end

function M.memory_used()
  if not lib then return (fallback.memory()) end
  return tonumber(section("memory").used) or 0
end

--- The one-minute load average.
function M.load()
  if not lib then return fallback.load() end
  local l = section("cpu").load
  return l and tonumber(l[1]) or 0
end

--- The processor's temperature in degrees, or nil.
function M.temperature()
  if not lib then return nil end
  return section("temperatures").cpu
end

--- Bytes a second coming in on the default interface.
function M.down_rate()
  if not lib then return 0 end
  return tonumber(section("network").rx_rate) or 0
end

--- Samples, oldest first, each 0..1: `cpu`, `memory` or `rx`.
function M.history(name)
  if not lib then return fallback.history(name) end
  local okh, list = pcall(lib.history, name)
  if not okh or type(list) ~= "table" then return {} end
  local out, peak = {}, 1
  if name == "rx" then for _, v in ipairs(list) do peak = math.max(peak, tonumber(v) or 0) end end
  for _, v in ipairs(list) do
    v = tonumber(v) or 0
    if name == "rx" then out[#out + 1] = v / peak
    else out[#out + 1] = v > 1 and v / 100 or v end
  end
  return out
end

--- Seconds since boot, or nil.
function M.uptime()
  if not lib then return nil end
  return section("system").uptime
end

--- "1.2 GiB"
function M.bytes(n)
  n = tonumber(n) or 0
  local units = { "B", "KiB", "MiB", "GiB", "TiB" }
  local i = 1
  while n >= 1024 and i < #units do n = n / 1024 i = i + 1 end
  return string.format(i <= 2 and "%.0f %s" or "%.1f %s", n, units[i])
end

--- "1.2 MiB/s"
function M.rate(n) return M.bytes(n) .. "/s" end

return M
