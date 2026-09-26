-- The dashboard's Performance tab: the processor (its name, temperature and
-- usage) across the top; under it the storage ring with a disk picker, the
-- network's speeds and totals with a small chart of the download, and the
-- memory ring.
--
-- Measured off the reference at 1920x1080 (the page is the panel less its
-- padding and tabs): 955 x 384, the processor card 152 tall, the three below
-- 220 tall and 363, 390 and 177 wide, 12 apart.

local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local shapes = require("lib.m3shapes")
local sysinfo = require("lib.sysinfo")

local C = theme.color
local M = {}

M.WIDTH, M.HEIGHT = 955, 384
local GAP = 12
local TOP_H, ROW_H = 152, 220
local STORAGE_W, NET_W, MEM_W = 363, 390, 177

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
  local cpu = read("cpu", { usage = 0, model = "" })
  local temps = read("temperatures", {})
  local memory = read("memory", { total = 0, used = 0, percent = 0 })
  local network = read("network", { rx_rate = 0, tx_rate = 0 })
  local disk_list = read("disks", {})

  -- -------------------------------------------------------------- cpu --
  local cpu_card = kit.card {
    id = "performance-cpu",
    width = M.WIDTH, height = TOP_H,
    kit.gauge {
      id = "performance-cpu-ring",
      x = 14, y = 17, size = 52, stroke = 4, gap = 3,
      value = function() return (cpu().usage or 0) / 100 end,
      color = function() return C.primary end,
      track = function() return C.surfaceVariant end,
      kit.icon("memory", 20, function() return C.primary end, { anchors = { center_in = true } }),
    },
    kit.text {
      x = 83, y = 16, text = "CPU", font_size = theme.size.large + 1, font_weight = 500,
      color = function() return C.primary end,
    },
    kit.text {
      id = "performance-cpu-name",
      x = 83, y = 46, width = 560, elide = "right",
      text = function() return M.cpu_name(cpu().model) end,
      font_size = theme.size.normal + 1,
    },
    ui.Row {
      x = 17, y = 90, gap = 6, align = "center",
      kit.icon("thermostat", 22, function() return C.primary end, { fill = true }),
      kit.text {
        id = "performance-temperature",
        text = function()
          local t = temps().cpu
          return t and ("%d°C"):format(math.floor(t + 0.5)) or "--°C"
        end,
        font_size = theme.size.large, font_weight = 500,
      },
    },
    kit.bar {
      id = "performance-temperature-bar",
      x = 19, y = 124, width = 200, stroke = 7,
      value = function() return (temps().cpu or 0) / 100 end,
      color = function() return C.primary end,
      track = function() return C.surfaceVariant end,
    },
    kit.text {
      anchors = { right = true, right_margin = 63 - 30 }, y = 19, width = 60,
      horizontal_alignment = "center",
      text = "Usage", font_size = theme.size.normal + 1,
    },
    ui.Item {
      anchors = { right = true, right_margin = 13 }, y = 40, width = 100, height = 100,
      ui.Path {
        anchors = { fill = true }, view_box = { 0, 0, 100, 100 }, d = shapes.path("sunny", { segments = false }),
        fill_color = function() return C.surfaceContainerHighest end,
      },
      kit.text {
        id = "performance-usage",
        anchors = { center_in = true },
        text = function() return ("%d%%"):format(math.floor((cpu().usage or 0) + 0.5)) end,
        font_size = theme.size.extra - 2, font_weight = 500,
        color = function() return C.primary end,
      },
    },
  }

  -- ---------------------------------------------------------- storage --
  local chosen = morf.signal("caelestia.performance.disk", "")
  local menu_open = morf.signal("caelestia.performance.disk_menu", false)
  local function disks() return M.disks(disk_list()) end
  local function disk()
    local list = disks()
    local want = chosen:get()
    for _, d in ipairs(list) do if d.name == want then return d end end
    return list[1] or { name = "none", total = 0, used = 0, percent = 0 }
  end
  local ring_color = function() return C.primary end
  local storage_ring = kit.gauge {
    id = "performance-storage-ring",
    x = 27, y = 22, size = 130, stroke = 8, from = 215, sweep = 290, gap = 4,
    value = function() return disk().percent / 100 end,
    color = ring_color,
    track = function() return C.surfaceVariant end,
    ui.Column {
      anchors = { center_in = true }, gap = 0, align = "center",
      kit.icon("hard_drive", 20, function() return C.onSurface end),
      kit.text {
        id = "performance-storage-percent",
        text = function() return ("%d%%"):format(math.floor(disk().percent + 0.5)) end,
        font_size = theme.size.extra - 3, font_weight = 500,
      },
      kit.text { text = "Used", font_size = theme.size.normal + 1 },
    },
  }
  local function picker_button(props, radius)
    local area = ctx.area(props)
    return kit.hover(area, function(hovered)
      return hovered and C.secondaryContainer:mix(C.onSecondaryContainer, 0.08) or C.secondaryContainer
    end, radius)
  end
  local menu_rows = {}
  local MAX_DISKS = 6
  for i = 1, MAX_DISKS do
    local function row() return disks()[i] end
    menu_rows[#menu_rows + 1] = kit.hover(ctx.area {
      id = "performance-disk-" .. i,
      width = 184, height = 36, cursor = "pointer",
      visible = function() return row() ~= nil end,
      on_clicked = function()
        local r = row()
        if r then chosen:set(r.name) end
        menu_open:set(false)
      end,
      kit.text {
        x = 16, anchors = { vertical_center = true },
        text = function() local r = row() return r and r.name or "" end,
        color = function()
          local r = row()
          return (r and r.name == disk().name) and C.primary or C.onSurface
        end,
      },
    }, function(hovered) return hovered and C.onSurface:alpha(0.08) or C.onSurface:alpha(0) end, 10)
  end
  local menu = ui.Rect {
    id = "performance-disk-menu",
    x = 68, z = 10, width = 184, radius = 12,
    height = function() return math.min(#disks(), MAX_DISKS) * 36 + 8 end,
    y = function() return 150 - (math.min(#disks(), MAX_DISKS) * 36 + 8) end,
    color = function() return C.surfaceContainerHighest end,
    visible = function() return menu_open:get() end,
    ui.Column { y = 4, gap = 0, table.unpack(menu_rows) },
  }
  local storage_card = kit.card {
    id = "performance-storage",
    width = STORAGE_W, height = ROW_H,
    storage_ring,
    ui.Column {
      x = 175, y = 60, gap = 4,
      kit.text { text = "Storage", font_size = theme.size.large + 1, font_weight = 500 },
      kit.text {
        id = "performance-storage-space",
        text = function()
          local d = disk()
          local total, unit = kit.bytes(d.total)
          local used = kit.bytes(d.used, unit)
          return ("%s / %s %s"):format(used, total, unit)
        end,
        font_size = theme.size.large - 1,
        color = function() return C.onSurfaceVariant end,
      },
    },
    picker_button({
      id = "performance-disk",
      x = 68, y = 156, width = 184, height = 40, cursor = "pointer",
      on_clicked = function()
        -- The next disk, round.
        local list = disks()
        for i, d in ipairs(list) do
          if d.name == disk().name then
            local nxt = list[i % #list + 1]
            if nxt then chosen:set(nxt.name) end
            return
          end
        end
      end,
      ui.Row {
        anchors = { center_in = true }, gap = 8, align = "center",
        kit.icon("storage", 18, function() return C.onSecondaryContainer end),
        kit.text {
          id = "performance-disk-name",
          text = function() return disk().name end,
          color = function() return C.onSecondaryContainer end,
        },
      },
    }, 20),
    picker_button({
      id = "performance-disk-more",
      x = 255, y = 156, width = 40, height = 40, cursor = "pointer",
      on_clicked = function() menu_open:set(not menu_open:get()) end,
      kit.icon(function() return menu_open:get() and "expand_less" or "expand_more" end, 20,
        function() return C.onSecondaryContainer end, { anchors = { center_in = true } }),
    }, 12),
    menu,
  }

  -- ---------------------------------------------------------- network --
  -- Totals since the shell started, as the reference counts them.
  local first = {}
  local function totals()
    local n = network()
    local p = n.primary
    if not p then return 0, 0 end
    local key = p.name
    if not first[key] then first[key] = { rx = p.rx_bytes, tx = p.tx_bytes } end
    return math.max(0, p.rx_bytes - first[key].rx), math.max(0, p.tx_bytes - first[key].tx)
  end
  local function rate(which)
    return function()
      local text, unit = kit.bytes(network()[which] or 0)
      return text .. " " .. unit .. "/s"
    end
  end
  local CHART_W, CHART_H, STEP = 200, 58, 13
  local chart = ui.Path {
    id = "performance-network-chart",
    anchors = { right = true, right_margin = 14 }, y = 57,
    width = CHART_W, height = CHART_H, view_box = { 0, 0, CHART_W, CHART_H },
    fill_color = function() return C.surfaceVariant end,
    stroke_color = function() return C.onSurfaceVariant end,
    stroke_width = 2, stroke_join = "round",
    d = function()
      network()
      local h = sysinfo.history("rx") or {}
      local n = math.min(#h, math.floor(CHART_W / STEP) + 1)
      if n < 2 then return "M0 0" end
      local peak = 1
      for i = #h - n + 1, #h do peak = math.max(peak, h[i] or 0) end
      local pts = {}
      for k = 1, n do
        local v = h[#h - n + k] or 0
        local x = CHART_W - (n - k) * STEP
        local y = CHART_H - 2 - (CHART_H - 8) * v / peak
        pts[#pts + 1] = ("%s%.1f %.1f"):format(k == 1 and "M" or "L", x, y)
      end
      local left = CHART_W - (n - 1) * STEP
      return table.concat(pts, " ") .. (" L%d %d L%.1f %d Z"):format(CHART_W, CHART_H, left, CHART_H)
    end,
  }
  local function line(icon, label, value, id, small)
    return ui.Item {
      width = NET_W - 44, height = 28,
      ui.Row {
        anchors = { vertical_center = true }, gap = 10, align = "center",
        kit.icon(icon, 20, function() return C.tertiary end),
        kit.text { text = label, font_size = theme.size.normal + 1, color = function() return C.onSurfaceVariant end },
      },
      kit.text {
        id = id,
        anchors = { right = true, vertical_center = true },
        text = value,
        font_size = small and theme.size.normal or theme.size.larger + 1,
        font_weight = small and 400 or 500,
        color = function() return small and C.onSurfaceVariant or C.tertiary end,
      },
    }
  end
  local network_card = kit.card {
    id = "performance-network",
    width = NET_W, height = ROW_H,
    ui.Row {
      x = 20, y = 16, gap = 10, align = "center",
      kit.icon("swap_vert", 22, function() return C.primary end),
      kit.text { text = "Network", font_size = theme.size.large + 1, font_weight = 500 },
    },
    chart,
    ui.Column {
      x = 22, y = 123, gap = 0,
      line("download", "Download", rate("rx_rate"), "performance-download"),
      line("upload", "Upload", rate("tx_rate"), "performance-upload"),
      line("history", "Total", function()
        local rx, tx = totals()
        local a, au = kit.bytes(rx)
        local b, bu = kit.bytes(tx)
        return ("↓%s %s ↑%s %s"):format(a, au, b, bu)
      end, "performance-network-total", true),
    },
  }

  -- ----------------------------------------------------------- memory --
  local memory_card = kit.card {
    id = "performance-memory",
    width = MEM_W, height = ROW_H,
    ui.Row {
      anchors = { horizontal_center = true }, y = 16, gap = 8, align = "center",
      kit.icon("memory_alt", 22, function() return C.tertiary end, { fill = true }),
      kit.text { text = "Memory", font_size = theme.size.large + 1, font_weight = 500 },
    },
    kit.gauge {
      id = "performance-memory-ring",
      anchors = { horizontal_center = true }, y = 58, size = 112, stroke = 8, from = 215, sweep = 290, gap = 4,
      value = function() return (memory().percent or 0) / 100 end,
      color = function() return C.tertiary end,
      track = function() return C.surfaceVariant end,
      ui.Column {
        anchors = { center_in = true }, gap = 0, align = "center",
        kit.text {
          id = "performance-memory-percent",
          text = function() return ("%d%%"):format(math.floor((memory().percent or 0) + 0.5)) end,
          font_size = theme.size.extra - 3, font_weight = 500,
          color = function() return C.tertiary end,
        },
        kit.text { text = "Used", font_size = theme.size.normal + 1 },
      },
    },
    kit.text {
      id = "performance-memory-space",
      anchors = { horizontal_center = true }, y = 180,
      text = function()
        local m = memory()
        local total, unit = kit.bytes(m.total)
        local used = kit.bytes(m.used, unit)
        return ("%s / %s %s"):format(used, total, unit)
      end,
      font_size = theme.size.larger + 1,
    },
  }

  return ui.Column {
    id = "dashboard-performance",
    width = M.WIDTH, height = M.HEIGHT, gap = GAP,
    cpu_card,
    ui.Row { gap = GAP, storage_card, network_card, memory_card },
  }
end

-- Built as the module loads, with its own instruction budget.
M.page = M.build(require("dashboard_state").context(3))

return M
