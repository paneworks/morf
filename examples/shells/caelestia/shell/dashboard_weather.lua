-- The dashboard's Weather tab: the place and the date, sunrise and sunset;
-- a big card with the condition and the temperature; humidity, feels-like
-- and wind; and the week ahead.
--
-- Measured off the reference at 1920x1080: the page 838 x 560; the big card
-- 178 tall from y = 84; the three small cards 60 tall, 12 under it; the
-- forecast title at 371 and its seven cards 165 tall from 394.

local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local services = require("services")

local C = theme.color
local M = {}

M.WIDTH, M.HEIGHT = 838, 561
local GAP = 12

--- "Reykjavik" from "Reykjavik, Iceland".
function M.place_name(place)
  place = tostring(place or "")
  return place:match("^([^,]+)") or place
end

--- A clock time as the reference writes it: "7:18 AM".
function M.clock(t)
  if not t then return "--" end
  local text = morf.time.format("%I:%M %p", t)
  return (text:gsub("^0", ""))
end

function M.build(ctx)
  local function now()
    if not ctx.opened() then return { available = false } end
    return services.weather()
  end
  local function temperature(v, bare)
    if v == nil then return "--" end
    local w = now()
    local unit = (w.units and w.units.temperature) or "°"
    return ("%d%s"):format(math.floor(v + 0.5), bare and "°" or unit)
  end
  local function today() return (now().daily or {})[1] or {} end

  local function sun(icon, label, value, id)
    return ui.Row {
      gap = 10, align = "center",
      kit.icon(icon, 40, function() return C.tertiary end),
      ui.Column {
        gap = 0,
        kit.text { text = label, font_size = theme.size.normal - 1, color = function() return C.onSurfaceVariant end },
        kit.text { id = id, text = value, font_size = theme.size.normal, font_weight = 600 },
      },
    }
  end

  local header = ui.Item {
    width = M.WIDTH, height = 72,
    ui.Column {
      x = 16, y = 4, gap = 2,
      kit.text {
        id = "weather-place",
        text = function()
          local w = now()
          return w.available and M.place_name(w.place) or "Weather"
        end,
        font_size = 36, font_weight = 600,
      },
      kit.text {
        id = "weather-date",
        text = function() morf.minute_clock:get() return morf.time.format("%A, %B %-d") end,
        font_size = theme.size.normal,
        color = function() return C.onSurfaceVariant end,
      },
    },
    ui.Row {
      anchors = { right = true, right_margin = 15 }, y = 12, gap = 22, align = "center",
      sun("wb_twilight", "Sunrise", function() return M.clock(today().sunrise) end, "weather-sunrise"),
      sun("bedtime", "Sunset", function() return M.clock(today().sunset) end, "weather-sunset"),
    },
  }

  local big = kit.card {
    id = "weather-now",
    y = 84, width = M.WIDTH, height = 178, radius = 60,
    ui.Row {
      anchors = { center_in = true }, gap = 24, align = "center",
      ui.Item {
        width = 136, height = 136,
        kit.icon(function()
          local w = now()
          return w.available and services.weather_symbol(w.code, w.is_day) or "cloud"
        end, 136, function() return C.primary end, {
          anchors = { center_in = true },
          visible = function() return now().available end,
        }),
        kit.loading(96, function() return C.primary end, {
          id = "weather-loading",
          anchors = { center_in = true },
          active = function() return ctx.opened() and not now().available end,
          visible = function() return not now().available end,
        }),
      },
      ui.Column {
        gap = 0,
        kit.text {
          id = "weather-temperature",
          text = function() local w = now() return w.available and temperature(w.temperature) or "--" end,
          font_size = 72, font_weight = 500, line_height = "80px",
          color = function() return C.primary end,
        },
        kit.text {
          id = "weather-condition",
          x = 4,
          text = function() local w = now() return w.available and (w.condition or "") or "No weather" end,
          font_size = theme.size.larger + 1,
        },
      },
    },
  }

  local function detail(icon, color, label, value, id)
    return kit.card {
      id = id,
      width = (M.WIDTH - 2 * GAP) / 3, height = 60, radius = 12,
      ui.Row {
        anchors = { center_in = true }, gap = 14, align = "center",
        kit.icon(icon, 28, color),
        ui.Column {
          gap = 0,
          kit.text { text = label, font_size = theme.size.normal - 1, color = function() return C.onSurfaceVariant end },
          kit.text { text = value, font_size = theme.size.normal, font_weight = 600 },
        },
      },
    }
  end
  local details = ui.Row {
    y = 84 + 178 + 13, gap = GAP,
    detail("humidity_percentage", function() return C.onSurface end, "Humidity", function()
      local w = now()
      return w.humidity and ("%d%%"):format(math.floor(w.humidity + 0.5)) or "--"
    end, "weather-humidity"),
    detail("thermostat", function() return C.primary end, "Feels like", function()
      return temperature(now().feels_like)
    end, "weather-feels"),
    detail("air", function() return C.tertiary end, "Wind", function()
      local w = now()
      if not w.wind_speed then return "--" end
      return ("%.1f %s"):format(w.wind_speed, (w.units and w.units.wind) or "km/h")
    end, "weather-wind"),
  }

  local CARD_W = (M.WIDTH - 6 * GAP) / 7
  local days = {}
  for i = 1, 7 do
    local function day() return (now().daily or {})[i] end
    days[#days + 1] = kit.card {
      id = "weather-day-" .. i,
      width = CARD_W, height = 165, radius = 14,
      -- wttr.in, the fallback, knows three days.
      visible = function() return i == 1 or day() ~= nil end,
      ui.Column {
        anchors = { horizontal_center = true }, y = 12, gap = 0, align = "center",
        kit.text {
          text = function()
            local d = day()
            if i == 1 then return "Today" end
            return d and d.time and morf.time.format("%a", d.time) or "--"
          end,
          font_size = theme.size.large - 2, font_weight = 600,
          color = function() return C.primary end,
        },
        kit.text {
          text = function()
            local d = day()
            return d and d.time and morf.time.format("%b %-d", d.time) or ""
          end,
          font_size = theme.size.normal,
          color = function() return C.onSurfaceVariant end,
        },
        ui.Item { width = 1, height = 12 },
        kit.icon(function()
          local d = day()
          return services.weather_symbol(d and d.code, true)
        end, 40, function() return C.primary end),
        ui.Item { width = 1, height = 14 },
        kit.text {
          text = function()
            local d = day()
            if not d then return "--" end
            return temperature(d.low, true) .. " / " .. temperature(d.high, true)
          end,
          font_size = theme.size.normal, font_weight = 600,
          color = function() return C.tertiary end,
        },
      },
    }
  end

  return ui.Item {
    id = "dashboard-weather-tab",
    width = M.WIDTH, height = M.HEIGHT,
    header,
    big,
    details,
    kit.text {
      x = 11, y = 360, text = "7-day forecast",
      font_size = theme.size.larger, font_weight = 600,
    },
    ui.Row { y = 394, gap = GAP, table.unpack(days) },
  }
end

-- Built as the module loads, with its own instruction budget.
M.page = M.build(require("dashboard_state").context(5))

return M
