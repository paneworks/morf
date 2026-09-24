-- The control centre's weather block: WeatherCard.qml.
--
-- One row: a single line. Two rows: now. Three: the rest of the day below
-- it. Four columns: the two side by side. It watches the reading while it
-- is on the grid, so the centre opened after a while shows a fresh one.

local ui = require("morf.ui")
local theme = require("theme")
local weather = require("services.weather")
local kit = require("components.kit")
local controls = require("components.controls")

local C = theme.color
local M = {}

-- The next four hours, running into tomorrow when today has none left.
M.AHEAD = 4

local function glyph() return weather.available() and weather.glyph() or "󰅤" end
local function degrees() return weather.available() and (weather.temperature() .. "°") or "—" end
local function range() return weather.high() .. "° / " .. weather.low() .. "°" end
local function conditions()
  if not weather.available() then return "No reading" end
  return weather.description() .. "  ·  feels " .. weather.feels_like() .. "°"
end
-- Location and age share the line under the conditions, to save a row.
local function whereabouts()
  local place = weather.place()
  if weather.region() ~= "" then place = place .. ", " .. weather.region() end
  local age = weather.age()
  return "󰍎 " .. place .. (age ~= "" and ("  ·  " .. age) or "")
end

--- The single line of a one-row block.
local function line(width, height)
  local tail = kit.text { visible = weather.available, text = range,
    mono = true, size = theme.size.label, color = C.textMuted }
  local head = ui.Row {
    gap = 10, align = "center",
    kit.glyph { glyph = glyph, size = 22, width = 26, color = C.accent },
    kit.text { text = degrees, size = 20, weight = 600 },
  }
  return ui.Item {
    width = width, height = height,
    ui.Item { anchors = { left = true, vertical_center = true }, width = function() return head.layout_width or 0 end,
      height = 28, head },
    kit.text {
      anchors = { vertical_center = true },
      x = function() return (head.layout_width or 0) + 10 end,
      width = function() return width - (head.layout_width or 0) - (tail.layout_width or 0) - 20 end,
      elide = "right", text = conditions, size = theme.size.small, color = C.textMuted,
    },
    ui.Item { anchors = { right = true, vertical_center = true }, visible = weather.available,
      width = function() return tail.layout_width or 0 end,
      height = 14, tail },
  }
end

--- Now: the sky, the temperature and the day's range, the conditions,
--- the place and how old the reading is.
local function now_block(width)
  local text_w = width - 40 - 12
  local hi_lo = kit.text { visible = weather.available, text = range,
    mono = true, size = theme.size.label, color = C.textMuted }
  return ui.Row {
    gap = 12, align = "start",
    kit.glyph { glyph = glyph, size = 40, width = 40, color = C.accent },
    ui.Column {
      gap = 1, width = text_w,
      ui.Item {
        width = text_w, height = 34,
        kit.text { anchors = { left = true, vertical_center = true }, text = degrees, size = 28, weight = 600 },
        -- Pushed to the far edge, on the figure's baseline.
        ui.Item { anchors = { right = true, bottom = true, bottom_margin = 5 },
          visible = weather.available, width = function() return hi_lo.layout_width or 0 end, height = 12, hi_lo },
      },
      kit.text { width = text_w, elide = "right", text = conditions,
        size = theme.size.small, color = C.textMuted },
      kit.text { width = text_w, elide = "right", text = whereabouts, visible = weather.available,
        size = theme.size.label, color = C.textMuted, opacity = 0.75 },
    },
  }
end

--- The rest of the day: four hours, each a filling column.
local function ahead_block(width)
  local list = function() return weather.hours_ahead(M.AHEAD) end
  local items = { width = width, height = 70 }
  for index = 1, M.AHEAD do
    local block = function() return list()[index] end
    local share = function() return (width - 6 * (math.max(1, #list()) - 1)) / math.max(1, #list()) end
    items[#items + 1] = ui.Item {
      x = function() return (index - 1) * (share() + 6) end,
      width = share, height = 70,
      visible = function() return block() ~= nil end,
      ui.Column {
        anchors = { horizontal_center = true, top = true }, gap = 3, align = "center",
        kit.text { text = function()
            local b = block()
            if not b then return "" end
            return ("%02d:00"):format(b.hour) .. (b.tomorrow and "⁺" or "")
          end, mono = true, size = theme.size.label, color = C.textMuted },
        kit.glyph { glyph = function() local b = block() return b and b.glyph or "" end, size = 16 },
        kit.text { text = function() local b = block() return b and (b.temperature .. "°") or "" end,
          mono = true, size = theme.size.small, weight = 600 },
        -- Only when there is some chance of rain.
        kit.text { visible = function() local b = block() return b ~= nil and (b.rain or 0) >= 20 end,
          text = function() local b = block() return b and ((b.rain or 0) .. "%") or "" end,
          mono = true, size = theme.size.label, color = C.blue },
      },
    }
  end
  return ui.Item(items)
end

--- `cols` x `rows` cells, `width` x `height` pixels.
function M.build(o)
  weather.subscribe()
  local inner_w, inner_h = o.width - 28, o.height - 28
  local one_line = o.rows <= 1
  local beside = o.cols >= 4 and o.rows <= 2
  local show_ahead = not one_line and (o.rows >= 3 or beside)
  local body
  if one_line then
    body = line(inner_w, inner_h)
  elseif beside then
    local half = (inner_w - 16) / 2
    body = ui.Item {
      width = inner_w, height = inner_h,
      ui.Item { x = 0, y = 0, width = half, height = inner_h, now_block(half) },
      ui.Item {
        x = half + 16, y = function() return (inner_h - 70) / 2 end, width = half, height = 70,
        visible = function() return #weather.hours_ahead(M.AHEAD) > 0 end,
        ahead_block(half),
      },
    }
  else
    body = ui.Item {
      width = inner_w, height = inner_h,
      now_block(inner_w),
      -- Stacked, the hours keep to the bottom of the card.
      ui.Item {
        x = 0, y = inner_h - 70 + 8, width = inner_w, height = 70,
        visible = function() return show_ahead and #weather.hours_ahead(M.AHEAD) > 0 end,
        ahead_block(inner_w),
      },
    }
  end
  return controls.card {
    width = o.width, height = o.height,
    on_destroyed = function() weather.release() end,
    body,
  }
end

return M
