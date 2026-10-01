-- The dashboard's Battery tab: battop's view of the battery, in the
-- Performance tab's language -- a bar of its charge, graphs of its charge,
-- its draw, its voltage and its temperature over the last minutes, its
-- readings and what it is. The battery only: one page, full width.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local graphs = require("graphs")

local C = theme.color
local M = {}

M.WIDTH, M.HEIGHT = 1040, 548
local GAP = 12
local LEFT_W = 320
local RIGHT_W = M.WIDTH - LEFT_W - GAP
local PAD = 24

local function hue(name) return function() return theme.lule[name] end end
local CHARGE, DRAW, VOLTS, HEAT = hue("color2"), hue("color3"), hue("color4"), hue("color1")

function M.build(model)
  local opened, battery_state, battery, charging = model.active, model.state, model.battery, model.charging
  local num, duration_text = model.number, model.duration

  -- ---------------------------------------------------- the charge ring --
  -- Its charge as a ring, the number inside it; what it draws, how long it
  -- lasts and how well it holds a charge under it.
  local RING = 220
  local function reading(icon, label, value, color)
    return ui.Item {
      width = LEFT_W - 2 * PAD, height = 44,
      kit.surface { anchors = { fill = true }, radius = 14, color = function() return C.surfaceContainerHigh end },
      kit.icon(icon, 20, color, { x = 14, anchors = { vertical_center = true }, fill = true }),
      kit.section_label { x = 44, anchors = { vertical_center = true }, text = label,
        color = function() return C.onSurfaceVariant end },
      kit.text { anchors = { right = true, right_margin = 14, vertical_center = true },
        text = value, font_weight = 600 },
    }
  end
  local left = kit.card {
    id = "battery-charge-card", width = LEFT_W, height = M.HEIGHT,
    kit.gauge {
      id = "battery-ring",
      anchors = { horizontal_center = true }, y = 34, size = RING, stroke = 16, from = 215, sweep = 290, gap = 5,
      value = function() return (battery().capacity or 0) / 100 end,
      color = CHARGE,
      track = function() return C.surfaceContainerHighest end,
      ui.Column {
        anchors = { center_in = true }, gap = 2, align = "center",
        kit.icon(function() return charging() and "bolt" or "battery_full" end, 26, CHARGE, { fill = true }),
        kit.text {
          id = "battery-percent", font_size = theme.size.extra + 12, font_weight = 700,
          text = function() return ("%d%%"):format(math.floor((battery().capacity or 0) + 0.5)) end,
        },
        kit.text {
          text = function() return battery().status or "No battery" end,
          color = function() return C.onSurfaceVariant end,
        },
      },
    },
    kit.text {
      anchors = { horizontal_center = true }, y = 34 + RING + 4,
      font_size = theme.size.small, color = function() return C.onSurfaceVariant end,
      text = function()
        local b = battery()
        if not b.charge_limit or b.charge_limit >= 100 then return "" end
        return ("Charges to %d%%%s"):format(b.charge_limit, b.charge_mode and (" · " .. b.charge_mode) or "")
      end,
    },
    ui.Column {
      x = PAD, y = 34 + RING + 40, gap = 8,
      reading("bolt", "Power", function() return num(battery().power, "%.1f W") end, DRAW),
      reading("schedule", function() return charging() and "Full in" or "Lasts" end, function()
        local s = battery_state()
        return duration_text(charging() and s.time_to_full or s.time_left)
      end, CHARGE),
      reading("health_metrics", "Health", function() return num(battery().health, "%.0f %%") end, CHARGE),
      reading("thermostat", "Temperature", function() return num(battery().temperature, "%.1f °C") end, HEAT),
    },
  }
  if theme.motion.value_flash then
    theme.motion.value_flash(left,"battery-charge",{
      x=LEFT_W-12,y=28,height=32,active=opened,
      read=function()
        local b=battery()
        return {capacity=b.capacity,status=b.status}
      end,
      changed=function(before,now)
        return before.status~=now.status or
          (before.capacity and now.capacity and math.abs(now.capacity-before.capacity)>=5)
      end,
      cooldown=3000,
    })
  end

  -- ----------------------------------------------------------- graphs --
  local GW = math.floor((RIGHT_W - 2 * PAD - 16) / 2)
  local GH = 118
  local minutes = model.minutes
  local function small(caption, scale, spec)
    spec.width, spec.height, spec.active = GW, GH, opened
    return graphs.captioned(caption, scale, spec)
  end
  local grid = ui.Grid {
    columns = 2, column_gap = 16, row_gap = 14,
    small("Charge · " .. minutes, function() return "100%" end, {
      id = "battery-graph-charge", color = CHARGE, top = 100, columns = 8, rows = 4,
      first = function() return model.series("percent") end,
    }),
    small("Power · " .. minutes, function(top) return ("%.1f W"):format(top) end, {
      id = "battery-graph-power", color = DRAW, floor = 5, columns = 8, rows = 4,
      first = function() return model.series("power") end,
    }),
    small("Voltage · " .. minutes, function(top) return ("%.2f V"):format(top) end, {
      id = "battery-graph-voltage", color = VOLTS, columns = 8, rows = 4,
      -- Around the design voltage, so a charge's swing shows.
      bottom = model.voltage_bottom,
      top = model.voltage_top,
      first = function() return model.series("voltage") end,
    }),
    small("Temperature · " .. minutes, function(top) return ("%.0f °C"):format(top) end, {
      id = "battery-graph-temperature", color = HEAT, top = 60, columns = 8, rows = 4,
      first = function() return model.series("temperature") end,
    }),
  }

  -- What it is, in two columns.
  local function fact(label, value)
    return ui.Item {
      width = GW, height = 24,
      kit.section_label { text = label, font_size = theme.size.small, color = function() return C.onSurfaceVariant end },
      kit.text { anchors = { right = true }, text = value, font_size = theme.size.small, font_weight = 500 },
    }
  end
  local facts = ui.Grid {
    columns = 2, column_gap = 16, row_gap = 4,
    fact("Vendor", function() return battery().vendor or "" end),
    fact("Voltage", function() return num(battery().voltage, "%.2f V") end),
    fact("Model", function() return battery().model or "" end),
    fact("Energy now", function() return num(battery().energy, "%.1f Wh") end),
    fact("Technology", function() return battery().technology or "" end),
    fact("Last full", function() return num(battery().energy_full, "%.1f Wh") end),
    fact("Serial", function() return battery().serial or "" end),
    fact("Full design", function() return num(battery().energy_design, "%.1f Wh") end),
    fact("Cycles", function() return num(battery().cycles, "%d") end),
    fact("Charge limit", model.limit),
  }

  local right = kit.card {
    id = "battery-main", width = RIGHT_W, height = M.HEIGHT,
    ui.Item {
      x = PAD, y = 18, width = RIGHT_W - 2 * PAD, height = 40,
      kit.heading { id = "battery-title", active = model.active, anchors = { vertical_center = true }, text = "Battery", font_size = theme.size.extra - 4, font_weight = 700 },
      kit.subtitle {
        anchors = { right = true, vertical_center = true }, font_size = theme.size.normal,
        color = function() return C.onSurfaceVariant end,
        text = function()
          local b = battery()
          return (((b.vendor or "") .. " " .. (b.model or "")):gsub("^%s+", ""))
        end,
      },
    },
    ui.Item { x = PAD, y = 70, grid },
    ui.Item { x = PAD, y = 70 + 2 * (GH + 22) + 14 + 22, facts },
  }

  return {page=ui.Row { id = "dashboard-battery", width = M.WIDTH, height = M.HEIGHT, gap = GAP, left, right }}
end

return M
