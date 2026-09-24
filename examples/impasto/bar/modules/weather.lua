-- Weather: the sky's glyph and the temperature on a chip; the detail is the
-- place, the conditions and the next six hours.
--
-- Port of WeatherModule.qml and the weather rows of ModuleService.qml
-- (glyphOf 184, valueOf 212, has 424). Nothing is fetched until something
-- reads it; a chip on the bar and an open detail keep it polling
-- (`weather.subscribe`).

local ui = require("morf.ui")
local theme = require("theme")
local modules = require("services.modules")
local weather = require("services.weather")
local kit = require("components.kit")

local C = theme.color
local M = {}

-- Six hours rather than the card's four: this detail is wider.
M.HOURS = 6

local function hour_label(block)
  return ("%02d:00"):format(block.hour) .. (block.tomorrow and "⁺" or "")
end

--- One hour: the time, the sky and the temperature, centred in its share
--- of `width`. Columns divide the width by hand, so fewer hours still
--- spread across it.
local function hour_column(index, width, height)
  local block = function() return weather.hours_ahead(M.HOURS)[index] end
  local share = function() return width / math.max(1, #weather.hours_ahead(M.HOURS)) end
  return ui.Item {
    x = function() return (index - 1) * share() end,
    width = share, height = height,
    visible = function() return block() ~= nil end,
    ui.Column {
      anchors = { center_in = true }, gap = 1, align = "center",
      kit.text { text = function() local b = block() return b and hour_label(b) or "" end,
        mono = true, size = theme.size.label, color = C.textMuted },
      kit.glyph { glyph = function() local b = block() return b and b.glyph or "" end,
        size = 15, color = C.indicator },
      kit.text { text = function() local b = block() return b and (b.temperature .. "°") or "" end,
        mono = true, size = theme.size.label },
    },
  }
end

function M.detail()
  weather.subscribe()
  local w, h = modules.open_size("weather")
  local inner_w = w - 8 - 28
  local text_w = inner_w - 30 - 13
  local temperature = kit.text {
    text = function() return weather.temperature() .. "°" end,
    mono = true, size = theme.size.medium, weight = 600,
  }
  local hours_h = (h - 8) - 12 - 10 - 44 - 8
  local columns = { width = inner_w, height = hours_h }
  for index = 1, M.HOURS do columns[#columns + 1] = hour_column(index, inner_w, hours_h) end
  return ui.Item {
    anchors = { fill = true },
    on_destroyed = function() weather.release() end,
    ui.Column {
      anchors = { left = true, top = true, left_margin = 14, top_margin = 12 },
      gap = 8,
      ui.Row {
        gap = 13, align = "center", height = 44,
        kit.glyph { glyph = function() return weather.available() and weather.glyph() or "󰅤" end,
          size = 30, width = 30, color = C.indicator },
        ui.Column {
          gap = 2, width = text_w,
          ui.Item {
            width = text_w, height = 20,
            kit.text {
              anchors = { left = true, vertical_center = true },
              width = function() return text_w - (temperature.layout_width or 0) - 10 end,
              elide = "right",
              text = function()
                if weather.available() then return weather.place() end
                return "No reading"
              end,
              size = theme.size.medium, weight = 600,
            },
            ui.Item { anchors = { right = true, vertical_center = true },
              width = function() return temperature.layout_width or 0 end, height = 20,
              visible = weather.available,
              ui.Item { anchors = { vertical_center = true }, height = 20, temperature } },
          },
          kit.text {
            width = text_w, elide = "right", size = theme.size.small, color = C.textMuted,
            text = function()
              if not weather.available() then return "Nothing fetched yet" end
              local parts = {}
              if weather.description() ~= "" then parts[#parts + 1] = weather.description() end
              parts[#parts + 1] = "feels " .. weather.feels_like() .. "°"
              if weather.age() ~= "" then parts[#parts + 1] = weather.age() end
              return table.concat(parts, " · ")
            end,
          },
        },
      },
      ui.Item {
        width = inner_w, height = hours_h,
        visible = function() return #weather.hours_ahead(M.HOURS) > 0 end,
        ui.Item(columns),
      },
    },
  }
end

modules.define("weather", {
  glyph = function()
    local g = weather.available() and weather.glyph() or ""
    return g ~= "" and g or "󰖐"
  end,
  value = function() return weather.available() and (weather.temperature() .. "°") or "--°" end,
  -- Reading it builds the source, which runs its first fetch; it turns true
  -- a moment later.
  has = weather.available,
  watch = function(on) if on then weather.subscribe() else weather.release() end end,
  detail = M.detail,
})

return M
