-- The dashboard's Performance tab, laid out as Mission Center lays out its
-- own: the machine's devices down the left -- the processor, memory, every
-- drive, network interface, GPU and fan, each a small graph of its last
-- minutes and its reading -- and the one picked on the right: its name,
-- large graphs of its recent history, its readings and what it is.
--
-- A drive's page lists its logical units as well: the partitions, and the
-- volumes (LVM, LUKS) built on them, each with its mounts, size and traffic.
-- A GPU that is powered down is shown as that and not read: reading it would
-- wake it.

local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local sysinfo = require("lib.sysinfo")

local C = theme.color
local M = {}

M.WIDTH, M.HEIGHT = 1400, 760
local GAP = 12
local SIDE_W = 300                        -- the devices
local MAIN_W = M.WIDTH - SIDE_W - GAP     -- the device picked
local PAD = 28
local STATS_W = 300
local GRAPH_W = MAIN_W - 3 * PAD - STATS_W
local TOP = 84                            -- under the title
local ROW_H, SPARK_W, SPARK_H = 72, 86, 52

-- Each kind of device its hue, from the desk's own terminal colours, as
-- Mission Center gives each its own.
local HUE = { cpu = "color4", memory = "color6", drive = "color2", net = "color5", gpu = "color1", fan = "color3" }

--- The processor's name without its trademarks and generation: "Intel
--- Core i7-11800H @ 2.30GHz", as the reference prints it.
function M.cpu_name(model)
  model = tostring(model or "")
  model = model:gsub("%(R%)", ""):gsub("%(TM%)", ""):gsub("%(tm%)", "")
  model = model:gsub("^%s*%d+%a%a Gen ", ""):gsub(" CPU ", " "):gsub("%s+Processor", "")
  model = model:gsub("%s+", " "):match("^%s*(.-)%s*$")
  if model == "" then return "Unknown processor" end
  return model
end

--- The physical disk a partition is on: nvme0n1p2 -> nvme0n1, sda3 -> sda.
function M.disk_of(device)
  local name = tostring(device or ""):match("([^/]+)$") or ""
  local base = name:match("^(nvme%d+n%d+)p%d+$") or name:match("^(mmcblk%d+)p%d+$")
  if base then return base end
  if name:match("^[shv]d%a+%d+$") then return (name:gsub("%d+$", "")) end
  return name
end

