-- Machine load, with a short history per reading so the graphs already
-- have something to draw when a panel opens.
--
-- Port of StatsService.qml. The original kept scripts/stats.py resident and
-- read one JSON line per interval; here the same files are read directly:
-- /proc/stat (every core), /proc/meminfo, /proc/loadavg, /proc/uptime,
-- /proc/net/dev, the hwmon sensors, and `morf.fs.disk` for each real mount.
-- Mounts and sensors are re-read only every tenth sample. When
-- examples/lib/sysinfo.lua is present and offers `temperature()`, that is
-- used instead of this file's own reader.
--
-- The sampler starts the first time something reads a figure (the bar's
-- module, the panel) and then keeps going, so the history is continuous.

local fs = morf.fs
local M = {}

M.POLL_MS = 3000
M.HISTORY = 100

local ok_sysinfo, sysinfo = pcall(require, "lib.sysinfo")
if not ok_sysinfo or type(sysinfo) ~= "table" then sysinfo = nil end

local s = {
  revision = morf.signal("impasto.stats.revision", 0),
}
M.signals = s

-- The latest sample. Plain values; `revision` is what bindings follow.
local now = {
  ready = false,
  cpu = 0, cores = {}, model = "", load = { 0, 0, 0 },
  memory_used = 0, memory_total = 0, swap_used = 0, swap_total = 0,
  disks = {}, down = 0, up = 0, temperature = nil, uptime = 0,
}
local history = { cpu = {}, memory = {}, down = {}, up = {}, temperature = {} }

local started = false
local function start()
  if started then return end
  started = true
  -- Out of the binding that asked: sampling writes the signal it reads.
  morf.timer(1, function()
    M.sample()
    morf.timer(M.POLL_MS, function() M.sample() end, true)
  end, false)
end

local function read() start() s.revision:get() return now end

function M.ready() return read().ready end
function M.cpu() return read().cpu end
function M.cores() return read().cores end
function M.model() return read().model end
function M.load() return read().load end
function M.memory_used() return read().memory_used end
function M.memory_total() return read().memory_total end
function M.swap_used() return read().swap_used end
function M.swap_total() return read().swap_total end
function M.disks() return read().disks end
function M.down() return read().down end
function M.up() return read().up end
function M.temperature() return read().temperature end
function M.uptime() return read().uptime end
function M.history(name) read() return history[name] or {} end

function M.memory_fraction()
  local n = read()
  return n.memory_total > 0 and n.memory_used / n.memory_total or 0
end
function M.swap_fraction()
  local n = read()
  return n.swap_total > 0 and n.swap_used / n.swap_total or 0
end

function M.thermal_word()
  local t = M.temperature()
  if not t then return "" end
  if t.celsius < 45 then return "Cool" end
  if t.celsius < 65 then return "Warm" end
  if t.celsius < 85 then return "Hot" end
  return "Very hot"
end

-- ------------------------------------------------------------ formatting --

local UNITS = { "B", "KiB", "MiB", "GiB", "TiB" }

function M.bytes(value, decimals)
  local amount, unit = math.max(0, tonumber(value) or 0), 1
  while amount >= 1024 and unit < #UNITS do
    amount = amount / 1024
    unit = unit + 1
  end
  local places = unit == 1 and 0 or (decimals or 1)
  return ("%." .. places .. "f %s"):format(amount, UNITS[unit])
end

function M.rate(value) return M.bytes(value, 0) .. "/s" end

--- Bytes a second coming in; the desk's name for `down`.
function M.down_rate() return M.down() end

function M.duration(seconds)
  seconds = math.floor(tonumber(seconds) or 0)
  local days, hours, minutes = seconds // 86400, (seconds % 86400) // 3600, (seconds % 3600) // 60
  if days > 0 then return ("%dd %dh"):format(days, hours) end
  return hours > 0 and ("%dh %dm"):format(hours, minutes) or ("%dm"):format(minutes)
end

-- --------------------------------------------------------------- readers --

