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
--   local sysinfo = require("lib.services.sysinfo")
--   ui.Text { text = function() return ("%d%%"):format(sysinfo.cpu().usage) end }
--   sysinfo.configure { intervals = { cpu = 1000 }, history = 120 }
--
-- Paths start at `sysinfo.root` ("" -- the real machine), so a test can put a
-- fake /proc and /sys in a folder and point the library at it.

local morf = require("morf")
local poll = require("lib.util.poll")

-- Sensor files may block in the driver. A polling sample reads them on
-- Morf's I/O workers and resumes on completion, keeping input/paint free.
-- History belongs to the individual coroutine while samples overlap.
local jobs = setmetatable({}, { __mode = "k" })
local fs = setmetatable({}, { __index = morf.fs })
fs.read = function(filename, limit)
  if jobs[coroutine.running()] then return coroutine.yield(filename, limit) end
  return morf.fs.read(filename, limit)
end

-- Long proc tables also cost CPU time. Leave a turn for input between
-- small groups of rows, just as file reads leave a turn while I/O runs.
local function yield_sample()
  if jobs[coroutine.running()] then coroutine.yield(false) end
end

local function sample_async(sample, done)
  local thread = coroutine.create(function()
    local self = coroutine.running()
    local job = { pushes = {} }
    jobs[self] = job
    local ok, value, err = pcall(sample)
    jobs[self] = nil
    if not ok then error(value, 0) end
    return { value = value, error = err, pushes = job.pushes }
  end)
  local function resume(...)
    local ok, value, limit = coroutine.resume(thread, ...)
    if not ok then jobs[thread] = nil done(nil, tostring(value)) return end
    if coroutine.status(thread) == "dead" then
      done(value.value, value.error, value.pushes)
      return
    end
    if value == false then morf.timer(1, resume, false) return end
    local submitted, err = morf.fs.read_async({ value }, function(success, contents)
      if success and type(contents[1]) == "string" then resume(contents[1])
      else resume(nil, "cannot read " .. value) end
    end, limit)
    if not submitted then jobs[thread] = nil done(nil, err) end
  end
  morf.timer(1, resume, false)