--- The machine's disks, each a physical device with its filesystems' space
--- summed (a device mounted twice counted once), largest first.
function M.disks(list)
  local by, order = {}, {}
  for _, d in ipairs(list or {}) do
    local name = M.disk_of(d.device)
    if name ~= "" and (d.total or 0) > 0 then
      local disk = by[name]
      if not disk then
        disk = { name = name, total = 0, used = 0, seen = {} }
        by[name] = disk
        order[#order + 1] = disk
      end
      if not disk.seen[d.device] then
        disk.seen[d.device] = true
        disk.total = disk.total + (d.total or 0)
        disk.used = disk.used + (d.used or 0)
      end
    end
  end
  table.sort(order, function(a, b) return a.total > b.total end)
  for _, disk in ipairs(order) do
    disk.seen = nil
    disk.percent = disk.total > 0 and 100 * disk.used / disk.total or 0
  end
  return order
end

--- A rate as Mission Center prints it: "83 KiB/s".
local function rate_text(bytes)
  local text, unit = kit.bytes(bytes or 0)
  return text .. " " .. unit .. "/s"
end

local function size_text(bytes)
  local text, unit = kit.bytes(bytes or 0)
  return text .. " " .. unit
end

local function ghz(mhz)
  if not mhz or mhz <= 0 then return "--" end
  if mhz >= 1000 then return ("%.2f GHz"):format(mhz / 1000) end
  return ("%d MHz"):format(math.floor(mhz + 0.5))
end

local function duration_text(seconds)
  seconds = math.floor(seconds or 0)
  local d, h = seconds // 86400, (seconds % 86400) // 3600
  local m, s = (seconds % 3600) // 60, seconds % 60
  return ("%d:%02d:%02d:%02d"):format(d, h, m, s)
end

function M.build(ctx)
  local opened = ctx.opened
  local shown = function() return opened() end

  local function read(section, fallback)
    return function()
      if not shown() then return fallback end
      local ok, v = pcall(sysinfo[section])
      return (ok and v) or fallback
    end
  end
  local cpu = read("cpu", { usage = 0, model = "", cores = {}, count = 0, frequency = 0 })
  local memory = read("memory", { total = 0, used = 0, percent = 0, available = 0, cached = 0,
    committed = 0, swap = { total = 0, used = 0, free = 0 } })
  local drives = read("drives", { drives = {} })
  local network = read("network", { interfaces = {} })
  local gpus = read("gpu", { cards = {} })
  local fans = read("fans", { fans = {} })
  local temps = read("temperatures", {})
  local system = read("system", { uptime = 0 })

  local function hue(kind)
    return function() return theme.lule[HUE[kind]] end
  end
  local function history(name)
    if not shown() then return {} end
    local ok, list = pcall(sysinfo.history, name)
    return ok and list or {}
  end

  -- ------------------------------------------------------------ devices --
  -- Each a row: `key` ("cpu", "drive:nvme0n1", ...), its kind and what it
  -- names. The readings are bindings on the samples, so a row is only
  -- rebuilt when a device comes or goes.
  local list = morf.state { devices = {} }
  local selected = morf.signal("caelestia.performance.device", "cpu")

  local function find(items, field, value)
    for _, item in ipairs(items or {}) do if item[field] == value then return item end end
    return nil
  end
  local function drive(name) return find(drives().drives, "name", name) or { units = {} } end
  local function iface(name) return find(network().interfaces, "name", name) or {} end
  local function card(name) return find(gpus().cards, "name", name) or {} end
  local function fan(key) return find(fans().fans, "key", key) or {} end

  morf.effect("caelestia.performance.devices", function()
    if not shown() then return end
    local rows = { { key = "cpu", kind = "cpu", ref = "" }, { key = "memory", kind = "memory", ref = "" } }
    for _, d in ipairs(drives().drives) do rows[#rows + 1] = { key = "drive:" .. d.name, kind = "drive", ref = d.name } end
    for _, i in ipairs(network().interfaces) do
      if not i.virtual then rows[#rows + 1] = { key = "net:" .. i.name, kind = "net", ref = i.name } end
    end
    for index, g in ipairs(gpus().cards) do
      rows[#rows + 1] = { key = "gpu:" .. g.name, kind = "gpu", ref = g.name, index = index - 1 }
    end
    for index, f in ipairs(fans().fans) do
      rows[#rows + 1] = { key = "fan:" .. f.key, kind = "fan", ref = f.key, index = index - 1 }
    end
    list.devices:replace(rows, "key")
  end)

  -- What the picked device is: its kind and name ("drive", "nvme0n1").
  local function picked()
    local key = selected:get()
    local kind, ref = key:match("^(%a+):(.*)$")
    return kind or key, ref or ""
  end
  local function on(kind) return function() return (picked()) == kind end end

  -- -------------------------------------------------------------- graph --
  -- Mission Center's graph: a bordered box on a faint grid, the first
  -- series filled under its line, the second a dashed line; the newest
  -- sample at the right edge.
  local function series_path(values, w, h, top, closed)
    local n = sysinfo.history_size
    local step = w / math.max(1, n - 1)
    local offset = n - #values
    if #values < 2 then return "M0 0" end
    local parts = {}
    for i, v in ipairs(values) do
      local x = (offset + i - 1) * step
      local y = h - 1 - math.max(0, math.min(1, (v or 0) / top)) * (h - 2)
      parts[#parts + 1] = ("%s%.1f %.1f"):format(i == 1 and "M" or "L", x, y)
    end
    if closed then
      parts[#parts + 1] = ("L%.1f %.1f L%.1f %.1f Z"):format(w, h, offset * step, h)
    end
    return table.concat(parts, " ")
  end

  local function grid_path(w, h, columns, rows)
    local parts = {}
    for i = 1, columns - 1 do parts[#parts + 1] = ("M%.1f 0 V%.1f"):format(w * i / columns, h) end
    for i = 1, rows - 1 do parts[#parts + 1] = ("M0 %.1f H%.1f"):format(h * i / rows, w) end
    return table.concat(parts, " ")
  end

  --- `spec`: width, height, kind, first (fn -> list), second (fn -> list,
  --- dashed), top (number or fn -> the value at the top; the peak when
  --- nil), grid (false for none), id.
  local function graph(spec)
    local w, h = spec.width, spec.height
    local color = hue(spec.kind)
    local function top()
      if type(spec.top) == "function" then return math.max(1e-9, spec.top()) end
      if spec.top then return spec.top end
      local peak = 0
      for _, v in ipairs(spec.first()) do if v > peak then peak = v end end
      if spec.second then for _, v in ipairs(spec.second()) do if v > peak then peak = v end end end
      return math.max(peak * 1.15, 1024)
    end
    local children = {
      id = spec.id,
      width = w, height = h, radius = 3,
      clip = true,
      color = function() return color():alpha(0.06) end,
      border_width = 1,
      border_color = function() return color():alpha(0.85) end,
    }
    if spec.grid ~= false then
      children[#children + 1] = ui.Path {
        width = w, height = h, view_box = { 0, 0, w, h },
        d = grid_path(w, h, spec.columns or 12, spec.rows or 6),
        stroke_color = function() return color():alpha(0.14) end, fill_color = function() return color():alpha(0) end, stroke_width = 1,
      }
    end
    children[#children + 1] = ui.Path {
      width = w, height = h, view_box = { 0, 0, w, h },
      d = function() return series_path(spec.first(), w, h, top(), true) end,
      fill_color = function() return color():alpha(0.28) end,
    }
    children[#children + 1] = ui.Path {
      width = w, height = h, view_box = { 0, 0, w, h },
      d = function() return series_path(spec.first(), w, h, top(), false) end,
      stroke_color = color, fill_color = function() return color():alpha(0) end, stroke_width = 1.5, stroke_join = "round",
    }
    if spec.second then
      children[#children + 1] = ui.Path {
        width = w, height = h, view_box = { 0, 0, w, h },
        d = function() return series_path(spec.second(), w, h, top(), false) end,
        stroke_color = color, fill_color = function() return color():alpha(0) end, stroke_width = 1.5, stroke_join = "round", dash = { 5, 4 },
      }
    end
    return ui.Rect(children), top
  end

  --- A graph with its caption over it, left, and its scale, right.
  local function captioned(caption, scale, spec)
    local box, top = graph(spec)
    return ui.Column {
      gap = 4,
      ui.Item {
        width = spec.width, height = 18,
        kit.text { text = caption, font_size = theme.size.small, color = function() return C.onSurfaceVariant end },
        kit.text {
          anchors = { right = true }, font_size = theme.size.small,
          text = function() return scale(top()) end,
          color = function() return C.onSurfaceVariant end,
        },
      },
      box,
    }
  end
  local function percent_scale() return "100%" end
  local function rate_scale(top) return rate_text(top) end
  local minutes = ("over %d minutes"):format(math.floor(sysinfo.history_size * 2 / 60 + 0.5))

  -- -------------------------------------------------------------- stats --
  -- The readings: a small label over a large value, two to a row; a
  -- coloured bar beside those a graph draws (solid, or dotted for the
  -- dashed series).
  local function stat(label, value, mark, kind)
    local column = ui.Column {
      gap = 0, width = STATS_W / 2 - 6,
      kit.text { text = label, font_size = theme.size.small, color = function() return C.onSurfaceVariant end },
      kit.text {
        text = value, font_size = theme.size.large, font_weight = 600,
        width = STATS_W / 2 - 14, elide = "right",
      },
    }
    if not mark then return column end
    return ui.Row {
      gap = 6,
      ui.Rect {
        width = 2, height = 40, radius = 1,
        color = function() return hue(kind)():alpha(mark == "dashed" and 0.5 or 1) end,
      },
      column,
    }
  end
  local function stats(items)
    return ui.Grid { columns = 2, column_gap = 12, row_gap = 18, table.unpack(items) }
  end
  --- What the device is: label, value lines.
  local function facts(rows)
    local column = { gap = 9 }
    for _, row in ipairs(rows) do
      column[#column + 1] = ui.Row {
        gap = 8,
        kit.text {
          text = row[1], width = 136, font_size = theme.size.small,
          color = function() return C.onSurfaceVariant end,
        },
        kit.text { text = row[2], width = STATS_W - 144, elide = "right", font_size = theme.size.small },
      }
    end
    return ui.Column(column)
  end

  local function title(text_fn, model_fn)
    return ui.Item {
      x = PAD, y = 22, width = MAIN_W - 2 * PAD, height = 44,
      kit.text {
        anchors = { vertical_center = true },
        text = text_fn, font_size = theme.size.extra - 2, font_weight = 700,
      },
      kit.text {
        anchors = { right = true, vertical_center = true }, x = 0,
        width = MAIN_W - 2 * PAD - 260, horizontal_alignment = "right", elide = "right",
        text = model_fn, font_size = theme.size.large, font_weight = 600,
      },
    }
  end

  local function page(kind, contents)
    contents.visible = on(kind)
    contents.width, contents.height = MAIN_W, M.HEIGHT
    return ui.Item(contents)
  end
  local stats_x = PAD + GRAPH_W + PAD
  local area_h = M.HEIGHT - TOP - PAD

  -- ---------------------------------------------------------------- cpu --
  local info = sysinfo.cpu_info()
  local threads = math.max(1, info.logical or 1)
  local cols = threads <= 4 and threads or threads <= 16 and 4 or threads <= 36 and 6 or 8
  local rows_n = math.ceil(threads / cols)
  local cell_gap = 10
  local cell_w = (GRAPH_W - (cols - 1) * cell_gap) / cols
  local cell_h = (area_h - 22 - (rows_n - 1) * cell_gap) / rows_n
  local cells = { columns = cols, column_gap = cell_gap, row_gap = cell_gap }
  for i = 0, threads - 1 do
    cells[#cells + 1] = (graph {
      id = "performance-core-" .. i, kind = "cpu",
      width = cell_w, height = cell_h, top = 100, columns = 6, rows = 4,
      first = function() return history("core" .. i) end,
    })
  end
  local cpu_page = page("cpu", {
    title(function() return "CPU" end, function() return M.cpu_name(info.model) end),
    ui.Column {
      x = PAD, y = TOP, gap = 4,
      ui.Item {
        width = GRAPH_W, height = 18,
        kit.text { text = "Utilization " .. minutes, font_size = theme.size.small, color = function() return C.onSurfaceVariant end },
        kit.text { anchors = { right = true }, text = "100%", font_size = theme.size.small, color = function() return C.onSurfaceVariant end },
      },
      ui.Grid(cells),
    },
    ui.Column {
      x = stats_x, y = TOP, gap = 28,
      stats {
        stat("Utilization", function() return ("%d%%"):format(math.floor((cpu().usage or 0) + 0.5)) end),
        stat("Speed", function() return ghz(cpu().frequency) end),
        stat("Processes", function() cpu() return M.process_count() end),
        stat("Threads", function() return tostring(cpu().threads or "--") end),
        stat("Temperature", function()
          local t = temps().cpu
          return t and ("%d °C"):format(math.floor(t + 0.5)) or "--"
        end),
        stat("Up time", function() return duration_text(system().uptime) end),
      },
      facts {
        { "Base speed:", ghz(info.base_mhz) },
        { "Max speed:", ghz(info.max_mhz) },
        { "Sockets:", tostring(info.sockets) },
        { "Virtual processors:", tostring(info.logical) },
        { "Virtualization:", info.virtualization ~= "" and info.virtualization or "No" },
        { "L1 cache:", size_text((info.caches.L1d or 0) + (info.caches.L1i or 0)) },
        { "L2 cache:", size_text(info.caches.L2 or 0) },
        { "L3 cache:", size_text(info.caches.L3 or 0) },
        { "Cpufreq driver:", info.driver },
        { "Cpufreq governor:", info.governor },
        { "Power preference:", info.preference },
      },
    },
  })

  -- ------------------------------------------------------------- memory --
  local mem_h = math.floor((area_h - 3 * 22 - 40 - 2 * 10) * 0.64)
  local swap_h = area_h - 3 * 22 - 40 - 2 * 10 - mem_h
  local function composition()
    local m = memory()
    local total = math.max(1, m.total or 1)
    local used = (m.used or 0) / total
    local cached = math.min(1 - used, (m.cached or 0) / total)
    return used, cached
  end
  local memory_page = page("memory", {
    title(function() return "Memory" end, function() return size_text(memory().total) end),
    ui.Column {
      x = PAD, y = TOP, gap = 10,
      captioned("Memory usage " .. minutes, function() return size_text(memory().total) end, {
        id = "performance-memory-graph", kind = "memory", width = GRAPH_W, height = mem_h, top = 100,
        first = function() return history("memory") end,
      }),
      captioned("Swap usage " .. minutes, function() return size_text(memory().swap.total) end, {
        id = "performance-swap-graph", kind = "memory", width = GRAPH_W, height = swap_h, top = 100,
        first = function() return history("swap") end,
      }),
      ui.Column {
        gap = 4,
        kit.text { text = "Memory composition", font_size = theme.size.small, color = function() return C.onSurfaceVariant end },
        ui.Rect {
          id = "performance-memory-composition",
          width = GRAPH_W, height = 40, radius = 3, clip = true,
          color = function() return hue("memory")():alpha(0.04) end,
          border_width = 1, border_color = function() return hue("memory")():alpha(0.85) end,
          ui.Rect {
            height = 40,
            width = function() return GRAPH_W * (composition()) end,
            color = function() return hue("memory")():alpha(0.35) end,
          },
          ui.Rect {
            height = 40,
            x = function() return GRAPH_W * (composition()) end,
            width = function() local _, c = composition() return GRAPH_W * c end,
            color = function() return hue("memory")():alpha(0.14) end,
          },
        },
      },
    },
    ui.Column {
      x = stats_x, y = TOP, gap = 28,
      stats {
        stat("In use", function() return size_text(memory().used) end, "solid", "memory"),
        stat("Available", function() return size_text(memory().available) end),
        stat("Committed", function() return size_text(memory().committed) end),
        stat("Cached", function() return size_text(memory().cached) end),
        stat("Swap used", function() return size_text(memory().swap.used) end),
        stat("Swap available", function() return size_text(memory().swap.free) end),
      },
      facts {
        { "Total:", function() return size_text(memory().total) end },
        { "Shared:", function() return size_text(memory().shared) end },
        { "Buffers:", function() return size_text(memory().buffers) end },
        { "Dirty:", function() return size_text(memory().dirty) end },
        { "Commit limit:", function() return size_text(memory().commit_limit) end },
      },
    },
  })

  -- -------------------------------------------------------------- drive --
  local function drive_ref() local _, ref = picked() return ref end
  local function the_drive() return drive(drive_ref()) end
  local units = morf.state { rows = {} }
  morf.effect("caelestia.performance.units", function()
    if not shown() or (picked()) ~= "drive" then return end
    local rows = {}
    for _, u in ipairs(the_drive().units or {}) do
      rows[#rows + 1] = { name = u.name }
    end
    units.rows:replace(rows, "name")
  end)
  local function unit(name) return find(the_drive().units, "name", name) or { mounts = {} } end
  local drive_graph_h = 200
  local units_y = TOP + 2 * (drive_graph_h + 22 + 10)
  local drive_page = page("drive", {
    title(function()
      local d = the_drive()
      return ("%s (%s)"):format(d.kind or "Drive", d.name or "")
    end, function() return the_drive().model or "" end),
    ui.Column {
      x = PAD, y = TOP, gap = 10,
      captioned("Active time " .. minutes, percent_scale, {
        id = "performance-drive-active", kind = "drive", width = GRAPH_W, height = drive_graph_h, top = 100,
        first = function() return history("disk:" .. drive_ref() .. ":busy") end,
      }),
      captioned("Throughput " .. minutes, rate_scale, {
        id = "performance-drive-throughput", kind = "drive", width = GRAPH_W, height = drive_graph_h,
        first = function() return history("disk:" .. drive_ref() .. ":read") end,
        second = function() return history("disk:" .. drive_ref() .. ":write") end,
      }),
    },
    -- Its logical units: the partitions and what is built on them.
    ui.Item {
      x = PAD, y = units_y, width = GRAPH_W, height = M.HEIGHT - units_y - PAD,
      kit.text { text = "Logical units", font_size = theme.size.small, color = function() return C.onSurfaceVariant end },
      ui.Flickable {
        y = 22, width = GRAPH_W, height = M.HEIGHT - units_y - PAD - 22, clip = true,
        ui.Repeater {
            as = "column", gap = 4, width = GRAPH_W,
            model = units.rows,
            delegate = function(row)
              local function u() return unit(row.name) end
              return ui.Rect {
                width = GRAPH_W, height = 36, radius = 8,
                color = function() return C.surfaceContainerHigh end,
                kit.text {
                  anchors = { vertical_center = true },
                  x = function() return 10 + 16 * math.max(0, (u().depth or 1) - 1) end,
                  width = 140, elide = "right", font_weight = 500, font_size = theme.size.small,
                  text = function() return u().label or row.name end,
                },
                kit.text {
                  anchors = { vertical_center = true }, x = 170, width = GRAPH_W - 170 - 250, elide = "right",
                  font_size = theme.size.small, color = function() return C.onSurfaceVariant end,
                  text = function()
                    local x = u()
                    if x.swap then return "swap" end
                    return #x.mounts > 0 and table.concat(x.mounts, "  ") or "not mounted"
                  end,
                },
                kit.text {
                  anchors = { vertical_center = true, right = true, right_margin = 10 },
                  font_size = theme.size.small, horizontal_alignment = "right", width = 236,
                  text = function()
                    local x = u()
                    return ("%s   ↓%s ↑%s"):format(size_text(x.size), rate_text(x.read_rate), rate_text(x.write_rate))
                  end,
                },
              }
            end,
        },
      },
    },
    ui.Column {
      x = stats_x, y = TOP, gap = 28,
      stats {
        stat("Read speed", function() return rate_text(the_drive().read_rate) end, "solid", "drive"),
        stat("Write speed", function() return rate_text(the_drive().write_rate) end, "dashed", "drive"),
        stat("Total read", function() return size_text(the_drive().read_total) end),
        stat("Total written", function() return size_text(the_drive().write_total) end),
        stat("Active time", function() return ("%d%%"):format(math.floor((the_drive().busy or 0) + 0.5)) end),
      },
      facts {
        { "Capacity:", function() return size_text(the_drive().capacity) end },
        { "System disk:", function() return the_drive().system and "Yes" or "No" end },
        { "Type:", function() return the_drive().kind or "" end },
        { "Removable:", function() return the_drive().removable and "Yes" or "No" end },
        { "Logical units:", function() return tostring(#(the_drive().units or {})) end },
      },
    },
  })

  -- ------------------------------------------------------------ network --
  local function net_ref() local _, ref = picked() return ref end
  local function the_iface() return iface(net_ref()) end
  -- Addresses and the network's name, asked once per interface picked.
  local addresses = morf.signal("caelestia.performance.addresses", { v4 = "", v6 = "", ssid = "" })
  local asked = ""
  morf.effect("caelestia.performance.addresses", function()
    if not shown() or (picked()) ~= "net" then asked = "" return end
    local name = net_ref()
    if name == asked then return end
    asked = name
    addresses:set({ v4 = "", v6 = "", ssid = "" })
    morf.run({ "ip", "-j", "addr", "show", "dev", name }, {}, function(result)
      if not (result and result.ok) then return end
      local ok, data = pcall(morf.json.decode, result.stdout or "")
      if not ok or type(data) ~= "table" or not data[1] then return end
      local v4, v6 = {}, {}
      for _, a in ipairs(data[1].addr_info or {}) do
        if a.family == "inet" then v4[#v4 + 1] = a["local"] end
        if a.family == "inet6" and a.scope == "global" then v6[#v6 + 1] = a["local"] end
      end
      local now = addresses:get()
      addresses:set({ v4 = table.concat(v4, ", "), v6 = table.concat(v6, ", "), ssid = now.ssid })
    end)
    if the_iface().wireless then
      morf.run({ "nmcli", "-t", "-g", "GENERAL.CONNECTION", "device", "show", name }, {}, function(result)
        if not (result and result.ok) then return end
        local now = addresses:get()
        addresses:set({ v4 = now.v4, v6 = now.v6, ssid = (tostring(result.stdout or ""):gsub("%s+$", "")) })
      end)
    end
  end)
  local function totals()
    local i = the_iface()
    return i.rx_bytes or 0, i.tx_bytes or 0
  end
  local net_page = page("net", {
    title(function() return the_iface().wireless and "Wi-Fi" or "Ethernet" end,
      function() return net_ref() end),
    ui.Column {
      x = PAD, y = TOP,
      captioned("Throughput " .. minutes, rate_scale, {
        id = "performance-net-throughput", kind = "net", width = GRAPH_W, height = area_h - 22,
        first = function() return history("rx:" .. net_ref()) end,
        second = function() return history("tx:" .. net_ref()) end,
      }),
    },
    ui.Column {
      x = stats_x, y = TOP, gap = 28,
      stats {
        stat("Receive", function() return rate_text(the_iface().rx_rate) end, "solid", "net"),
        stat("Send", function() return rate_text(the_iface().tx_rate) end, "dashed", "net"),
        stat("Total received", function() return size_text((totals())) end),
        stat("Total sent", function() local _, tx = totals() return size_text(tx) end),
      },
      facts {
        { "Interface name:", net_ref },
        { "Connection type:", function() return the_iface().wireless and "Wireless" or "Wired" end },
        { "Network:", function() local a = addresses:get() return a.ssid ~= "" and a.ssid or "--" end },
        { "Link speed:", function() local sp = the_iface().speed return sp and (sp .. " Mb/s") or "--" end },
        { "State:", function() return the_iface().state or "" end },
        { "Hardware address:", function() return the_iface().address or "" end },
        { "IPv4 address:", function() local a = addresses:get() return a.v4 ~= "" and a.v4 or "N/A" end },
        { "IPv6 address:", function() local a = addresses:get() return a.v6 ~= "" and a.v6 or "N/A" end },
      },
    },
  })

  -- ---------------------------------------------------------------- gpu --
  local function gpu_ref() local _, ref = picked() return ref end
  local function the_card() return card(gpu_ref()) end
  local function gpu_index()
    for index, g in ipairs(gpus().cards) do if g.name == gpu_ref() then return index - 1 end end
    return 0
  end
  local gpu_page = page("gpu", {
    title(function() return "GPU " .. gpu_index() end, function() return the_card().model or "" end),
    -- One graph for a GPU sharing the system's memory; Mission Center's
    -- three -- utilisation, video engines, its own memory -- for one with
    -- memory of its own.
    ui.Column {
      x = PAD, y = TOP,
      visible = function() local g = the_card() return not g.suspended and not g.vram_total end,
      captioned("Utilization " .. minutes, percent_scale, {
        id = "performance-gpu-graph", kind = "gpu", width = GRAPH_W, height = area_h - 22, top = 100,
        first = function() return history("gpu:" .. gpu_ref()) end,
      }),
    },
    ui.Column {
      x = PAD, y = TOP, gap = 14,
      visible = function() local g = the_card() return not g.suspended and g.vram_total ~= nil end,
      captioned("Utilization " .. minutes, percent_scale, {
        id = "performance-gpu-busy", kind = "gpu", width = GRAPH_W, height = area_h - 3 * 22 - 28 - 2 * 150, top = 100,
        first = function() return history("gpu:" .. gpu_ref()) end,
      }),
      captioned("Video encode/decode " .. minutes, percent_scale, {
        id = "performance-gpu-video", kind = "gpu", width = GRAPH_W, height = 150, top = 100,
        first = function() return history("gpuenc:" .. gpu_ref()) end,
        second = function() return history("gpudec:" .. gpu_ref()) end,
      }),
      captioned("Memory usage " .. minutes, function() return size_text(the_card().vram_total) end, {
        id = "performance-gpu-memory", kind = "gpu", width = GRAPH_W, height = 150, top = 100,
        first = function() return history("gpumem:" .. gpu_ref()) end,
      }),
    },
    ui.Rect {
      x = PAD, y = TOP + 22, width = GRAPH_W, height = area_h - 22, radius = 3,
      visible = function() return the_card().suspended == true end,
      color = function() return hue("gpu")():alpha(0.04) end,
      border_width = 1, border_color = function() return hue("gpu")():alpha(0.5) end,
      ui.Column {
        anchors = { center_in = true }, gap = 6, align = "center",
        kit.icon("power_settings_new", 40, function() return C.onSurfaceVariant end),
        kit.text { text = "Powered down", font_size = theme.size.large, font_weight = 600 },
        kit.text {
          text = "Not read while it sleeps: reading it would wake it.",
          font_size = theme.size.small, color = function() return C.onSurfaceVariant end,
        },
      },
    },
    ui.Column {
      x = stats_x, y = TOP, gap = 28,
      stats {
        stat("Utilization", function()
          local g = the_card()
          if g.suspended then return "Off" end
          return g.busy and ("%d%%"):format(math.floor(g.busy + 0.5)) or "--"
        end),
        stat("Clock speed", function()
          local g = the_card()
          if g.suspended or not g.clock_mhz then return "--" end
          return ghz(g.clock_mhz)
        end),
        stat("Memory usage", function()
          local g = the_card()
          if not g.vram_total then return "Shared" end
          return size_text(g.vram_used)
        end),
        stat("Temperature", function()
          local g = the_card()
          return g.temperature and ("%d °C"):format(g.temperature) or "--"
        end),
        stat("Video encode", function() local g = the_card() return g.encoder and ("%d%%"):format(g.encoder) or "--" end,
          "solid", "gpu"),
        stat("Video decode", function() local g = the_card() return g.decoder and ("%d%%"):format(g.decoder) or "--" end,
          "dashed", "gpu"),
        stat("Power draw", function()
          local g = the_card()
          if not g.power then return "--" end
          return g.power_limit and ("%d / %d W"):format(math.floor(g.power + 0.5), math.floor(g.power_limit + 0.5))
            or ("%d W"):format(math.floor(g.power + 0.5))
        end),
        stat("State", function() return the_card().suspended and "Suspended" or "Active" end),
      },
      facts {
        { "Vendor:", function() return the_card().vendor or "" end },
        { "Max clock:", function() return ghz(the_card().max_mhz) end },
        { "Memory speed:", function()
          local g = the_card()
          return g.memory_clock_mhz and ("%s / %s"):format(ghz(g.memory_clock_mhz), ghz(g.memory_max_mhz)) or "--"
        end },
        { "Driver version:", function() return the_card().driver_version or "--" end },
        { "Driver:", function() return the_card().driver or "" end },
        { "PCI bus address:", function() return the_card().slot or "" end },
        { "Card:", gpu_ref },
      },
    },
  })

  -- ---------------------------------------------------------------- fan --
  local function fan_ref() local _, ref = picked() return ref end
  local function the_fan() return fan(fan_ref()) end
  local fan_page = page("fan", {
    title(function() return "Fan " .. (fan_ref():gsub("^fan", "")) end, function() return the_fan().label or "" end),
    ui.Column {
      x = PAD, y = TOP,
      captioned("Speed " .. minutes, function(top) return ("%d RPM"):format(math.floor(top)) end, {
        id = "performance-fan-graph", kind = "fan", width = GRAPH_W, height = area_h - 22,
        top = function()
          local f = the_fan()
          if f.max and f.max > 0 then return f.max end
          local peak = 1000
          for _, v in ipairs(history(fan_ref())) do if v > peak then peak = v end end
          return peak * 1.15
        end,
        first = function() return history(fan_ref()) end,
      }),
    },
    ui.Column {
      x = stats_x, y = TOP, gap = 28,
      stats {
        stat("Speed", function() return ("%d RPM"):format(the_fan().rpm or 0) end, "solid", "fan"),
        stat("Temperature", function()
          local t = temps().cpu
          return t and ("%d °C"):format(math.floor(t + 0.5)) or "--"
        end),
      },
      facts {
        { "Label:", function() return the_fan().label or "" end },
        { "Sensor chip:", function() return the_fan().chip or "" end },
        { "Maximum:", function() local f = the_fan() return f.max and (f.max .. " RPM") or "--" end },
      },
    },
  })

  -- ------------------------------------------------------------ the list --
  local function row_title(row)
    if row.kind == "cpu" then return "CPU" end
    if row.kind == "memory" then return "Memory" end
    if row.kind == "drive" then return ("%s (%s)"):format(drive(row.ref).kind or "Drive", row.ref) end
    if row.kind == "net" then return ("%s (%s)"):format(iface(row.ref).wireless and "Wi-Fi" or "Ethernet", row.ref) end
    if row.kind == "gpu" then return ("GPU %d"):format(row.index or 0) end
    return ("Fan %d"):format(row.index or 0)
  end
  local function row_sub(row)
    if row.kind == "cpu" then return M.cpu_name(info.model) end
    if row.kind == "memory" then
      local m = memory()
      return ("%s / %s"):format(size_text(m.used), size_text(m.total))
    end
    if row.kind == "drive" then return drive(row.ref).model or "" end
    if row.kind == "net" then
      local i = iface(row.ref)
      return ("S: %s  R: %s"):format(rate_text(i.tx_rate), rate_text(i.rx_rate))
    end
    if row.kind == "gpu" then return card(row.ref).model or "" end
    return fan(row.ref).label or ""
  end
  local function row_value(row)
    if row.kind == "cpu" then
      local t = temps().cpu
      return ("%d%%%s"):format(math.floor((cpu().usage or 0) + 0.5), t and (" (%d °C)"):format(math.floor(t + 0.5)) or "")
    end
    if row.kind == "memory" then return ("%d%%"):format(math.floor((memory().percent or 0) + 0.5)) end
    if row.kind == "drive" then return ("%d%%"):format(math.floor((drive(row.ref).busy or 0) + 0.5)) end
    if row.kind == "net" then return iface(row.ref).state or "" end
    if row.kind == "gpu" then
      local g = card(row.ref)
      if g.suspended then return "Suspended" end
      return g.busy and ("%d%%"):format(math.floor(g.busy + 0.5)) or "Active"
    end
    return ("%d RPM"):format(fan(row.ref).rpm or 0)
  end
  local function row_series(row)
    if row.kind == "cpu" then return function() return history("cpu") end, nil, 100 end
    if row.kind == "memory" then return function() return history("memory") end, nil, 100 end
    if row.kind == "drive" then return function() return history("disk:" .. row.ref .. ":busy") end, nil, 100 end
    if row.kind == "net" then
      return function() return history("rx:" .. row.ref) end, function() return history("tx:" .. row.ref) end, nil
    end
    if row.kind == "gpu" then return function() return history("gpu:" .. row.ref) end, nil, 100 end
    return function() return history(row.ref) end, nil, function()
      local f = fan(row.ref)
      return (f.max and f.max > 0) and f.max or 8000
    end
  end

  local function device_row(row)
    local first, second, top = row_series(row)
    local spark = graph {
      kind = row.kind, width = SPARK_W, height = SPARK_H, grid = false, top = top,
      first = first, second = second,
    }
    local area
    area = ui.MouseArea {
      id = "performance-device-" .. row.key,
      width = SIDE_W - 20, height = ROW_H, cursor = "pointer",
      on_clicked = function() selected:set(row.key) end,
      ui.Rect {
        anchors = { fill = true }, radius = 10,
        color = function()
          if selected:get() == row.key then return C.surfaceContainerHighest end
          return (area and area.hovered) and C.onSurface:alpha(0.05) or C.onSurface:alpha(0)
        end,
        behavior = { color = { duration = theme.duration.small } },
      },
      ui.Item { x = 10, y = (ROW_H - SPARK_H) / 2, width = SPARK_W, height = SPARK_H, spark },
      ui.Column {
        x = SPARK_W + 24, anchors = { vertical_center = true }, gap = 1,
        kit.text { text = function() return row_title(row) end, font_weight = 500, width = SIDE_W - SPARK_W - 50, elide = "right" },
        kit.text {
          text = function() return row_sub(row) end, font_size = theme.size.small - 2,
          width = SIDE_W - SPARK_W - 50, elide = "right", color = function() return C.onSurfaceVariant end,
        },
        kit.text {
          text = function() return row_value(row) end, font_size = theme.size.small - 2,
          color = function() return C.onSurfaceVariant end,
        },
      },
    }
    return area
  end

  local side = kit.card {
    id = "performance-devices",
    width = SIDE_W, height = M.HEIGHT,
    kit.text {
      x = 0, y = 22, width = SIDE_W, horizontal_alignment = "center",
      text = "Devices", font_size = theme.size.larger, font_weight = 600,
    },
    ui.Flickable {
      x = 10, y = 62, width = SIDE_W - 20, height = M.HEIGHT - 72, clip = true,
      ui.Repeater { as = "column", gap = 6, width = SIDE_W - 20, model = list.devices, delegate = device_row },
    },
  }

  local main = kit.card {
    id = "performance-main",
    width = MAIN_W, height = M.HEIGHT,
    cpu_page, memory_page, drive_page, net_page, gpu_page, fan_page,
  }

  return ui.Row {
    id = "dashboard-performance",
    width = M.WIDTH, height = M.HEIGHT, gap = GAP,
    side, main,
  }
end

--- How many processes run: the numbered folders in /proc.
function M.process_count()
  local ok, entries = pcall(morf.fs.list, "/proc")
  if not ok or type(entries) ~= "table" then return "--" end
  local count = 0
  for _, e in ipairs(entries) do if e.name:match("^%d+$") then count = count + 1 end end
  return tostring(count)
end

-- Built as the module loads, with its own instruction budget.
M.page = M.build(require("dashboard_state").context(3))

return M
