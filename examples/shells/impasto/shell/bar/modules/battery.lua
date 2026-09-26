-- Battery: the ring shows the charge; the detail adds what UPower reports
-- beyond it -- time remaining, charge direction, power draw, cell health.
--
-- Port of BatteryModule.qml and BatteryWidget.qml.

local theme = require("theme")
local modules = require("services.modules")
local battery = require("services.battery")
local controls = require("components.controls")
local detail = require("bar.modules.detail")

local C = theme.color
local M = {}

--- BatteryWidget: the outline is the charge, the glyph says what it is.
function M.widget(size)
  return controls.ring_glyph {
    size = size, thickness = 2.5,
    progress = function() return battery.percent() / 100 end,
    track_color = C.indicatorDim,
    fill_color = battery.tint,
    glyph = battery.icon, glyph_size = math.floor(size * 0.38 + 0.5),
    glyph_color = C.indicator,
  }
end

modules.define("battery", {
  glyph = battery.icon,
  value = function() return battery.percent() .. "%" end,
  has = battery.available,
  tint = function()
    if battery.available() and not battery.charging() and not battery.full() then
      if battery.percent() <= 10 then return C.indicatorBad end
      if battery.percent() <= 20 then return C.indicatorWarn end
    end
    return C.text()
  end,
  chip = function() return M.widget(theme.capsule_height()) end,
  detail = function()
    return detail.card("battery", {
      mark = M.widget(44),
      title = function() return battery.percent() .. "%" end,
      -- The estimate is blank for a minute or two after the cable changes.
      subtitle = function()
        local estimate = battery.estimate()
        if estimate ~= "" then return battery.state_word() .. "  ·  " .. estimate end
        return battery.state_word()
      end,
      figures = {
        { label = "POWER",
          value = function()
            local watts = battery.watts()
            return watts > 0 and string.format("%.1f W", watts) or "—"
          end,
          note = function() return battery.charging() and "going in" or "coming out" end },
        { label = function() return battery.health_known() and "HEALTH" or "CHARGE" end,
          value = function()
            if battery.health_known() then return battery.health() .. "%" end
            return string.format("%.1f Wh", battery.energy())
          end,
          note = function()
            local full = battery.energy_capacity()
            return full > 0 and string.format("of %.1f Wh full", full) or ""
          end },
      },
    })
  end,
})

return M
