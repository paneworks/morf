-- The dashboard's Battery tab: battop's view of the battery, in the
-- Performance tab's language -- a bar of its charge, graphs of its charge,
-- its draw, its voltage and its temperature over the last minutes, its
-- readings and what it is. The battery only: one page, full width.

local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local graphs = require("graphs")
local services = require("services")
local sysinfo = require("lib.sysinfo")

local C = theme.color
local M = {}

M.WIDTH, M.HEIGHT = 1400, 760
local GAP = 12
local MAIN_W = M.WIDTH
local PAD = 28
local STATS_W = 300
local GRAPH_W = MAIN_W - 3 * PAD - STATS_W
local TOP = 84

local function hue(name) return function() return theme.lule[name] end end
local CHARGE, DRAW, VOLTS, HEAT = hue("color2"), hue("color3"), hue("color4"), hue("color1")

local function duration_text(seconds)
  if not seconds or seconds <= 0 then return "--" end
  local h, m = seconds // 3600, (seconds % 3600) // 60
  if h > 0 then return ("%dh %02dm"):format(h, m) end
  return ("%dm"):format(m)
end
local function num(v, fmt) return v and fmt:format(v) or "--" end

function M.build(ctx)
  local opened = ctx.opened
  local function battery_state()
    if not opened() then return { batteries = {} } end
    local ok, v = pcall(sysinfo.battery)
    return ok and v or { batteries = {} }
  end
  local function history(name)
    if not opened() then return {} end
    local ok, list = pcall(sysinfo.history, name)
    return ok and list or {}
  end
  -- The machine's battery (the first, when it has more than one).
  local function battery() return battery_state().batteries[1] or {} end

  local minutes = ("over %d minutes"):format(math.floor(graphs.SAMPLES * 3 / 60 + 0.5))
  local stats_x = PAD + GRAPH_W + PAD

  local function title(text_fn, model_fn)
    return ui.Item {
      x = PAD, y = 22, width = MAIN_W - 2 * PAD, height = 44,
      kit.text { anchors = { vertical_center = true }, text = text_fn, font_size = theme.size.extra - 2, font_weight = 700 },
      kit.text {
        anchors = { right = true, vertical_center = true },
        width = MAIN_W - 2 * PAD - 320, horizontal_alignment = "right", elide = "right",
        text = model_fn, font_size = theme.size.large, font_weight = 600,
      },
    }
  end

  -- The charge as battop draws it: one wide bar, filled to the charge,
  -- the charge limit marked when the battery keeps one.
  local BAR_H = 48
  local function charge_bar(id, percent, limit, color)
    return ui.Rect {
      id = id, width = GRAPH_W, height = BAR_H, radius = 12, clip = true,
      color = function() return color():alpha(0.08) end,
      border_width = 1, border_color = function() return color():alpha(0.6) end,
      ui.Rect {
        height = BAR_H, radius = 12,
        width = function() return math.max(0, math.min(1, (percent() or 0) / 100)) * GRAPH_W end,
        color = function() return color():alpha(0.55) end,
        behavior = { width = { duration = 600, easing = theme.ease.standard } },
      },
      ui.Rect {
        width = 2, height = BAR_H,
        x = function() return math.max(0, math.min(1, (limit() or 100) / 100)) * GRAPH_W - 1 end,
        visible = function() local l = limit() return l ~= nil and l < 100 end,
        color = function() return C.onSurface:alpha(0.7) end,
      },
      kit.text {
        anchors = { center_in = true }, font_size = theme.size.large, font_weight = 700,
        text = function() return ("%.0f %%"):format(percent() or 0) end,
      },
    }
  end

  local function page(_, contents)
    contents.width, contents.height = MAIN_W, M.HEIGHT
    return ui.Item(contents)
  end

  -- ------------------------------------------------------------ battery --
  local graphs_top = TOP + BAR_H + 16
  local area = M.HEIGHT - graphs_top - PAD
  local each = math.floor((area - 4 * 22 - 3 * 12) / 4)
  local function bat(field) return function() return battery()[field] end end
  local function key(name) return "bat:" .. (battery().name or "BAT0") .. ":" .. name end
  local battery_page = page("bat", {
    title(function() return ("Battery (%s)"):format(battery().name or "") end,
      function() local b = battery() return (((b.vendor or "") .. " " .. (b.model or "")):gsub("^%s+", "")) end),
    ui.Item {
      x = PAD, y = TOP,
      charge_bar("battery-charge", bat("capacity"), bat("charge_limit"), CHARGE),
    },
    ui.Column {
      x = PAD, y = graphs_top, gap = 12,
      graphs.captioned("Charge " .. minutes, function() return "100%" end, {
        id = "battery-graph-charge", width = GRAPH_W, height = each, color = CHARGE, top = 100,
        first = function() return history(key("percent")) end,
      }),
      graphs.captioned("Power " .. minutes, function(top) return ("%.1f W"):format(top) end, {
        id = "battery-graph-power", width = GRAPH_W, height = each, color = DRAW, floor = 5,
        first = function() return history(key("power")) end,
      }),
      graphs.captioned("Voltage " .. minutes, function(top) return ("%.2f V"):format(top) end, {
        id = "battery-graph-voltage", width = GRAPH_W, height = each, color = VOLTS,
        -- From just under the design voltage to just over the highest
        -- reading: the swing of a charge, not a flat line at 13 V of 15.
        bottom = function()
          local design = battery().voltage_design or 0
          local low = design > 0 and design * 0.92 or 0
          for _, v in ipairs(history(key("voltage"))) do if v > 0 and v < low then low = v * 0.98 end end
          return low
        end,
        top = function()
          local peak = (battery().voltage_design or 0) * 1.05
          for _, v in ipairs(history(key("voltage"))) do if v * 1.02 > peak then peak = v * 1.02 end end
          return math.max(1, peak)
        end,
        first = function() return history(key("voltage")) end,
      }),
      graphs.captioned("Temperature " .. minutes, function(top) return ("%.0f °C"):format(top) end, {
        id = "battery-graph-temperature", width = GRAPH_W, height = each, color = HEAT, top = 60,
        first = function() return history(key("temperature")) end,
      }),
    },
    ui.Column {
      x = stats_x, y = TOP, gap = 28,
      graphs.stats {
        graphs.stat("State", function() return battery().status or "--" end),
        graphs.stat("Charge", function() return num(battery().capacity, "%.0f %%") end, "solid", CHARGE),
        graphs.stat("Power", function() return num(battery().power, "%.2f W") end, "solid", DRAW),
        graphs.stat("Voltage", function() return num(battery().voltage, "%.2f V") end, "solid", VOLTS),
        graphs.stat("Temperature", function() return num(battery().temperature, "%.1f °C") end, "solid", HEAT),
        graphs.stat("Health", function() return num(battery().health, "%.0f %%") end),
        graphs.stat("Time to empty", function()
          local s = battery_state()
          return (battery().status == "Discharging") and duration_text(s.time_left) or "--"
        end),
        graphs.stat("Time to full", function()
          local s = battery_state()
          return (battery().status == "Charging") and duration_text(s.time_to_full) or "--"
        end),
      },
      graphs.facts {
        { "Vendor:", function() return battery().vendor or "" end },
        { "Model:", function() return battery().model or "" end },
        { "Serial:", function() return battery().serial or "" end },
        { "Technology:", function() return battery().technology or "" end },
        { "Cycles:", function() return num(battery().cycles, "%d") end },
        { "Energy now:", function() return num(battery().energy, "%.2f Wh") end },
        { "Last full:", function() return num(battery().energy_full, "%.2f Wh") end },
        { "Full design:", function() return num(battery().energy_design, "%.2f Wh") end },
        { "Charge limit:", function()
          local b = battery()
          if not b.charge_limit then return "--" end
          return b.charge_start and ("%d–%d %%"):format(b.charge_start, b.charge_limit) or ("%d %%"):format(b.charge_limit)
        end },
        { "Charge mode:", function() return battery().charge_mode or "--" end },
      },
    },
  })

  local main = kit.card {
    id = "battery-main", width = MAIN_W, height = M.HEIGHT,
    battery_page,
    kit.text {
      anchors = { center_in = true }, font_size = theme.size.large,
      color = function() return C.onSurfaceVariant end,
      text = "No battery in this machine",
      visible = function() return opened() and battery().name == nil end,
    },
  }
  return ui.Item { id = "dashboard-battery", width = M.WIDTH, height = M.HEIGHT, main }
end

--- Opens the dashboard on this tab.
function M.show()
  local state = require("dashboard_state")
  state.tab:set(M.INDEX)
  require("dashboard").drawer.set(true)
end
M.INDEX = 4

M.page = M.build(require("dashboard_state").context(4))

return M