end

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
-- The pushes a sample makes while it is recorded, for the screens that read
-- the sample rather than take it (a shared source's `mirrored`).

-- Each series is a ring for `history` and a data channel (morf.channel)
-- for a chart to draw with no Lua: both take every sample.
local function ring(name)
  local found = rings[name]
  if not found then
    local kept = poll.ring(sysinfo.history_size)
    local channel = morf.channel { size = sysinfo.history_size }
    found = { list = kept.list, channel = channel,
      push = function(value) kept.push(value) channel:push(tonumber(value) or 0) end }
    rings[name] = found
  end
  local job = jobs[coroutine.running()]
  if job then
    local log = job.pushes
    return { push = function(value) log[#log + 1] = { name, value } found.push(value) end }
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
    committed = (kb.Committed_AS or 0) * 1024,
    commit_limit = (kb.CommitLimit or 0) * 1024,
    buffers = (kb.Buffers or 0) * 1024,
    shared = (kb.Shmem or 0) * 1024,
    dirty = (kb.Dirty or 0) * 1024,
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
-- GPUs, every card: busy where the driver says it (amdgpu's
-- gpu_busy_percent; i915/xe from how long the GPU spent in RC6, asleep),
-- clocks, video memory, and for a card that is powered down (a laptop's
-- discrete GPU, most of the time) only that: reading it would wake it.

local last_rc6 = {}
local gpu_names = {}
local pci_ids -- /usr/share/hwdata/pci.ids, read once: false when absent

-- The card's name from the PCI id database -- a file, never the device:
-- asking the device (lspci does) wakes a GPU that is powered down.
local function gpu_name(card, vendor, device)
  local known = gpu_names[card]
  if known then return known end
  local v = tostring(vendor or ""):gsub("^0x", ""):lower()
  local d = tostring(device or ""):gsub("^0x", ""):lower()
  if pci_ids == nil then
    pci_ids = read("/usr/share/hwdata/pci.ids", 8 * 1024 * 1024)
      or read("/usr/share/misc/pci.ids", 8 * 1024 * 1024) or false
  end
  local name
  if pci_ids and v ~= "" and d ~= "" then
    local at = pci_ids:find("\n" .. v .. "  ", 1, true)
    if at then
      local next_vendor = pci_ids:find("\n%x%x%x%x  ", at + 1) or #pci_ids
      local line = pci_ids:find("\n\t" .. d .. "  ", at, true)
      if line and line < next_vendor then
        name = pci_ids:match("^([^\n]+)", line + 8)
      end
    end
  end
  if name then
    -- "TigerLake-H GT1 [UHD Graphics]" -> "UHD Graphics"; "GA107M [GeForce
    -- RTX 3050 Ti Mobile]" -> "GeForce RTX 3050 Ti Mobile".
    name = name:match("%[(.-)%]") or name
  end
  gpu_names[card] = name or ("%s:%s"):format(v, d)
  return gpu_names[card]
end

-- NVIDIA's own driver publishes no load in sysfs; its tool does. Asked
-- off the loop, for cards that are awake, and read on the next sample.
local nvidia = { by_slot = {}, asking = false }
local NVIDIA_FIELDS = "pci.bus_id,utilization.gpu,utilization.encoder,utilization.decoder,memory.used,memory.total,"
  .. "temperature.gpu,power.draw,power.limit,clocks.gr,clocks.max.gr,clocks.mem,clocks.max.mem,driver_version"

local function ask_nvidia()
  if nvidia.asking or not morf.run then return end
  nvidia.asking = true
  local ok = pcall(morf.run, { "nvidia-smi", "--query-gpu=" .. NVIDIA_FIELDS, "--format=csv,noheader,nounits" }, {},
    function(result)
      nvidia.asking = false
      if not (result and result.ok) then return end
      for line in tostring(result.stdout or ""):gmatch("[^\n]+") do
        local f = {}
        for field in (line .. ","):gmatch("%s*([^,]*),") do f[#f + 1] = field end
        local function num(i) return tonumber(f[i]) end
        -- "00000000:01:00.0" -> "0000:01:00.0", as sysfs names it.
        local slot = tostring(f[1] or ""):lower():match("(%x%x%x%x:%x%x:%x%x%.%x)$")
        if slot then
          nvidia.by_slot[slot] = {
            busy = num(2), encoder = num(3), decoder = num(4),
            vram_used = num(5) and num(5) * 1024 * 1024, vram_total = num(6) and num(6) * 1024 * 1024,
            temperature = num(7), power = num(8), power_limit = num(9),
            clock_mhz = num(10), max_mhz = num(11), memory_clock_mhz = num(12), memory_max_mhz = num(13),
            driver_version = f[14],
          }
        end
      end
    end)
  if not ok then nvidia.asking = false end
end

local VENDORS = { ["0x8086"] = "Intel", ["0x10de"] = "NVIDIA", ["0x1002"] = "AMD" }

local function sample_gpu()
  local out = { cards = {} }
  local now = morf.time.now()
  local cards = fs.list(path("/sys/class/drm"), { follow = true }) or {}
  table.sort(cards, function(a, b) return a.name < b.name end)
  for _, card in ipairs(cards) do
    if card.name:match("^card%d+$") then
      local base = "/sys/class/drm/" .. card.name .. "/device"
      local vendor_id = text_at(base .. "/vendor")
      local uevent = read(base .. "/uevent", 4096) or ""
      local slot = uevent:match("PCI_SLOT_NAME=(%S+)")
      local runtime = text_at(base .. "/power/runtime_status")
      local entry = {
        name = card.name,
        vendor = VENDORS[vendor_id or ""] or vendor_id or "",
        model = gpu_name(card.name, vendor_id, text_at(base .. "/device")),
        driver = uevent:match("DRIVER=(%S+)") or "",
        slot = slot or "",
        suspended = runtime == "suspended",
        key = "gpu:" .. card.name,
      }
      if not entry.suspended then
        entry.busy = number_at(base .. "/gpu_busy_percent")
        entry.vram_used = number_at(base .. "/mem_info_vram_used")
        entry.vram_total = number_at(base .. "/mem_info_vram_total")
        -- i915 and xe: the time asleep, as a share of the time since last.
        local gt = "/sys/class/drm/" .. card.name .. "/gt/gt0"
        local rc6 = number_at(gt .. "/rc6_residency_ms")
        if rc6 then
          local before = last_rc6[card.name]
          if before and now > before.at then
            local asleep = (rc6 - before.ms) / 1000 / (now - before.at)
            entry.busy = math.max(0, math.min(100, 100 * (1 - asleep)))
          end
          last_rc6[card.name] = { ms = rc6, at = now }
          entry.clock_mhz = number_at(gt .. "/rps_act_freq_mhz")
          entry.max_mhz = number_at(gt .. "/rps_max_freq_mhz") or number_at(gt .. "/rps_RP0_freq_mhz")
        end
        entry.clock_mhz = entry.clock_mhz or number_at(base .. "/pp_dpm_sclk_mhz")
        if entry.driver == "nvidia" then
          ask_nvidia()
          for key, value in pairs(nvidia.by_slot[slot or ""] or {}) do entry[key] = value end
        end
      end
      out.cards[#out.cards + 1] = entry
      ring(entry.key).push(entry.busy or 0)
      ring("gpumem:" .. card.name).push(entry.vram_total and entry.vram_total > 0
        and 100 * (entry.vram_used or 0) / entry.vram_total or 0)
      ring("gpuenc:" .. card.name).push(entry.encoder or 0)
      ring("gpudec:" .. card.name).push(entry.decoder or 0)
    end
  end
  for _, entry in ipairs(out.cards) do
    if entry.busy then out.busy = entry.busy break end
  end
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
    if name ~= "lo" then
      local base = "/sys/class/net/" .. name
      entry.wireless = fs.exists(path(base .. "/wireless")) or false
      entry.virtual = not fs.exists(path(base .. "/device"))
      entry.address = text_at(base .. "/address") or ""
      entry.state = text_at(base .. "/operstate") or ""
      local speed = number_at(base .. "/speed")
      entry.speed = speed and speed > 0 and speed or nil
      ring("rx:" .. name).push(entry.rx_rate)
      ring("tx:" .. name).push(entry.tx_rate)
      out.interfaces[#out.interfaces + 1] = entry
    end
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
      -- As designed, for the health: energy_full_design, or the charge's
      -- at the design voltage (else the present one).
      local design = number_at(base .. "/energy_full_design")
      if not design then
        local charge_design = number_at(base .. "/charge_full_design")
        local v_design = (number_at(base .. "/voltage_min_design") or 0) / 1e6
        if v_design <= 0 then v_design = voltage end
        design = charge_design and charge_design * v_design
      end
      local status = text_at(base .. "/status") or "Unknown"
      local temp = number_at(base .. "/temp")
      local cycles = number_at(base .. "/cycle_count")
      -- The kinds of charging it knows ("[Trickle] Fast Standard ..."), the
      -- bracketed one on.
      local modes, mode = {}, nil
      for word in (text_at(base .. "/charge_types") or ""):gmatch("%S+") do
        local on = word:match("^%[(.+)%]$")
        modes[#modes + 1] = on or word
        if on then mode = on end
      end
      local battery = {
        name = supply.name,
        status = status,
        capacity = number_at(base .. "/capacity"),
        energy = now and now / 1e6 or nil,         -- Wh
        energy_full = full and full / 1e6 or nil,  -- Wh
        energy_design = design and design / 1e6 or nil, -- Wh
        power = rate and math.abs(rate) / 1e6 or nil, -- W
        -- Signed: negative while it drains.
        rate = rate and (status == "Discharging" and -1 or 1) * math.abs(rate) / 1e6 or nil,
        voltage = voltage > 0 and voltage or nil,  -- V
        voltage_design = (number_at(base .. "/voltage_min_design") or 0) / 1e6,
        temperature = temp and temp / 10 or nil,   -- °C (the driver's tenths)
        cycles = (cycles and cycles > 0) and cycles or nil,
        vendor = text_at(base .. "/manufacturer") or "",
        model = text_at(base .. "/model_name") or "",
        serial = text_at(base .. "/serial_number") or "",
        technology = text_at(base .. "/technology") or "",
        level = text_at(base .. "/capacity_level") or "",
        charge_limit = number_at(base .. "/charge_control_end_threshold"),
        charge_start = number_at(base .. "/charge_control_start_threshold"),
        charge_modes = modes, charge_mode = mode,
      }
      if not battery.capacity and now and full and full > 0 then
        battery.capacity = 100 * now / full
      end
      if battery.energy_full and battery.energy_design and battery.energy_design > 0 then
        battery.health = math.min(100, 100 * battery.energy_full / battery.energy_design)
      end
      local key = "bat:" .. supply.name
      ring(key .. ":percent").push(battery.capacity or 0)
      ring(key .. ":power").push(battery.power or 0)
      ring(key .. ":voltage").push(battery.voltage or 0)
      ring(key .. ":temperature").push(battery.temperature or 0)
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
-- Drives: each physical disk's activity from /proc/diskstats deltas, and its
-- logical units -- partitions, and the device-mapper volumes (LVM, LUKS) on
-- them -- each with its own rates, its mounts and their space.

local SECTOR = 512
local last_io, last_io_at = {}, nil
-- What a drive is does not change while the machine runs: found once.
local drive_facts = {}

-- Whole disks worth listing: not loop, RAM, zram, optical or mapper devices.
local function is_drive(name)
  return not (name:match("^loop") or name:match("^ram") or name:match("^zram")
    or name:match("^sr%d") or name:match("^dm%-"))
end

local function block_size(base)
  return (number_at(base .. "/size") or 0) * SECTOR
end

local function facts(name)
  local known = drive_facts[name]
  if known then return known end
  local base = "/sys/block/" .. name
  local rotational = number_at(base .. "/queue/rotational") == 1
  local kind = name:match("^nvme") and "NVMe" or name:match("^mmcblk") and "SD/MMC"
    or rotational and "HDD" or "SSD"
  known = {
    model = text_at(base .. "/device/model") or text_at(base .. "/device/name") or "",
    vendor = text_at(base .. "/device/vendor") or "",
    capacity = block_size(base),
    removable = number_at(base .. "/removable") == 1,
    kind = kind,
  }
  drive_facts[name] = known
  return known
end

-- The mapper name of a dm-N device ("sys-arch"), or nil.
local function dm_name(name)
  if not name:match("^dm%-%d+$") then return nil end
  return text_at("/sys/block/" .. name .. "/dm/name")
end

-- Every device node a mount may name for `name`: /dev/X, and for a mapper
-- volume /dev/mapper/NAME and /dev/VG/LV.
local function aliases(name)
  local list = { "/dev/" .. name }
  local mapped = dm_name(name)
  if mapped then
    list[#list + 1] = "/dev/mapper/" .. mapped
    local vg, lv = mapped:match("^(.-[^-])%-([^-].*)$")
    if vg then list[#list + 1] = "/dev/" .. vg:gsub("%-%-", "-") .. "/" .. lv:gsub("%-%-", "-") end
  end
  return list
end

local function read_mounts()
  local by = {}
  for _, line in ipairs(fs.lines(path("/proc/mounts")) or {}) do
    local device, mount = line:match("^(%S+)%s+(%S+)")
    if device then
      by[device] = by[device] or {}
      local list = by[device]
      list[#list + 1] = (mount:gsub("\\(%d%d%d)", function(o) return string.char(tonumber(o, 8)) end))
    end
  end
  local swaps = {}
  for index, line in ipairs(fs.lines(path("/proc/swaps")) or {}) do
    local device = index > 1 and line:match("^(%S+)")
    if device then swaps[device] = true end
  end
  return by, swaps
end

local function sample_drives()
  local text, err = read("/proc/diskstats", 1024 * 1024)
  if not text then return nil, err end
  local now = morf.time.now()
  local dt = last_io_at and now - last_io_at or nil
  local io = {}
  -- Match once from each line's start. Modern kernels append discard/flush
  -- counters; a global partial match would retry the disk prefix at every
  -- character of those trailing counters, stalling the UI on partitioned UFS.
  local rows = 0
  for line in text:gmatch("[^\n]+") do
    rows = rows + 1
    if rows % 8 == 0 then yield_sample() end
    local name, rd, wr, ticks = line:match(
      "^%s*%d+%s+%d+%s+(%S+)%s+%d+%s+%d+%s+(%d+)%s+%d+%s+%d+%s+%d+%s+(%d+)%s+%d+%s+%d+%s+(%d+)")
    if name then
      local now_io = { read = tonumber(rd) * SECTOR, write = tonumber(wr) * SECTOR, ticks = tonumber(ticks) }
      local before = last_io[name]
      local rates = { read_total = now_io.read, write_total = now_io.write, read_rate = 0, write_rate = 0, busy = 0 }
      if before and dt and dt > 0 and now_io.read >= before.read and now_io.write >= before.write then
        rates.read_rate = (now_io.read - before.read) / dt
        rates.write_rate = (now_io.write - before.write) / dt
        rates.busy = math.min(100, 100 * (now_io.ticks - before.ticks) / 1000 / dt)
      end
      io[name] = rates
      last_io[name] = now_io
    end
  end
  last_io_at = now
  local mounts, swaps = read_mounts()
  local function unit(name, base, depth)
    local rates = io[name] or {}
    local out = {
      name = name, label = dm_name(name) or name, depth = depth,
      size = block_size(base), mounts = {}, swap = false,
      read_rate = rates.read_rate or 0, write_rate = rates.write_rate or 0,
    }
    for _, alias in ipairs(aliases(name)) do
      for _, mount in ipairs(mounts[alias] or {}) do out.mounts[#out.mounts + 1] = mount end
      if swaps[alias] then out.swap = true end
    end
    return out
  end
  local drives = {}
  local root_disk
  -- diskstats already names every disk and partition. Listing sysfs again
  -- stats dozens of unrelated attributes per drive, blocking the UI on ARM.
  local names = {}
  for name in pairs(io) do names[#names + 1] = name end
  table.sort(names)
  for index, name in ipairs(names) do
    if index % 8 == 0 then yield_sample() end
    if is_drive(name) and fs.exists(path("/sys/block/" .. name)) then
      local base = "/sys/block/" .. name
      local drive = { name = name, units = {} }
      for key, value in pairs(facts(name)) do drive[key] = value end
      for key, value in pairs(io[name]) do drive[key] = value end
      -- Partitions, each followed by what is built on it, depth first.
      local children = {}
      for _, child in ipairs(names) do
        if child ~= name and child:find(name, 1, true) == 1 and fs.exists(path(base .. "/" .. child)) then
          children[#children + 1] = child
        end
      end
      table.sort(children)
      local function holders(of, of_base, depth, seen)
        for _, holder in ipairs(fs.list(path(of_base .. "/holders"), { follow = true }) or {}) do
          if not seen[holder.name] then
            seen[holder.name] = true
            local u = unit(holder.name, "/sys/block/" .. holder.name, depth)
            drive.units[#drive.units + 1] = u
            holders(holder.name, "/sys/block/" .. holder.name, depth + 1, seen)
          end
        end
      end
      local seen = {}
      local whole = unit(name, base, 0)
      if #whole.mounts > 0 or whole.swap then drive.units[#drive.units + 1] = whole end
      holders(name, base, 1, seen)
      for _, child in ipairs(children) do
        drive.units[#drive.units + 1] = unit(child, base .. "/" .. child, 1)
        holders(child, base .. "/" .. child, 2, seen)
      end
      for _, u in ipairs(drive.units) do
        for _, mount in ipairs(u.mounts) do if mount == "/" then root_disk = name end end
      end
      ring("disk:" .. name .. ":busy").push(drive.busy)
      ring("disk:" .. name .. ":read").push(drive.read_rate)
      ring("disk:" .. name .. ":write").push(drive.write_rate)
      drives[#drives + 1] = drive
    end
  end
  table.sort(drives, function(a, b) return a.name < b.name end)
  for _, drive in ipairs(drives) do drive.system = drive.name == root_disk end
  return { drives = drives }
end

-- ---------------------------------------------------------------------------
-- Fans: hwmon fan inputs in RPM, labelled.

local fan_files

local function sample_fans()
  if not fan_files then
    fan_files = {}
    for _, chip in ipairs(fs.list(path("/sys/class/hwmon"), { follow = true }) or {}) do
      local base = "/sys/class/hwmon/" .. chip.name
      local chip_name = text_at(base .. "/name") or chip.name
      for _, file in ipairs(fs.list(path(base), { follow = true }) or {}) do
        local index = file.name:match("^fan(%d+)_input$")
        if index then
          fan_files[#fan_files + 1] = {
            chip = chip_name,
            label = text_at(base .. "/fan" .. index .. "_label") or ("Fan " .. index),
            input = base .. "/" .. file.name,
            max = number_at(base .. "/fan" .. index .. "_max"),
          }
        end
      end
    end
  end
  local out, seen, per_chip = { fans = {} }, {}, {}
  for _, file in ipairs(fan_files) do
    local rpm = number_at(file.input)
    -- Two drivers often report the same fans (dell_smm and dell_ddv, each
    -- labelled its own way): the nth fan of a second chip reading exactly
    -- what the nth of an earlier one reads is that fan again.
    per_chip[file.chip] = (per_chip[file.chip] or 0) + 1
    local nth = per_chip[file.chip]
    -- Sampled a moment apart, the same fan reads a few RPM differently.
    local again = rpm and seen[nth] and seen[nth].chip ~= file.chip
      and math.abs(seen[nth].rpm - rpm) <= math.max(50, 0.05 * rpm)
    if rpm and not again then
      if not seen[nth] then seen[nth] = { chip = file.chip, rpm = rpm } end
      local index = #out.fans
      out.fans[index + 1] = { chip = file.chip, label = file.label, rpm = rpm, max = file.max, key = "fan" .. index }
      ring("fan" .. index).push(rpm)
    end
  end
  return out
end

-- ---------------------------------------------------------------------------
-- The CPU's facts that do not change: caches, clocks, governor, features.

local cpu_facts
function sysinfo.cpu_info()
  if cpu_facts then return cpu_facts end
  local info = read("/proc/cpuinfo", 1024 * 1024) or ""
  local flags = info:match("flags%s*:%s*([^\n]+)") or ""
  local sockets = {}
  for id in info:gmatch("physical id%s*:%s*(%d+)") do sockets[id] = true end
  local socket_count = 0
  for _ in pairs(sockets) do socket_count = socket_count + 1 end
  local caches = {}
  for _, entry in ipairs(fs.list(path("/sys/devices/system/cpu/cpu0/cache"), { follow = true }) or {}) do
    local base = "/sys/devices/system/cpu/cpu0/cache/" .. entry.name
    local level, kind = number_at(base .. "/level"), text_at(base .. "/type")
    local size = text_at(base .. "/size")
    if level and size then
      local bytes = tonumber(size:match("%d+")) * (size:match("K") and 1024 or size:match("M") and 1024 * 1024 or 1)
      local key = "L" .. level .. (kind == "Data" and "d" or kind == "Instruction" and "i" or "")
      caches[key] = bytes
    end
  end
  local cpufreq = "/sys/devices/system/cpu/cpu0/cpufreq"
  cpu_facts = {
    model = read_model(),
    sockets = math.max(1, socket_count),
    logical = select(2, info:gsub("\nprocessor%s*:", "")) + (info:match("^processor%s*:") and 1 or 0),
    base_mhz = (number_at(cpufreq .. "/base_frequency") or number_at(cpufreq .. "/cpuinfo_max_freq") or 0) / 1000,
    max_mhz = (number_at(cpufreq .. "/cpuinfo_max_freq") or 0) / 1000,
    driver = text_at(cpufreq .. "/scaling_driver") or "",
    governor = text_at(cpufreq .. "/scaling_governor") or "",
    preference = text_at(cpufreq .. "/energy_performance_preference") or "",
    virtualization = flags:match("%f[%w]vmx%f[%W]") and "Intel VT-x" or flags:match("%f[%w]svm%f[%W]") and "AMD-V" or "",
    caches = caches,
  }
  return cpu_facts
end

-- ---------------------------------------------------------------------------
-- Sources

local EMPTY = {
  cpu = { usage = 0, cores = {}, count = 0, frequency = 0, frequencies = {}, load = { 0, 0, 0 }, model = "" },
  memory = { total = 0, available = 0, used = 0, free = 0, cached = 0, percent = 0,
    swap = { total = 0, free = 0, used = 0, percent = 0 } },
  disks = {},
  drives = { drives = {} },
  fans = { fans = {} },
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
  drives = { sample_drives, 2000 },
  fans = { sample_fans, 2000 },
  temperatures = { sample_temperatures, 5000 },
  gpu = { sample_gpu, 2000 },
  network = { sample_network, 2000 },
  battery = { sample_battery, 3000 },
  backlight = { sample_backlight, 5000 },
  system = { sample_system, 60000 },
}

local sources = {}
local watch_backlights -- below: the backlights' own change notices

-- Sampled once for every screen, with the history each sample adds replayed
-- on the others: the machine is the same machine whichever screen asks.
-- Not the backlight: its value carries a setter, which cannot cross.
local SHARED = { cpu = true, memory = true, drives = true, fans = true, gpu = true, network = true,
  battery = true, temperatures = true, system = true, disks = true }

for name, entry in pairs(SAMPLERS) do
  local sample = entry[1]
  sources[name] = poll.source {
    name = "sysinfo." .. name,
    interval = entry[2],
    initial = EMPTY[name],
    shared = SHARED[name],
    sample = function(done)
      sample_async(sample, done)
    end,
    mirrored = function(_, pushes)
      for _, push in ipairs(pushes or {}) do ring(push[1]).push(push[2]) end
    end,
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

-- A backlight says when it changes: the kernel notifies whoever polls its
-- `actual_brightness` (morf.fs.watch does, for a file under /sys), so a
-- change from a key or another program shows at once rather than on the
-- next sample. The sample on its timer stays, for a driver that does not.
local backlight_watches = {}
watch_backlights = function(light)
  if not (fs.watch and light) then return end
  for _, device in ipairs(light.devices or {}) do
    if backlight_watches[device.name] == nil then
      local ok, handle = pcall(fs.watch, path("/sys/class/backlight/" .. device.name .. "/actual_brightness"),
        function() sources.backlight:refresh() end)
      backlight_watches[device.name] = ok and handle or false
    end
  end
end

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
--- Physical drives: `{ drives = { {name, model, vendor, kind, capacity,
--- removable, system, busy (percent), read_rate, write_rate (bytes/s),
--- read_total, write_total, units = { {name, label, depth, size, mounts,
--- swap, read_rate, write_rate} } } } }` -- units are the partitions and the
--- volumes on them, depth first.
function sysinfo.drives() return sources.drives:get() end
--- `{ fans = { {chip, label, rpm, max, key} } }`; `key` names its history.
function sysinfo.fans() return sources.fans:get() end
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
function sysinfo.backlight()
  local light = sources.backlight:get()
  watch_backlights(light)
  return light
end
--- `{ os, os_id, os_version, os_release, kernel, hostname, user, uptime (s) }`.
function sysinfo.system() return sources.system:get() end
--- `{ count, by_cpu = { {pid, name, state, cpu, memory, memory_percent} }, by_memory }`.
function sysinfo.processes() return sources.processes:get() end

--- The last `history_size` samples of one series, oldest first: `cpu`,
--- `load`, `memory`, `swap`, `temperature`, `gpu`, `rx`, `tx`, `core0`...
--- Reading it follows (and wakes) the section that feeds it.
local FEEDS = { cpu = "cpu", load = "cpu", memory = "memory", swap = "memory",
  temperature = "temperatures", gpu = "gpu", rx = "network", tx = "network" }
--- Per device too: `gpu:card1`, `disk:nvme0n1:busy` (`:read`, `:write`),
--- `rx:wlan0`, `tx:wlan0`, `fan0`.
local PREFIXED = { { "^core%d+$", "cpu" }, { "^gpu:", "gpu" }, { "^gpumem:", "gpu" },
  { "^gpuenc:", "gpu" }, { "^gpudec:", "gpu" }, { "^disk:", "drives" },
  { "^rx:", "network" }, { "^tx:", "network" }, { "^fan%d+$", "fans" }, { "^bat:", "battery" } }
function sysinfo.history(name)
  local feed = FEEDS[name]
  for _, rule in ipairs(PREFIXED) do
    if feed then break end
    if name:match(rule[1]) then feed = rule[2] end
  end
  if not feed then error("sysinfo.history: no series " .. tostring(name), 2) end
  sources[feed]:get()
  return ring(name).list()
end

--- The same series as a data channel (`morf.channel`), for a `ui.Path`'s
--- `series` to draw. Reading it wakes the section that feeds it, as
--- `history` does; a chart that keeps reading only the channel lets the
--- section sleep, so read `history` (or this) where the chart is shown.
function sysinfo.channel(name)
  sysinfo.history(name)
  return rings[name] and rings[name].channel or ring(name).channel
end

--- In-memory handoff for a soft UI reload; each series remains bounded.
function sysinfo.snapshot_history()
  local history = {}
  for name, values in pairs(rings) do history[name] = values.list() end
  return {history=history,stat=last_stat,rc6=last_rc6,net=last_net,net_at=last_net_at,
    proc=last_proc,proc_total=last_proc_total,io=last_io,io_at=last_io_at}
end
function sysinfo.restore_history(saved)
  if not saved then return end
  rings = {}
  for name, values in pairs(saved.history or {}) do
    for _, value in ipairs(values) do ring(name).push(value) end
  end
  last_stat,last_rc6,last_net,last_net_at=saved.stat or {},saved.rc6 or {},saved.net or {},saved.net_at
  last_proc,last_proc_total=saved.proc or {},saved.proc_total
  last_io,last_io_at=saved.io or {},saved.io_at
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
