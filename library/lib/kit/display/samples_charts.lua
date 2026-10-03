-- Gallery samples for the charts display widgets: name -> function(kit) returning a
-- node that fits a 260x190 cell (examples/shells/caelestia/tests/kit_gallery_spec.lua).
local W, H = 260, 190

local function wave(n, f)
  local t = {}
  for i = 1, n do t[i] = f(i) end
  return t
end

-- A small deterministic noise, so every run draws the same picture.
local seed = 7
local function noise()
  seed = (seed * 1103515245 + 12345) % 2147483648
  return seed / 2147483648
end

return {
  bars = function(kit)
    return kit.bars { width = W, height = H, title = "Weekly commits",
      values = { 12, 19, 7, 15, 22, 9, 17 }, labels = { "Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun" } }
  end,
  stacked = function(kit)
    return kit.stacked { width = W, height = H, title = "Memory by kind",
      series = { { 4, 5, 6, 5, 7, 6 }, { 2, 3, 2, 4, 3, 4 }, { 1, 1, 2, 1, 2, 2 } },
      legend = { "Apps", "Cache", "Swap" }, labels = { "1h", "2h", "3h", "4h", "5h", "6h" } }
  end,
  stacked_area = function(kit)
    return kit.stacked_area { width = W, height = H, title = "Traffic",
      series = { wave(32, function(i) return 4 + 2 * math.sin(i / 5) end),
        wave(32, function(i) return 3 + 1.5 * math.cos(i / 4) end),
        wave(32, function(i) return 2 + math.sin(i / 3 + 1) end) },
      legend = { "Web", "API", "Sync" } }
  end,
  histogram = function(kit)
    seed = 11
    local values = wave(240, function()
      return (noise() + noise() + noise() + noise()) * 25
    end)
    return kit.histogram { width = W, height = H, title = "Latency (ms)", values = values, bins = 14 }
  end,
  scatter = function(kit)
    seed = 3
    local a = wave(24, function(i) return { i, i * 0.8 + noise() * 8 } end)
    local b = wave(18, function(i) return { i + 4, 20 - i * 0.6 + noise() * 6 } end)
    return kit.scatter { width = W, height = H, title = "Load vs temp", series = { a, b }, legend = { "CPU", "GPU" } }
  end,
  pie = function(kit)
    return kit.pie { width = W, height = H, title = "Disk usage",
      values = { 48, 22, 14, 9, 7 }, labels = { "System", "Home", "Media", "Cache", "Other" } }
  end,
  donut = function(kit)
    return kit.donut { width = W, height = H, title = "Battery drain",
      values = { 38, 26, 18, 18 }, labels = { "Screen", "CPU", "Radio", "Idle" }, unit = "%" }
  end,
  radial_bar = function(kit)
    return kit.radial_bar { width = W, height = H, title = "Resources",
      values = { 0.72, 0.48, 0.86, 0.31 }, labels = { "CPU", "RAM", "Disk", "Net" } }
  end,
  heatmap = function(kit)
    local rows = {}
    for r = 1, 5 do
      rows[r] = wave(12, function(c) return 0.5 + 0.45 * math.sin(c / 2 + r) * math.cos(r / 2 + c / 5) end)
    end
    return kit.heatmap { width = W, height = H, title = "Activity by hour", values = rows,
      row_labels = { "Mon", "Tue", "Wed", "Thu", "Fri" }, column_labels = { "0", "2", "4", "6", "8", "10", "12", "14", "16", "18", "20", "22" } }
  end,
  calendar_heatmap = function(kit)
    seed = 5
    local days = wave(18 * 7, function(i)
      local weekday = (i - 1) % 7
      local v = noise() * (weekday >= 5 and 0.4 or 1)
      return v < 0.15 and 0 or v
    end)
    return kit.calendar_heatmap { width = W, height = H, title = "Contributions", values = days, weeks = 18,
      months = { { 1, "Jun" }, { 5, "Jul" }, { 10, "Aug" }, { 14, "Sep" } } }
  end,
  waveform = function(kit)
    seed = 9
    local amp = wave(160, function(i)
      local env = 0.35 + 0.6 * math.abs(math.sin(i / 18))
      return env * (0.55 + 0.45 * noise())
    end)
    return kit.waveform { width = W, height = H, title = "Microphone", values = amp, top = 1 }
  end,
  spectrogram = function(kit)
    seed = 13
    local columns = {}
    for c = 1, 40 do
      columns[c] = wave(20, function(r)
        local band = math.exp(-((r - 5 - 3 * math.sin(c / 6)) ^ 2) / 6) + 0.6 * math.exp(-((r - 14) ^ 2) / 3) * math.abs(math.sin(c / 4))
        return math.min(1, band * 0.9 + noise() * 0.18)
      end)
    end
    return kit.spectrogram { width = W, height = H, title = "Spectrogram", values = columns, columns = 40 }
  end,
  candlestick = function(kit)
    seed = 17
    local price, candles = 40, {}
    for i = 1, 18 do
      local open = price
      local close = open + (noise() - 0.47) * 8
      candles[i] = { open, math.max(open, close) + noise() * 3, math.min(open, close) - noise() * 3, close }
      price = close
    end
    return kit.candlestick { width = W, height = H, title = "Price", values = candles }
  end,
  box_plot = function(kit)
    return kit.box_plot { width = W, height = H, title = "Build times",
      values = { { 12, 18, 22, 27, 35 }, { 8, 14, 17, 21, 30 }, { 15, 21, 26, 30, 41 }, { 10, 13, 15, 19, 24 } },
      labels = { "Core", "UI", "Lua", "Docs" } }
  end,
  state_timeline = function(kit)
    local function run(pattern)
      local t = {}
      for _, seg in ipairs(pattern) do for _ = 1, seg[2] do t[#t + 1] = seg[1] end end
      return t
    end
    return kit.state_timeline { width = W, height = H, title = "Services",
      series = {
        { label = "API", values = run { { 0, 14 }, { 1, 4 }, { 0, 18 }, { 2, 3 }, { 0, 9 } } },
        { label = "DB", values = run { { 0, 30 }, { 1, 8 }, { 0, 10 } } },
        { label = "Cache", values = run { { 0, 6 }, { 2, 5 }, { 1, 6 }, { 0, 31 } } },
      } }
  end,
  status_history = function(kit)
    seed = 21
    local function row(bad)
      return wave(20, function()
        local v = noise()
        return v < bad and 2 or (v < bad * 2.5 and 1 or 0)
      end)
    end
    return kit.status_history { width = W, height = H, title = "Uptime",
      series = { { label = "Web", values = row(0.05) }, { label = "Mail", values = row(0.1) },
        { label = "VPN", values = row(0.15) }, { label = "DNS", values = row(0.02) } } }
  end,
  gantt = function(kit)
    return kit.gantt { width = W, height = H, title = "Release plan", now = 9,
      values = {
        { label = "Design", start = 0, finish = 5, progress = 1 },
        { label = "Engine", start = 3, finish = 12, progress = 0.6 },
        { label = "Themes", start = 6, finish = 14, progress = 0.3 },
        { label = "Docs", start = 10, finish = 16, progress = 0 },
      },
      ticks = { { 0, "W1" }, { 4, "W2" }, { 8, "W3" }, { 12, "W4" }, { 16, "W5" } } }
  end,
  treemap = function(kit)
    return kit.treemap { width = W, height = H, title = "Storage",
      values = { children = {
        { name = "Media", children = { { name = "Video", value = 40 }, { name = "Music", value = 18 }, { name = "Photos", value = 12 } } },
        { name = "Code", children = { { name = "mold", value = 16 }, { name = "rust", value = 9 } } },
        { name = "Games", value = 22 },
        { name = "Docs", children = { { name = "PDF", value = 6 }, { name = "Notes", value = 3 } } },
      } } }
  end,
  sunburst = function(kit)
    return kit.sunburst { width = W, height = H, title = "Time spent",
      values = { children = {
        { name = "Work", children = { { name = "Code", value = 5 }, { name = "Mail", value = 2 }, { name = "Meet", value = 2 } } },
        { name = "Home", children = { { name = "Cook", value = 2 }, { name = "Read", value = 3 } } },
        { name = "Sleep", value = 7 },
        { name = "Play", children = { { name = "Games", value = 1.5 }, { name = "Walk", value = 1.5 } } },
      } } }
  end,
  sankey = function(kit)
    return kit.sankey { width = W, height = H, title = "Power flow",
      values = { nodes = { "Grid", "Solar", "House", "Battery", "Heat", "Light" },
        links = {
          { source = "Grid", target = "House", value = 5 }, { source = "Solar", target = "House", value = 3 },
          { source = "Solar", target = "Battery", value = 2 }, { source = "House", target = "Heat", value = 5 },
          { source = "House", target = "Light", value = 3 },
        } } }
  end,
  funnel = function(kit)
    return kit.funnel { width = W, height = H, title = "Signups",
      values = { 1200, 640, 310, 120 }, labels = { "Visits", "Sign-up", "Verified", "Paid" } }
  end,
  flame_graph = function(kit)
    return kit.flame_graph { width = W, height = H, title = "CPU profile",
      values = { name = "main", value = 100, children = {
        { name = "render", value = 55, children = {
          { name = "layout", value = 20, children = { { name = "text", value = 12 } } },
          { name = "paint", value = 30, children = { { name = "gpu", value = 18, children = { { name = "upload", value = 8 } } } } },
        } },
        { name = "lua", value = 30, children = { { name = "gc", value = 8 }, { name = "bind", value = 16 } } },
        { name = "io", value = 10 },
      } } }
  end,
}
