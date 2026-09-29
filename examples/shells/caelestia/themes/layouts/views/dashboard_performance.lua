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

function M.build(model)
  morf.effect("material.performance.present",function() model.present(model.selected:get()) end)
  local opened, shown = model.active, model.active
  local cpu, memory, drives, network = model.cpu, model.memory, model.drives, model.network
  local gpus, fans, temps, system = model.gpus, model.fans, model.temps, model.system
  local history, list, selected, picked, on = model.history, model.list, model.selected, model.picked, model.on
  local drive, iface, card, fan = model.drive, model.iface, model.card, model.fan
  local rate_text, size_text, ghz, duration_text = model.rate, model.size, model.ghz, model.duration
  local function hue(kind) return function() return theme.lule[HUE[kind]] end end

  -- -------------------------------------------------------------- graph --
  -- Mission Center's graph: a bordered box on a faint grid, the first
  -- series filled under its line, the second a dashed line; the newest
  -- sample at the right edge.
  local function series_path(values, w, h, top, closed)
    local n = model.samples
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
    return kit.surface(children), top
  end

  --- A graph with its caption over it, left, and its scale, right.
  local building_kind = "cpu"
  local function captioned(caption, scale, spec)
    local kind = building_kind
    local box, top = graph(spec)
    return ui.Column {
      gap = 4,
      ui.Item {
        width = spec.width, height = 18,
        kit.heading { text = caption, active = function() return opened() and on(kind)() end, level = "caption", width = spec.width - 72, font_size = theme.size.small, color = function() return C.onSurfaceVariant end },
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
  local function minutes(section)
    return ("over %d minutes"):format(math.floor(model.samples * model.intervals[section] / 60000 + 0.5))
  end

  -- -------------------------------------------------------------- stats --
  -- The readings: a small label over a large value, two to a row; a
  -- coloured bar beside those a graph draws (solid, or dotted for the
  -- dashed series).
  local function stat(label, value, mark, kind)
    local column = ui.Column {
      gap = 0, width = STATS_W / 2 - 6,
      kit.section_label { text = label, font_size = theme.size.small, color = function() return C.onSurfaceVariant end },
      kit.text {
        text = value, font_size = theme.size.large, font_weight = 600,
        width = STATS_W / 2 - 14, elide = "right",
      },
    }
    if not mark then return column end
    return ui.Row {
      gap = 6,
      kit.surface {
        width = 2, height = 40, radius = 1,
        color = function() return hue(kind)():alpha(mark == "dashed" and 0.5 or 1) end,
      },
      column,
    }
  end
  local function stats(kind)
    local items={}
    for _,row in ipairs(model.readouts[kind].stats) do items[#items+1]=stat(row.label,row.value,row.mark,row.kind) end
    return ui.Grid { columns = 2, column_gap = 12, row_gap = 18, table.unpack(items) }
  end
  --- What the device is: label, value lines.
  local function facts(rows)
    local column = { gap = 9 }
    for _, row in ipairs(rows) do
      column[#column + 1] = ui.Row {
        gap = 8,
        kit.section_label {
          text = row[1], width = 136, font_size = theme.size.small,
          color = function() return C.onSurfaceVariant end,
        },
        kit.text { text = row[2], width = STATS_W - 144, elide = "right", font_size = theme.size.small },
      }
    end
    return ui.Column(column)
  end

  local title_index = 0
  local title_kinds = { "cpu", "memory", "drive", "net", "gpu", "fan" }
  local function title(text_fn, model_fn)
    title_index = title_index + 1
    local kind = title_kinds[title_index]
    building_kind = kind
    return ui.Item {
      x = PAD, y = 22, width = MAIN_W - 2 * PAD, height = 44,
      kit.heading { id = "performance-title-" .. kind, active = function() return opened() and on(kind)() end,
        anchors = { vertical_center = true },
        text = text_fn, font_size = theme.size.extra - 2, font_weight = 700,
      },
      kit.subtitle {
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
  local info = model.info
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
    title(function() return "CPU" end, function() return model.cpu_name(info.model) end),
    ui.Column {
      x = PAD, y = TOP, gap = 4,
      ui.Item {
        width = GRAPH_W, height = 18,
        kit.heading { id = "performance-utilization-title", active = function() return opened() and on("cpu")() end,
          text = "Utilization " .. minutes("cpu"), level = "caption", width = GRAPH_W - 60,
          font_size = theme.size.small, color = function() return C.onSurfaceVariant end },
        kit.text { anchors = { right = true }, text = "100%", font_size = theme.size.small, color = function() return C.onSurfaceVariant end },
      },
      ui.Grid(cells),
    },
    ui.Column {
      x = stats_x, y = TOP, gap = 28,
      stats("cpu"),
      facts(model.readouts.cpu.facts),
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
      captioned("Memory usage " .. minutes("memory"), function() return size_text(memory().total) end, {
        id = "performance-memory-graph", kind = "memory", width = GRAPH_W, height = mem_h, top = 100,
        first = function() return history("memory") end,
      }),
      captioned("Swap usage " .. minutes("memory"), function() return size_text(memory().swap.total) end, {
        id = "performance-swap-graph", kind = "memory", width = GRAPH_W, height = swap_h, top = 100,
        first = function() return history("swap") end,
      }),
      ui.Column {
        gap = 4,
        kit.heading { id = "performance-memory-composition-title", level = "caption",
          active = function() return opened() and on("memory")() end, text = "Memory composition", font_size = theme.size.small, color = function() return C.onSurfaceVariant end },
        kit.surface {
          id = "performance-memory-composition",
          width = GRAPH_W, height = 40, radius = 3, clip = true,
          color = function() return hue("memory")():alpha(0.04) end,
          border_width = 1, border_color = function() return hue("memory")():alpha(0.85) end,
          kit.surface {
            height = 40,
            width = function() return GRAPH_W * (composition()) end,
            color = function() return hue("memory")():alpha(0.35) end,
          },
          kit.surface {
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
      stats("memory"),
      facts(model.readouts.memory.facts),
    },
  })

  -- -------------------------------------------------------------- drive --
  local drive_ref,the_drive,units,unit=model.drive_ref,model.the_drive,model.units,model.unit
  local drive_graph_h = 200
  local units_y = TOP + 2 * (drive_graph_h + 22 + 10)
  local drive_page = page("drive", {
    title(function()
      local d = the_drive()
      return ("%s (%s)"):format(d.kind or "Drive", d.name or "")
    end, function() return the_drive().model or "" end),
    ui.Column {
      x = PAD, y = TOP, gap = 10,
      captioned("Active time " .. minutes("drives"), percent_scale, {
        id = "performance-drive-active", kind = "drive", width = GRAPH_W, height = drive_graph_h, top = 100,
        first = function() return history("disk:" .. drive_ref() .. ":busy") end,
      }),
      captioned("Throughput " .. minutes("drives"), rate_scale, {
        id = "performance-drive-throughput", kind = "drive", width = GRAPH_W, height = drive_graph_h,
        first = function() return history("disk:" .. drive_ref() .. ":read") end,
        second = function() return history("disk:" .. drive_ref() .. ":write") end,
      }),
    },
    -- Its logical units: the partitions and what is built on them.
    ui.Item {
      x = PAD, y = units_y, width = GRAPH_W, height = M.HEIGHT - units_y - PAD,
      kit.heading { id = "performance-logical-units-title", level = "caption",
        active = function() return opened() and on("drive")() end, text = "Logical units", font_size = theme.size.small, color = function() return C.onSurfaceVariant end },
      ui.Flickable {
        y = 22, width = GRAPH_W, height = M.HEIGHT - units_y - PAD - 22, clip = true,
        ui.Repeater {
            as = "column", gap = 4, width = GRAPH_W,
            model = units.rows,
            delegate = function(row)
              local function u() return unit(row.name) end
              return kit.surface {
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
      stats("drive"),
      facts(model.readouts.drive.facts),
    },
  })

  -- ------------------------------------------------------------ network --
  local net_ref,the_iface,addresses,totals=model.net_ref,model.the_iface,model.addresses,model.totals
  local net_page = page("net", {
    title(function() return the_iface().wireless and "Wi-Fi" or "Ethernet" end,
      function() return net_ref() end),
    ui.Column {
      x = PAD, y = TOP,
      captioned("Throughput " .. minutes("network"), rate_scale, {
        id = "performance-net-throughput", kind = "net", width = GRAPH_W, height = area_h - 22,
        first = function() return history("rx:" .. net_ref()) end,
        second = function() return history("tx:" .. net_ref()) end,
      }),
    },
    ui.Column {
      x = stats_x, y = TOP, gap = 28,
      stats("net"),
      facts(model.readouts.net.facts),
    },
  })

  -- ---------------------------------------------------------------- gpu --
  local gpu_ref,the_card,gpu_index=model.gpu_ref,model.the_card,model.gpu_index
  local gpu_page = page("gpu", {
    title(function() return "GPU " .. gpu_index() end, function() return the_card().model or "" end),
    -- One graph for a GPU sharing the system's memory; Mission Center's
    -- three -- utilisation, video engines, its own memory -- for one with
    -- memory of its own.
    ui.Column {
      x = PAD, y = TOP,
      visible = function() local g = the_card() return not g.suspended and not g.vram_total end,
      captioned("Utilization " .. minutes("gpu"), percent_scale, {
        id = "performance-gpu-graph", kind = "gpu", width = GRAPH_W, height = area_h - 22, top = 100,
        first = function() return history("gpu:" .. gpu_ref()) end,
      }),
    },
    ui.Column {
      x = PAD, y = TOP, gap = 14,
      visible = function() local g = the_card() return not g.suspended and g.vram_total ~= nil end,
      captioned("Utilization " .. minutes("gpu"), percent_scale, {
        id = "performance-gpu-busy", kind = "gpu", width = GRAPH_W, height = area_h - 3 * 22 - 28 - 2 * 150, top = 100,
        first = function() return history("gpu:" .. gpu_ref()) end,
      }),
      captioned("Video encode/decode " .. minutes("gpu"), percent_scale, {
        id = "performance-gpu-video", kind = "gpu", width = GRAPH_W, height = 150, top = 100,
        first = function() return history("gpuenc:" .. gpu_ref()) end,
        second = function() return history("gpudec:" .. gpu_ref()) end,
      }),
      captioned("Memory usage " .. minutes("gpu"), function() return size_text(the_card().vram_total) end, {
        id = "performance-gpu-memory", kind = "gpu", width = GRAPH_W, height = 150, top = 100,
        first = function() return history("gpumem:" .. gpu_ref()) end,
      }),
    },
    kit.surface {
      x = PAD, y = TOP + 22, width = GRAPH_W, height = area_h - 22, radius = 3,
      visible = function() return the_card().suspended == true end,
      color = function() return hue("gpu")():alpha(0.04) end,
      border_width = 1, border_color = function() return hue("gpu")():alpha(0.5) end,
      ui.Column {
        anchors = { center_in = true }, gap = 6, align = "center",
        kit.icon("power_settings_new", 40, function() return C.onSurfaceVariant end),
        kit.heading { id = "performance-gpu-sleep-title", text = "Powered down", level = "section",
          active = function() return shown() and (picked()) == "gpu" and the_card().suspended == true end,
          font_size = theme.size.large, font_weight = 600 },
        kit.subtitle {
          text = "Not read while it sleeps: reading it would wake it.",
          font_size = theme.size.small, color = function() return C.onSurfaceVariant end,
        },
      },
    },
    ui.Column {
      x = stats_x, y = TOP, gap = 28,
      stats("gpu"),
      facts(model.readouts.gpu.facts),
    },
  })

  -- ---------------------------------------------------------------- fan --
  local fan_ref,the_fan=model.fan_ref,model.the_fan
  local fan_page = page("fan", {
    title(function() return "Fan " .. (fan_ref():gsub("^fan", "")) end, function() return the_fan().label or "" end),
    ui.Column {
      x = PAD, y = TOP,
      captioned("Speed " .. minutes("fans"), function(top) return ("%d RPM"):format(math.floor(top)) end, {
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
      stats("fan"),
      facts(model.readouts.fan.facts),
    },
  })

  -- ------------------------------------------------------------ the list --
  local row_title,row_sub,row_value,row_series=model.row_title,model.row_sub,model.row_value,model.row_series
  local function device_row(row)
    local first, second, top = row_series(row)
    local spark = graph {
      kind = row.kind, width = SPARK_W, height = SPARK_H, grid = false, top = top,
      first = first, second = second,
    }
    local area
    area = kit.action {
      id = "performance-device-" .. row.key,
      width = SIDE_W - 20, height = ROW_H, cursor = "pointer",
      on_clicked = function() selected:set(row.key) end,
      kit.surface {
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
    kit.heading { id = "performance-devices-title", active = opened,
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

  return {page=ui.Row {
    id = "dashboard-performance",
    width = M.WIDTH, height = M.HEIGHT, gap = GAP,
    side, main,
  }}
end

return M
