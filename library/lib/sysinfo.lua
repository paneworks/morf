-- The machine, from /proc and /sys.
--
-- Every system monitor on Linux reads the same files: /proc/stat for the
-- CPU, /proc/meminfo for memory, /sys/class/hwmon for temperatures. The
-- engine does not wrap them -- which files matter, and what counts as "the
-- CPU temperature" on a given board, is policy -- so this library reads them
-- with `morf.fs` and hands out plain tables.
--
-- Each reader (`sysinfo.cpu()`, `sysinfo.memory()`, ...) is tracked: a binding
-- that calls it re-runs on each new sample. A section is only sampled while
-- something reads it (see lib/poll.lua), so a panel that shows the clock and
-- nothing else costs nothing here. The tables handed out are shared and
-- replaced whole on each sample: read them, do not change them.
--
--   local sysinfo = require("lib.sysinfo")
--   ui.Text { text = function() return ("%d%%"):format(sysinfo.cpu().usage) end }
--   sysinfo.configure { intervals = { cpu = 1000 }, history = 120 }
--
-- Paths start at `sysinfo.root` ("" -- the real machine), so a test can put a
-- fake /proc and /sys in a folder and point the library at it.

local morf = require("morf")
local poll = require("lib.poll")

local fs = morf.fs

local sysinfo = {
  --- Prefix for every /proc, /sys and /etc path.
  root = "",
  --- Samples kept for `history(name)`.
  history_size = 60,
  --- How many processes `processes()` lists in each ranking.
  top = 10,
}

local function path(p) return sysinfo.root .. p end

local function read(p, limit)
  return fs.read(path(p), limit)
end

local function number_at(p)
  local text = read(p, 4096)
  return text and tonumber(text:match("^%s*(%-?[%d.]+)")) or nil
end

local function text_at(p)
  local text = read(p, 4096)
  return text and text:match("^%s*(.-)%s*$") or nil
end

-- ---------------------------------------------------------------------------
-- History

local rings = {}
local function ring(name)
  local found = rings[name]
  if not found then
    found = poll.ring(sysinfo.history_size)
    rings[name] = found
  end
  return found
end

-- ---------------------------------------------------------------------------
-- CPU: usage from /proc/stat deltas, frequency, load.

local cpu_model
local last_stat = {}

local function read_model()
  if cpu_model then return cpu_model end
  local text = read("/proc/cpuinfo", 1024 * 1024)
  cpu_model = text and (text:match("model name%s*:%s*([^\n]+)")
    or text:match("Hardware%s*:%s*([^\n]+)")
    or text:match("Model%s*:%s*([^\n]+)")) or ""
  return cpu_model
end

local function frequencies(count)
  local list, sum, n = {}, 0, 0
  local text = read("/proc/cpuinfo", 1024 * 1024)
  if text then
    for mhz in text:gmatch("cpu MHz%s*:%s*([%d.]+)") do
      n = n + 1
      list[n] = tonumber(mhz)
      sum = sum + list[n]
    end
  end
  if n == 0 then
    -- ARM and some virtual machines leave cpuinfo without clocks; cpufreq
    -- has them in kHz, one file per core.
    for index = 0, count - 1 do
      local khz = number_at("/sys/devices/system/cpu/cpu" .. index .. "/cpufreq/scaling_cur_freq")
      if not khz then break end
      n = n + 1
      list[n] = khz / 1000
      sum = sum + list[n]
    end
  end
  return list, n > 0 and sum / n or 0
end