local previous_cpu = {}
local function read_cpu()
  local text = fs.read("/proc/stat") or ""
  local total, cores = 0, {}
  for name, rest in text:gmatch("(cpu%d*)%s+([^\n]+)") do
    local fields, sum = {}, 0
    for number in rest:gmatch("%d+") do
      fields[#fields + 1] = tonumber(number)
      if #fields <= 8 then sum = sum + fields[#fields] end
    end
    local idle = (fields[4] or 0) + (fields[5] or 0)
    local before = previous_cpu[name]
    local busy = 0
    if before and sum > before.sum then
      busy = 100 * (1 - (idle - before.idle) / (sum - before.sum))
    end
    previous_cpu[name] = { sum = sum, idle = idle }
    busy = math.max(0, math.min(100, busy))
    if name == "cpu" then total = busy else cores[#cores + 1] = busy end
  end
  return total, cores
end

local function read_memory()
  local text = fs.read("/proc/meminfo") or ""
  local kb = function(key) return (tonumber(text:match(key .. ":%s+(%d+)")) or 0) * 1024 end
  local total, available = kb("MemTotal"), kb("MemAvailable")
  local swap_total, swap_free = kb("SwapTotal"), kb("SwapFree")
  return total - available, total, swap_total - swap_free, swap_total
end

local previous_net
local function read_network()
  local text = fs.read("/proc/net/dev") or ""
  local rx, tx = 0, 0
  for name, rest in text:gmatch("\n%s*([^:%s]+):([^\n]+)") do
    if name ~= "lo" then
      local fields = {}
      for number in rest:gmatch("%d+") do fields[#fields + 1] = tonumber(number) end
      rx = rx + (fields[1] or 0)
      tx = tx + (fields[9] or 0)
    end
  end
  local stamp = morf.time.now_ms()
  local down, up = 0, 0
  if previous_net and stamp > previous_net.at then
    local seconds = (stamp - previous_net.at) / 1000
    down = math.max(0, (rx - previous_net.rx) / seconds)
    up = math.max(0, (tx - previous_net.tx) / seconds)
  end
  previous_net = { rx = rx, tx = tx, at = stamp }
  return down, up
end

local REAL = { ext4 = true, ext3 = true, btrfs = true, xfs = true, f2fs = true, vfat = true,
  exfat = true, zfs = true, bcachefs = true, ntfs3 = true, ntfs = true, jfs = true }

local function read_disks()
  -- What stats.py took from `df`: every mounted real filesystem, each mount
  -- on its own (a btrfs subvolume is a line of its own there too), the four
  -- largest, in mount order among equals. Not `lib.sysinfo`'s list, which
  -- keeps one mount per device and names it `mount`.
  local text = fs.read("/proc/mounts") or ""
  local out = {}
  for _, target, kind in text:gmatch("(%S+)%s+(%S+)%s+(%S+)[^\n]*") do
    target = target:gsub("\\040", " ")
    if REAL[kind] then
      local usage = fs.disk(target)
      if usage and (usage.total or 0) > 0 then
        out[#out + 1] = { target = target, total = usage.total, used = usage.used, order = #out }
      end
    end
  end
  table.sort(out, function(a, b)
    if a.total ~= b.total then return a.total > b.total end
    return a.order < b.order
  end)
  while #out > 4 do table.remove(out) end
  return out
end

-- The hottest sensor with a plausible reading (5 to 125 degrees), to a
-- tenth of a degree (stats.py `temperature`). A whole number stays whole,
-- so it reads "48°", not "48.0°".
local function tenths(value)
  local rounded = math.floor(value * 10 + 0.5) / 10
  if rounded == math.floor(rounded) then return math.floor(rounded) end
  return rounded
end
M.tenths = tenths

local function read_temperature()
  if sysinfo and type(sysinfo.temperature) == "function" then
    local ok, t = pcall(sysinfo.temperature)
    if ok and type(t) == "table" and t.celsius then return t end
  end
  local best
  for _, entry in ipairs(fs.list("/sys/class/hwmon") or {}) do
    local dir = fs.join("/sys/class/hwmon", entry.name)
    local name = (fs.read(fs.join(dir, "name")) or ""):match("^%s*(.-)%s*$")
    for index = 1, 16 do
      local raw = fs.read(fs.join(dir, "temp" .. index .. "_input"))
      local milli = raw and tonumber(raw:match("^%s*(%d+)"))
      if milli then
        local celsius = milli / 1000
        if celsius > 5 and celsius < 125 and (not best or celsius > best.raw) then
          local label = (fs.read(fs.join(dir, "temp" .. index .. "_label")) or ""):match("^%s*(.-)%s*$")
          best = { raw = celsius, celsius = tenths(celsius), label = label ~= "" and label or name }
        end
      end
    end
  end
  if best then best.raw = nil end
  return best
end

local function read_model()
  local text = fs.read("/proc/cpuinfo", 65536) or ""
  return (text:match("model name%s*:%s*([^\n]+)") or ""):gsub("%s+", " ")
end

local function push(series, value)
  series[#series + 1] = value
  while #series > M.HISTORY do table.remove(series, 1) end
end

local tick = 0
function M.sample()
  tick = tick + 1
  now.cpu, now.cores = read_cpu()
  now.memory_used, now.memory_total, now.swap_used, now.swap_total = read_memory()
  local a, b, c = (fs.read("/proc/loadavg") or ""):match("(%S+)%s+(%S+)%s+(%S+)")
  now.load = { tonumber(a) or 0, tonumber(b) or 0, tonumber(c) or 0 }
  now.uptime = tonumber((fs.read("/proc/uptime") or ""):match("^(%S+)")) or 0
  now.down, now.up = read_network()
  if tick % 10 == 1 then
    now.disks = read_disks()
    now.model = read_model()
  end
  -- Sensors change slowly but are what a hot machine watches: every sample.
  now.temperature = read_temperature()
  -- The first CPU sample has no interval to measure over.
  if tick > 1 then
    push(history.cpu, now.cpu / 100)
    push(history.memory, now.memory_total > 0 and now.memory_used / now.memory_total or 0)
    push(history.down, now.down)
    push(history.up, now.up)
    if now.temperature then push(history.temperature, now.temperature.celsius) end
    now.ready = true
  end
  s.revision:set(s.revision:get() + 1)
end

--- For a test bench: `n` samples at once, with made-up movement, so a
--- panel opened right after boot has a history to draw.
function M.warm(n)
  start()
  for i = 1, n or 40 do
    local wave = (math.sin(i / 4) + 1) / 2
    push(history.cpu, 0.15 + 0.5 * wave * ((i % 7) / 7))
    push(history.memory, 0.42 + 0.05 * math.sin(i / 9))
    push(history.down, 40000 + 900000 * wave * ((i % 5) / 5))
    push(history.up, 8000 + 120000 * ((i % 3) / 3) * wave)
    push(history.temperature, 48 + 10 * wave)
  end
  now.ready = true
  s.revision:set(s.revision:get() + 1)
end

return M
