-- System statistics: each card pairs a figure with its recent history.
-- Weighted rather than evenly tiled: the processor gets the most room,
-- storage a corner.
--
-- Port of StatsPanel.qml. `morf ipc call stats` toggles it;
-- `stats.warm [n]` fills the history at once, for a screenshot taken right
-- after boot.

local ui = require("morf.ui")
local theme = require("theme")
local island = require("bar.island")
local kit = require("components.kit")
local stat_card = require("components.stat_card")
local stats = require("services.stats")

local C = theme.color

local PANEL_W, PANEL_H = 940, 614
local GAP = 12
local INNER_W = PANEL_W - 2 * theme.panel_padding
local INNER_H = PANEL_H - 2 * theme.panel_padding
local LEFT_W = math.floor((INNER_W - GAP) * 3 / 5)
local RIGHT_W = INNER_W - GAP - LEFT_W

local fast = function() return theme.behave("fast") end
local medium = function() return theme.behave("medium") end

local function has_temperature() return stats.temperature() ~= nil end

-- The rows the Repeaters follow: they change when a core or a mount comes
-- or goes, not with every sample.
local bars = morf.list_model({})
morf.effect("impasto.stats.cores", function()
  local rows = {}
  for index = 1, #stats.cores() do rows[index] = { index = index } end
  bars:replace(rows, "index")
end)
local disks = morf.list_model({})
morf.effect("impasto.stats.disks", function()
  local rows = {}
  for _, disk in ipairs(stats.disks()) do rows[#rows + 1] = { target = disk.target } end
  disks:replace(rows, "target")
end)

-- Per core, because an average hides a single pinned core.
local function cores(width)
  local bar_w = function()
    local n = math.max(1, #stats.cores())
    return math.max(2, (width - 3 * (n - 1)) / n)
  end
  return ui.Repeater {
    as = "row", gap = 3,
    model = bars,
    delegate = function(row)
      local value = function() return stats.cores()[row.index] or 0 end
      return ui.Rect {
        width = bar_w, height = 20, radius = 2, color = C.islandSurfaceHover,
        ui.Rect {
          anchors = { left = true, right = true, bottom = true },
          height = function() return math.max(2, 20 * math.min(1, value() / 100)) end,
          radius = 2,
          color = function() return value() > 80 and C.red() or C.accent() end,
          behavior = { height = medium(), color = fast() },
        },
      }
    end,
  }
end

local function swap(width)
  local used = kit.text { text = function() return stats.bytes(stats.swap_used()) end,
    mono = true, size = 9, color = C.textMuted }
  local caption = kit.text { text = "SWAP", size = 9, weight = 600, color = C.textMuted }
  local track_w = function() return width - (caption.layout_width or 26) - (used.layout_width or 40) - 16 end
  return ui.Row {
    gap = 8, align = "center",
    visible = function() return stats.swap_total() > 0 end,
    caption,
    ui.Rect {
      width = track_w, height = 4, radius = 2, color = C.islandSurfaceHover,
      ui.Rect {
        height = 4, radius = 2, color = C.yellow,
        width = function() return track_w() * stats.swap_fraction() end,
        behavior = { width = medium() },
      },
    },
    used,
  }
end

local function storage(width, height)
  local inner = width - 28
  return ui.Rect {
    width = width, height = height, radius = theme.radius_medium,
    color = C.islandSurface, border_width = 1, border_color = C.islandBorder,
    ui.Column {
      x = 14, y = 14, gap = 8,
      ui.Row {
        gap = 9, align = "center",
        kit.glyph { glyph = "󰋊", size = 15, color = C.accent },
        kit.text { text = "Storage", size = theme.size.small, weight = 600 },
      },
      ui.Repeater {
        as = "column", gap = 8,
        model = disks,
        delegate = function(row)
          local disk = function()
            for _, d in ipairs(stats.disks()) do if d.target == row.target then return d end end
            return { total = 0, used = 0 }
          end
          local fraction = function()
            local d = disk()
            return d.total > 0 and d.used / d.total or 0
          end
          return ui.Column {
            gap = 3,
            ui.Item {
              width = inner, height = 14,
              kit.text { anchors = { left = true, vertical_center = true }, text = row.target,
                mono = true, size = theme.size.label, width = inner - 90, elide = "middle" },
              -- Free space, not used: how much room is left.
              kit.text { anchors = { right = true, vertical_center = true },
                text = function() local d = disk() return stats.bytes(d.total - d.used) .. " free" end,
                mono = true, size = 9, color = C.textMuted },
            },
            ui.Rect {
              width = inner, height = 5, radius = 2.5, color = C.islandSurfaceHover,
              ui.Rect {
                height = 5, radius = 2.5,
                width = function() return inner * fraction() end,
                color = function()
                  local f = fraction()
                  if f > 0.9 then return C.red() end
                  return f > 0.75 and C.yellow() or C.accent()
                end,
                behavior = { width = medium(), color = fast() },
              },
            },
          }
        end,
      },
    },
  }
end

local function build()
  local cpu_h = math.floor((INNER_H - GAP) * 5 / 9)
  local memory_h = INNER_H - GAP - cpu_h
  -- The right column: temperature (when a sensor answers), network, storage.
  local right_n = function() return has_temperature() and 3 or 2 end
  local right_h = function() return (INNER_H - GAP * (right_n() - 1)) / right_n() end

  return ui.Item {
    width = INNER_W, height = INNER_H,
    stat_card {
      x = 0, y = 0, width = LEFT_W, height = cpu_h,
      icon = "󰻠", title = "Processor",
      reading = function() return ("%.0f%%"):format(stats.cpu()) end,
      detail = function()
        local load = stats.load()
        return ("load %.2f  ·  %d threads  ·  up %s"):format(load[1] or 0, #stats.cores(), stats.duration(stats.uptime()))
      end,
      series = function() return stats.history("cpu") end,
      accent = C.accent,
      extra = cores(LEFT_W - 28), extra_height = 20,
    },
    stat_card {
      x = 0, y = cpu_h + GAP, width = LEFT_W, height = memory_h,
      icon = "󰍛", title = "Memory",
      reading = function() return ("%.0f%%"):format(stats.memory_fraction() * 100) end,
      detail = function() return stats.bytes(stats.memory_used()) .. " of " .. stats.bytes(stats.memory_total()) end,
      series = function() return stats.history("memory") end,
      accent = C.blue,
      extra = swap(LEFT_W - 28), extra_height = 12,
    },
    stat_card {
      x = LEFT_W + GAP, y = 0, width = RIGHT_W, height = right_h,
      visible = has_temperature,
      icon = "󰔏", title = "Temperature",
      reading = function() local t = stats.temperature() return t and (t.celsius .. "°") or "—" end,
      detail = function()
        local t = stats.temperature()
        return t and (stats.thermal_word() .. "  ·  " .. t.label) or ""
      end,
      -- Degrees have no ceiling worth drawing against: the range seen.
      maximum = 0,
      series = function() return stats.history("temperature") end,
      accent = function()
        local t = stats.temperature()
        return (t and t.celsius > 80) and C.red() or C.yellow()
      end,
    },
    stat_card {
      x = LEFT_W + GAP,
      y = function() return has_temperature() and (right_h() + GAP) or 0 end,
      width = RIGHT_W, height = right_h,
      icon = "󰛳", title = "Network",
      reading = function() return stats.rate(stats.down()) end,
      detail = function() return "↑ " .. stats.rate(stats.up()) end,
      maximum = 0,
      series = function() return stats.history("down") end,
      accent = C.green,
      -- Upload on its own line under the download, so the shape of a
      -- transfer shows rather than being split across two cards.
      extra = require("components.sparkline") {
        width = RIGHT_W - 28, height = 22, maximum = 0, thickness = 1.2,
        values = function() return stats.history("up") end, stroke = C.blue,
      },
      extra_height = 22,
    },
    ui.Item {
      x = LEFT_W + GAP,
      y = function() return (right_n() - 1) * (right_h() + GAP) end,
      width = RIGHT_W, height = right_h,
      storage(RIGHT_W, right_h),
    },
  }
end

island.register("stats", {
  size = function() return PANEL_W, PANEL_H end,
  build = build,
})

morf.ipc["stats.warm"] = function(n)
  stats.warm(tonumber(n) or 40)
  return "ok"
end