local function sample_cpu()
  local text, err = read("/proc/stat", 1024 * 1024)
  if not text then return nil, err end
  local out = { usage = 0, cores = {}, model = read_model() }
  local now = {}
  for name, user, nice, system, idle, iowait, irq, softirq, steal in text:gmatch(
    "(cpu%d*)%s+(%d+) (%d+) (%d+) (%d+) (%d+) (%d+) (%d+) ?(%d*)") do
    local busy = tonumber(user) + tonumber(nice) + tonumber(system) + tonumber(irq)
      + tonumber(softirq) + (tonumber(steal) or 0)
    local rest = tonumber(idle) + tonumber(iowait)
    now[name] = { busy = busy, total = busy + rest }
    local before = last_stat[name]
    local usage = 0
    if before and now[name].total > before.total then
      usage = 100 * (busy - before.busy) / (now[name].total - before.total)
    end
    if name == "cpu" then
      out.usage = usage
    else
      out.cores[#out.cores + 1] = { name = name, usage = usage }
    end
  end
  last_stat = now
  out.count = #out.cores
  out.frequencies, out.frequency = frequencies(out.count)
  for index, core in ipairs(out.cores) do core.frequency = out.frequencies[index] end
  local load = read("/proc/loadavg", 4096)
  if load then
    local one, five, fifteen, running, total = load:match("^(%S+) (%S+) (%S+) (%d+)/(%d+)")
    out.load = { tonumber(one) or 0, tonumber(five) or 0, tonumber(fifteen) or 0 }
    out.running, out.threads = tonumber(running), tonumber(total)
  else
    out.load = { 0, 0, 0 }
  end
  ring("cpu").push(out.usage)
  ring("load").push(out.load[1])
  for index, core in ipairs(out.cores) do ring("core" .. (index - 1)).push(core.usage) end
  return out
end

-- ---------------------------------------------------------------------------
-- Memory and swap, in bytes.

local function sample_memory()
  local text, err = read("/proc/meminfo", 1024 * 1024)
  if not text then return nil, err end
  local kb = {}
  for key, value in text:gmatch("([%w_()]+):%s+(%d+)") do kb[key] = tonumber(value) end
  local total = (kb.MemTotal or 0) * 1024
  -- MemAvailable is the kernel's own estimate of what a program could get
  -- without swapping; "free" is not, because the page cache is reclaimable.
  local available = (kb.MemAvailable or ((kb.MemFree or 0) + (kb.Cached or 0))) * 1024
  local swap_total = (kb.SwapTotal or 0) * 1024
  local swap_free = (kb.SwapFree or 0) * 1024
  local out = {
    total = total,
    available = available,
    used = total - available,
    free = (kb.MemFree or 0) * 1024,
    cached = ((kb.Cached or 0) + (kb.Buffers or 0) + (kb.SReclaimable or 0)) * 1024,
    percent = total > 0 and 100 * (total - available) / total or 0,
    swap = {
      total = swap_total,
      free = swap_free,
      used = swap_total - swap_free,
      percent = swap_total > 0 and 100 * (swap_total - swap_free) / swap_total or 0,
    },
  }
  ring("memory").push(out.percent)
  ring("swap").push(out.swap.percent)
  return out
end

-- ---------------------------------------------------------------------------
-- Disks: real filesystems from /proc/mounts, usage from statvfs.

-- Filesystems that live in memory or describe the kernel; nobody wants a bar
-- for /sys/fs/cgroup.
local VIRTUAL = {}
for name in ([[proc sysfs tmpfs devtmpfs devpts cgroup cgroup2 debugfs tracefs securityfs
  pstore efivarfs mqueue hugetlbfs fusectl configfs bpf autofs binfmt_misc rpc_pipefs
  nsfs ramfs selinuxfs overlay squashfs fuse.portal fuse.gvfsd-fuse fuse.snapfuse
  fuse.lxcfs tracefs devfs iso9660 nfsd]]):gmatch("%S+") do VIRTUAL[name] = true end
-- Network filesystems are real disks as far as a person is concerned.
local NETWORK = { nfs = true, nfs4 = true, cifs = true, smb3 = true, ["fuse.sshfs"] = true }

local function unescape(text)
  -- Mount points with spaces arrive as \040.
  return (text:gsub("\\(%d%d%d)", function(octal) return string.char(tonumber(octal, 8)) end))
end

local function sample_disks()
  local lines, err = fs.lines(path("/proc/mounts"))
  if not lines then return nil, err end
  local by_device, order = {}, {}
  for _, line in ipairs(lines) do
    local device, mount, kind = line:match("^(%S+) (%S+) (%S+)")
    if device and not VIRTUAL[kind]
      and (NETWORK[kind] or (device:sub(1, 5) == "/dev/" and device:sub(1, 9) ~= "/dev/loop")) then
      mount = unescape(mount)
      -- btrfs subvolumes and bind mounts repeat one device many times; the
      -- shortest mount point stands for it.
      local known = by_device[device]
      if not known then
        by_device[device] = { device = device, mount = mount, type = kind }
        order[#order + 1] = device
      elseif #mount < #known.mount then
        known.mount = mount
      end
    end
  end
  local out = {}
  for _, device in ipairs(order) do
    local disk = by_device[device]
    local usage = fs.disk(path(disk.mount))
    if usage and usage.total > 0 then
      disk.total, disk.free, disk.available = usage.total, usage.free, usage.available
      disk.used = usage.total - usage.free
      disk.percent = 100 * disk.used / (disk.used + usage.available)
      out[#out + 1] = disk
    end
  end
  table.sort(out, function(a, b) return a.mount < b.mount end)
  return out
end

-- ---------------------------------------------------------------------------
-- Temperatures: hwmon sensors and thermal zones, labelled.

-- Which sensor is "the CPU", best first: package sensors of the big vendors,
-- then the ARM and ACPI names.
local CPU_SENSORS = {
  { chip = "coretemp", label = "^Package" },
  { chip = "k10temp", label = "^Tctl" },
  { chip = "k10temp", label = "^Tdie" },
  { chip = "zenpower", label = "^Tdie" },
  { chip = "k10temp" },
  { chip = "coretemp" },
  { chip = "cpu_thermal" },
  { chip = "cpu-thermal" },
  { zone = "x86_pkg_temp" },
  { zone = "TCPU" },
  { zone = "cpu%-thermal" },
  { zone = "cpu" },
  { chip = "acpitz" },
  { zone = "acpitz" },
}

-- Found once and re-found now and then: the list of sensor files does not
-- change while the machine runs, and listing every hwmon folder costs more
-- than reading the few numbers in it.
local sensor_files
local sensor_age = 0

local function discover_sensors()
  local found = {}
  local chips = fs.list(path("/sys/class/hwmon"), { follow = true }) or {}
  for _, chip in ipairs(chips) do
    local base = "/sys/class/hwmon/" .. chip.name
    local name = text_at(base .. "/name") or chip.name
    local files = fs.list(path(base), { follow = true }) or {}
    for _, file in ipairs(files) do
      local index = file.name:match("^temp(%d+)_input$")
      if index then
        found[#found + 1] = {
          chip = name,
          label = text_at(base .. "/temp" .. index .. "_label") or (name .. " " .. index),
          input = base .. "/" .. file.name,
          critical = number_at(base .. "/temp" .. index .. "_crit"),
        }
      end
    end
  end
  local zones = fs.list(path("/sys/class/thermal"), { follow = true }) or {}
  for _, zone in ipairs(zones) do
    if zone.name:match("^thermal_zone%d+$") then
      local base = "/sys/class/thermal/" .. zone.name
      local kind = text_at(base .. "/type") or zone.name
      found[#found + 1] = { zone = kind, chip = "thermal", label = kind, input = base .. "/temp" }
    end
  end
  return found
end

local function pick_cpu(sensors)
  for _, rule in ipairs(CPU_SENSORS) do
    for _, sensor in ipairs(sensors) do
      if sensor.celsius and (
        (rule.chip and sensor.chip == rule.chip and (not rule.label or sensor.label:match(rule.label)))
        or (rule.zone and sensor.zone and sensor.zone:match("^" .. rule.zone))) then
        return sensor
      end
    end
  end
  return nil
end

local function sample_temperatures()
  if not sensor_files or sensor_age >= 60 then
    sensor_files = discover_sensors()
    sensor_age = 0
  end
  sensor_age = sensor_age + 1
  local sensors = {}
  for _, file in ipairs(sensor_files) do
    local milli = number_at(file.input)
    -- A sensor that reads nothing, or the -273 some drivers use for "none",
    -- is not a temperature.
    if milli and milli > -40000 then
      sensors[#sensors + 1] = {
        chip = file.chip, zone = file.zone, label = file.label,
        celsius = milli / 1000,
        critical = file.critical and file.critical / 1000 or nil,
      }
    end
  end
  local cpu = pick_cpu(sensors)
  local out = { sensors = sensors, cpu = cpu and cpu.celsius or nil, cpu_sensor = cpu }
  if out.cpu then ring("temperature").push(out.cpu) end
  return out
end

-- ---------------------------------------------------------------------------
-- GPU: busy percentage where the driver says (amdgpu). Intel's needs fdinfo
-- bookkeeping per client, which is a monitor of its own; it reads as nil.

local function sample_gpu()
  local out = { cards = {} }
  local cards = fs.list(path("/sys/class/drm"), { follow = true }) or {}
  for _, card in ipairs(cards) do
    if card.name:match("^card%d+$") then
      local base = "/sys/class/drm/" .. card.name .. "/device"
      local busy = number_at(base .. "/gpu_busy_percent")
      if busy then
        out.cards[#out.cards + 1] = {
          name = card.name,
          busy = busy,
          vram_used = number_at(base .. "/mem_info_vram_used"),
          vram_total = number_at(base .. "/mem_info_vram_total"),
        }
      end
    end
  end
  out.busy = out.cards[1] and out.cards[1].busy or nil
  if out.busy then ring("gpu").push(out.busy) end
  return out
end

-- ---------------------------------------------------------------------------
-- Network: bytes per second per interface, from /proc/net/dev deltas.

local last_net, last_net_at = {}, nil

local function default_interface()
  local lines = fs.lines(path("/proc/net/route")) or {}
  local best, best_metric
  for index = 2, #lines do
    local name, destination, metric = lines[index]:match("^(%S+)%s+(%x+)%s+%x+%s+%x+%s+%d+%s+%d+%s+(%d+)")
    if name and destination == "00000000" then
      metric = tonumber(metric)
      if not best_metric or metric < best_metric then best, best_metric = name, metric end
    end
  end
  return best
end

local function sample_network()
  local text, err = read("/proc/net/dev", 1024 * 1024)
  if not text then return nil, err end
  local now = morf.time.now()
  local dt = last_net_at and now - last_net_at or nil
  local out = { interfaces = {}, default = default_interface() }
  local seen = {}
  for name, rx, tx in text:gmatch("\n%s*([^%s:]+):%s*(%d+)%s+%d+%s+%d+%s+%d+%s+%d+%s+%d+%s+%d+%s+%d+%s+(%d+)") do
    rx, tx = tonumber(rx), tonumber(tx)
    local before = last_net[name]
    local entry = { name = name, rx_bytes = rx, tx_bytes = tx, rx_rate = 0, tx_rate = 0 }
    -- A counter that went backwards is an interface that was reset.
    if before and dt and dt > 0 and rx >= before.rx and tx >= before.tx then
      entry.rx_rate = (rx - before.rx) / dt
      entry.tx_rate = (tx - before.tx) / dt
    end
    seen[name] = { rx = rx, tx = tx }
    if name ~= "lo" then out.interfaces[#out.interfaces + 1] = entry end
    if name == out.default then out.primary = entry end
  end
  last_net, last_net_at = seen, now
  local primary = out.primary or { rx_rate = 0, tx_rate = 0 }
  out.rx_rate, out.tx_rate = primary.rx_rate, primary.tx_rate
  ring("rx").push(out.rx_rate)
  ring("tx").push(out.tx_rate)
  return out
end

-- ---------------------------------------------------------------------------
-- Battery and AC, for machines without UPower.

local function sample_battery()
  local supplies = fs.list(path("/sys/class/power_supply"), { follow = true }) or {}
  local out = { batteries = {}, ac = nil, present = false }
  local energy_now, energy_full, power = 0, 0, 0
  for _, supply in ipairs(supplies) do
    local base = "/sys/class/power_supply/" .. supply.name
    local kind = text_at(base .. "/type")
    local scope = text_at(base .. "/scope")
    if kind == "Mains" then
      out.ac = (number_at(base .. "/online") or 0) == 1
    elseif kind == "Battery" and scope ~= "Device" then
      -- energy_* are in µWh, charge_* in µAh; either works for a percentage,
      -- and µAh times the voltage makes µWh for the time estimate.
      local voltage = (number_at(base .. "/voltage_now") or 0) / 1e6
      local now = number_at(base .. "/energy_now")
      local full = number_at(base .. "/energy_full")
      local rate = number_at(base .. "/power_now")
      if not now then
        local charge = number_at(base .. "/charge_now")
        local charge_full = number_at(base .. "/charge_full")
        local current = number_at(base .. "/current_now")
        now = charge and charge * voltage
        full = charge_full and charge_full * voltage
        rate = current and current * voltage
      end
      local battery = {
        name = supply.name,
        status = text_at(base .. "/status") or "Unknown",
        capacity = number_at(base .. "/capacity"),
        energy = now and now / 1e6 or nil,         -- Wh
        energy_full = full and full / 1e6 or nil,  -- Wh
        power = rate and math.abs(rate) / 1e6 or nil, -- W
      }
      if not battery.capacity and now and full and full > 0 then
        battery.capacity = 100 * now / full
      end
      out.batteries[#out.batteries + 1] = battery
      energy_now = energy_now + (now or 0)
      energy_full = energy_full + (full or 0)
      power = power + (rate and math.abs(rate) or 0)
    end
  end
  local first = out.batteries[1]
  if first then
    out.present = true
    out.status = first.status
    out.percent = energy_full > 0 and 100 * energy_now / energy_full or first.capacity
    out.power = power / 1e6
    out.charging = first.status == "Charging"
    -- Hours left, or to full, at the present draw.
    if power > 0 then
      if first.status == "Discharging" then
        out.time_left = 3600 * energy_now / power
      elseif first.status == "Charging" then
        out.time_to_full = 3600 * (energy_full - energy_now) / power
      end
    end
  end
  return out
end

-- ---------------------------------------------------------------------------
-- Backlight. Reading is free; writing needs the file to be writable, which
-- is usually a udev rule giving the `video` group write access.

local own_groups
local function in_group_of(name)
  if not own_groups then
    own_groups = {}
    local status = read("/proc/self/status", 64 * 1024) or ""
    local uid = status:match("\nUid:%s+(%d+)")
    own_groups.root = uid == "0"
    for gid in (status:match("\nGroups:([^\n]*)") or ""):gmatch("%d+") do own_groups[gid] = true end
  end
  if own_groups.root then return true end
  local key = "name:" .. name
  if own_groups[key] == nil then
    local groups = "\n" .. (read("/etc/group", 1024 * 1024) or "")
    local gid = groups:match("\n" .. name .. ":[^:]*:(%d+):")
    own_groups[key] = gid ~= nil and own_groups[gid] == true
  end
  return own_groups[key]
end

local function writable(file)
  local stat = fs.stat(path(file))
  if not stat then return false end
  local mode = stat.mode
  if math.floor(mode / 2) % 2 == 1 then return true end         -- o+w
  -- Group writable: the conventional rule gives it to `video`, and fs.stat
  -- does not say which group owns the file, so that is the one asked about.
  if math.floor(mode / 16) % 2 == 1 and in_group_of("video") then return true end
  return own_groups ~= nil and own_groups.root == true
end

local function sample_backlight()
  local devices = fs.list(path("/sys/class/backlight"), { follow = true }) or {}
  local out = { devices = {} }
  for _, device in ipairs(devices) do
    local base = "/sys/class/backlight/" .. device.name
    local now = number_at(base .. "/brightness")
    local max = number_at(base .. "/max_brightness")
    if now and max and max > 0 then
      out.devices[#out.devices + 1] = {
        name = device.name,
        brightness = now,
        max = max,
        percent = 100 * now / max,
        writable = writable(base .. "/brightness"),
      }
    end
  end
  local first = out.devices[1]
  if first then
    out.name, out.brightness, out.max, out.percent, out.writable =
      first.name, first.brightness, first.max, first.percent, first.writable
    -- The setter is only handed out when it can work, so a slider can
    -- show itself read-only by asking whether `set` is there.
    if first.writable then
      out.set = function(percent) return sysinfo.set_brightness(percent, first.name) end
    end
  end
  return out
end

-- ---------------------------------------------------------------------------
-- The system: names and uptime.

local static_system

local function sample_system()
  if not static_system then
    local release = {}
    local text = read("/etc/os-release", 64 * 1024) or read("/usr/lib/os-release", 64 * 1024) or ""
    for key, value in text:gmatch("([%w_]+)=([^\n]*)") do
      release[key] = value:match('^"(.*)"$') or value:match("^'(.*)'$") or value
    end
    static_system = {
      os = release.PRETTY_NAME or release.NAME or "Linux",
      os_id = release.ID,
      os_version = release.VERSION_ID,
      os_release = release,
      kernel = text_at("/proc/sys/kernel/osrelease") or "",
      user = morf.env("USER") or morf.env("LOGNAME") or "",
    }
  end
  local uptime = number_at("/proc/uptime") or 0
  return {
    os = static_system.os,
    os_id = static_system.os_id,
    os_version = static_system.os_version,
    os_release = static_system.os_release,
    kernel = static_system.kernel,
    user = static_system.user,
    -- The hostname can change under a running shell; it is one small read.
    hostname = text_at("/proc/sys/kernel/hostname") or "",
    uptime = uptime,
  }
end

-- ---------------------------------------------------------------------------
-- Processes: the top few by CPU and by memory, from /proc/[pid]/stat.

local last_proc, last_proc_total = {}, nil

local function total_jiffies()
  local text = read("/proc/stat", 1024 * 1024)
  if not text then return nil end
  local sum = 0
  for value in (text:match("^cpu%s+([^\n]+)") or ""):gmatch("%d+") do sum = sum + tonumber(value) end
  return sum
end

-- Page size is 4 KiB on everything this is likely to run on; the kernel does
-- not put it anywhere in /proc cheaper to read than assuming it.
local PAGE = 4096

local function sample_processes(done)
  poll.job(function(spend)
    local entries = fs.list(path("/proc")) or {}
    local total = total_jiffies()
    local cores = math.max(1, (sysinfo.cpu_count or 1))
    local memory = read("/proc/meminfo", 64 * 1024)
    local mem_total = memory and tonumber(memory:match("MemTotal:%s+(%d+)")) or 0
    local elapsed = (total and last_proc_total) and (total - last_proc_total) or 0
    local seen, count = {}, 0
    local top = sysinfo.top
    -- The few largest, kept sorted as the scan goes: sorting every process
    -- would cost more than reading them.
    local by_cpu, by_memory = {}, {}
    local function keep(list, process, key)
      local size = #list
      if size >= top and list[size][key] >= process[key] then return end
      local index = math.min(size, top - 1) + 1
      while index > 1 and list[index - 1][key] < process[key] do
        list[index] = list[index - 1]
        index = index - 1
      end
      list[index] = process
      if #list > top then list[#list] = nil end
    end
    for _, entry in ipairs(entries) do
      local pid = entry.name:match("^%d+$") and entry.name
      if pid then
        -- The read, three matches, four conversions, two rankings, this.
        spend(12)
        local stat = read("/proc/" .. pid .. "/stat", 4096)
        local name, rest
        if stat then name, rest = stat:match("^%d+ %((.*)%) (.*)$") end
        if rest then
          local state, utime, stime, rss = rest:match(
            "^(%S) %S+ %S+ %S+ %S+ %S+ %S+ %S+ %S+ %S+ %S+ (%d+) (%d+) %S+ %S+ %S+ %S+ %S+ %S+ %S+ %S+ (%d+)")
          if utime then
            local used = tonumber(utime) + tonumber(stime)
            seen[pid] = used
            local before = last_proc[pid]
            local cpu = 0
            -- Percent of one core, the way top counts it.
            if before and elapsed > 0 then cpu = 100 * (used - before) * cores / elapsed end
            local bytes = tonumber(rss) * PAGE
            local process = {
              pid = tonumber(pid), name = name, state = state,
              cpu = cpu, memory = bytes,
              memory_percent = mem_total > 0 and 100 * bytes / (mem_total * 1024) or 0,
            }
            count = count + 1
            keep(by_cpu, process, "cpu")
            keep(by_memory, process, "memory")
          end
        end
      end
    end
    last_proc, last_proc_total = seen, total
    return { count = count, by_cpu = by_cpu, by_memory = by_memory }
  end, function(value, err) done(value, err) end)
end

-- ---------------------------------------------------------------------------
-- Sources

local EMPTY = {
  cpu = { usage = 0, cores = {}, count = 0, frequency = 0, frequencies = {}, load = { 0, 0, 0 }, model = "" },
  memory = { total = 0, available = 0, used = 0, free = 0, cached = 0, percent = 0,
    swap = { total = 0, free = 0, used = 0, percent = 0 } },
  disks = {},
  temperatures = { sensors = {} },
  gpu = { cards = {} },
  network = { interfaces = {}, rx_rate = 0, tx_rate = 0 },
  battery = { batteries = {}, present = false },
  backlight = { devices = {} },
  system = { os = "", kernel = "", hostname = "", user = "", uptime = 0 },
  processes = { count = 0, by_cpu = {}, by_memory = {} },
}

local SAMPLERS = {
  cpu = { sample_cpu, 2000 },
  memory = { sample_memory, 3000 },
  disks = { sample_disks, 30000 },
  temperatures = { sample_temperatures, 5000 },
  gpu = { sample_gpu, 2000 },
  network = { sample_network, 2000 },
  battery = { sample_battery, 30000 },
  backlight = { sample_backlight, 5000 },
  system = { sample_system, 60000 },
}

local sources = {}

for name, entry in pairs(SAMPLERS) do
  local sample = entry[1]
  sources[name] = poll.source {
    name = "sysinfo." .. name,
    interval = entry[2],
    initial = EMPTY[name],
    sample = function(done) done(sample()) end,
  }
end
sources.processes = poll.source {
  name = "sysinfo.processes",
  interval = 5000,
  initial = EMPTY.processes,
  sample = function(done)
    -- The core count scales per-process percentages; take it from the CPU
    -- section when that has run, else count the lines once.
    if not sysinfo.cpu_count then
      local text = read("/proc/stat", 1024 * 1024) or ""
      local count = 0
      for _ in text:gmatch("\ncpu%d+") do count = count + 1 end
      sysinfo.cpu_count = math.max(count, 1)
    end
    sample_processes(done)
  end,
}

--- The sources, by name, for `pin`, `refresh`, `running` and `error`.
sysinfo.sources = sources

--- CPU: `{ usage, cores = { {name, usage, frequency} }, count, frequency (MHz,
--- mean), frequencies, load = {1, 5, 15}, running, threads, model }`.
function sysinfo.cpu()
  local value = sources.cpu:get()
  if value.count and value.count > 0 then sysinfo.cpu_count = value.count end
  return value
end
--- Memory in bytes: `{ total, available, used, free, cached, percent,
--- swap = { total, free, used, percent } }`.
function sysinfo.memory() return sources.memory:get() end
--- Real filesystems: `{ {device, mount, type, total, used, free, available, percent} }`.
function sysinfo.disks() return sources.disks:get() end
--- `{ cpu (°C or nil), cpu_sensor, sensors = { {chip, label, celsius, critical} } }`.
function sysinfo.temperatures() return sources.temperatures:get() end
--- `{ busy (percent or nil), cards = { {name, busy, vram_used, vram_total} } }`.
function sysinfo.gpu() return sources.gpu:get() end
--- `{ default, primary, rx_rate, tx_rate (bytes/s of the default interface),
--- interfaces = { {name, rx_bytes, tx_bytes, rx_rate, tx_rate} } }`.
function sysinfo.network() return sources.network:get() end
--- `{ present, percent, status, charging, power (W), time_left, time_to_full
--- (seconds), ac, batteries = { ... } }`.
function sysinfo.battery() return sources.battery:get() end
--- `{ name, brightness, max, percent, writable, set(percent) (only when
--- writable), devices = { ... } }`.
function sysinfo.backlight() return sources.backlight:get() end
--- `{ os, os_id, os_version, os_release, kernel, hostname, user, uptime (s) }`.
function sysinfo.system() return sources.system:get() end
--- `{ count, by_cpu = { {pid, name, state, cpu, memory, memory_percent} }, by_memory }`.
function sysinfo.processes() return sources.processes:get() end

--- The last `history_size` samples of one series, oldest first: `cpu`,
--- `load`, `memory`, `swap`, `temperature`, `gpu`, `rx`, `tx`, `core0`...
--- Reading it follows (and wakes) the section that feeds it.
local FEEDS = { cpu = "cpu", load = "cpu", memory = "memory", swap = "memory",
  temperature = "temperatures", gpu = "gpu", rx = "network", tx = "network" }
function sysinfo.history(name)
  local feed = FEEDS[name] or (name:match("^core%d+$") and "cpu")
  if not feed then error("sysinfo.history: no series " .. tostring(name), 2) end
  sources[feed]:get()
  return ring(name).list()
end

--- Sets the backlight to `percent` (0-100) of its range. Only there when the
--- first backlight's file is writable; call `sysinfo.backlight()` first (a
--- binding does), since that is what finds out. Returns true or nil, message.
function sysinfo.set_brightness(percent, device)
  local light = sources.backlight.value
  local target
  for _, candidate in ipairs(light and light.devices or {}) do
    if not device or candidate.name == device then target = candidate break end
  end
  if not target then return nil, "no backlight " .. tostring(device) end
  if not target.writable then return nil, "the backlight " .. target.name .. " is not writable" end
  local value = math.floor(math.max(0, math.min(100, percent)) * target.max / 100 + 0.5)
  local ok, err = fs.write(path("/sys/class/backlight/" .. target.name .. "/brightness"),
    tostring(value), { atomic = false, parents = false })
  if not ok then return nil, err end
  sources.backlight:refresh()
  return true
end

--- Takes one section's sample now and returns it, without waiting for the
--- timer (processes excepted: those arrive later). For scripts and tests.
function sysinfo.sample(name)
  local entry = SAMPLERS[name]
  if not entry then error("sysinfo.sample: no section " .. tostring(name), 2) end
  local value, err = entry[1]()
  if value then sources[name]:publish(value) end
  return value, err
end

--- Options: `root`, `history` (samples kept), `top` (processes listed) and
--- `intervals = { cpu = ms, ... }`.
function sysinfo.configure(options)
  if options.root then
    sysinfo.root = options.root
    -- Everything remembered came from the old root.
    sensor_files, cpu_model, static_system, own_groups = nil, nil, nil, nil
    last_stat, last_net, last_net_at, last_proc, last_proc_total = {}, {}, nil, {}, nil
  end
  if options.history then
    sysinfo.history_size = options.history
    rings = {}
  end
  if options.top then sysinfo.top = options.top end
  for name, ms in pairs(options.intervals or {}) do
    if not sources[name] then error("sysinfo.configure: no section " .. name, 2) end
    sources[name]:set_interval(ms)
  end
end

return sysinfo
