-- A drawing of the sky at every size, with the temperature under it on a
-- 2x2. The 4x2 adds the thermometer, the description and the next hours as
-- small skies; the band shows six. The 4x4 plots the coming hours as a
-- temperature curve with the sky at each point.
--
-- Port of analogue/WeatherFace.qml.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local weather = require("services.weather")
local instrument = require("desktop.faces.analogue.instrument")
local sky = require("desktop.faces.analogue.sky")
local thermometer = require("desktop.faces.analogue.thermometer")
local svg = require("desktop.faces.analogue.svg")

local M = {}
local n = svg.n

local function hour_label(b)
  local hour = string.format("%02d", b.hour)
  return b.tomorrow and (hour .. "⁺") or (hour .. "h")
end

local function now_kind()
  return sky.kind_of(weather.code(), weather.is_day(), weather.available())
end

-- The next hours as columns: the hour, a small sky, the temperature.
local function hours(ctx, count)
  return function(w, h)
    local cols = {}
    for i = 1, count do
      local function block() return weather.hours_ahead(count)[i] end
      cols[i] = ui.Item {
        width = w / count, height = h,
        visible = function() return block() ~= nil end,
        ui.Column {
          anchors = { horizontal_center = true, top = true }, gap = 1, align = "center",
          kit.text { mono = true, size = theme.size.label, color = ctx.ink.muted,
            text = function() local b = block() return b and hour_label(b) or "" end },
          sky.build { size = 20, ink = ctx.ink,
            kind = function() local b = block() return b and sky.kind_of(b.code, true, true) or "cloud" end },
          kit.text { size = theme.size.small, color = ctx.ink.text,
            text = function() local b = block() return b and (b.temperature .. "°") or "" end },
        },
      }
    end
    return ui.Row { width = w, height = h, table.unpack(cols) }
  end
end

-- The coming hours as a curve on one scale: a dot and the value at each, the
-- hour and the sky under the baseline.
local function chart(ctx, count)
  return function(w, h)
    local ink = ctx.ink
    local plot_top, plot_bottom = 20, h - 52
    local left, right = 20, w - 20
    local function scale()
      local blocks = weather.hours_ahead(count)
      local lo, hi = math.huge, -math.huge
      for _, b in ipairs(blocks) do lo = math.min(lo, b.temperature) hi = math.max(hi, b.temperature) end
      if #blocks == 0 then lo, hi = 0, 1 end
      return blocks, lo - 2, hi + 2
    end
    local function px(i, total) return left + (right - left) * (i - 1) / math.max(1, total - 1) end
    local function py(t, lo, hi) return plot_bottom - (plot_bottom - plot_top) * (t - lo) / math.max(1, hi - lo) end
    local children = {
      ui.Rect { x = left, y = plot_bottom, width = right - left, height = 1, color = ink.dim },
      ui.Image { width = w, height = h, source = function()
        local blocks, lo, hi = scale()
        local pts = {}
        for i, b in ipairs(blocks) do pts[#pts + 1] = n(px(i, #blocks)) .. "," .. n(py(b.temperature, lo, hi)) end
        local accent = svg.hex(ink.accent())
        local dots = {}
        for i, b in ipairs(blocks) do
          dots[#dots + 1] = string.format('<circle cx="%s" cy="%s" r="4" fill="%s"/>', n(px(i, #blocks)), n(py(b.temperature, lo, hi)), accent)
        end
        return svg.doc(w, h, (#pts > 1 and string.format('<polyline points="%s" fill="none" stroke="%s" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"/>',
          table.concat(pts, " "), accent) or "") .. table.concat(dots))
      end },
    }
    for i = 1, count do
      local function at()
        local blocks, lo, hi = scale()
        local b = blocks[i]
        if not b then return nil end
        return b, px(i, #blocks), py(b.temperature, lo, hi)
      end
      children[#children + 1] = ui.Item {
        x = function() local _, x = at() return (x or 0) - 30 end, y = 0, width = 60, height = h,
        visible = function() return at() ~= nil end,
        kit.text { x = 0, width = 60, horizontal_alignment = "center", size = theme.size.small, weight = 600, color = ink.text,
          y = function() local _, _, y = at() return (y or 0) - 24 end,
          text = function() local b = at() return b and (b.temperature .. "°") or "" end },
        kit.text { mono = true, x = 0, y = plot_bottom + 8, width = 60, horizontal_alignment = "center",
          size = theme.size.label, color = ink.muted,
          text = function() local b = at() return b and hour_label(b) or "" end },
        sky.build { x = 19, y = plot_bottom + 22, size = 22, ink = ink, quiet = true,
          kind = function() local b = at() return b and sky.kind_of(b.code, true, true) or "cloud" end },
      }
    end
    return ui.Item { width = w, height = h, table.unpack(children) }
  end
end

function M.build(ctx)
  local square = ctx.family == "2x2"
  local large = ctx.family == "4x4"
  local band = ctx.family == "8x2"
  local ink = ctx.ink
  return instrument.build(ctx, {
    line = function()
      if not weather.available() then return "No reading" end
      local p = weather.place()
      return weather.temperature() .. "°" .. (p ~= "" and (" · " .. p) or "")
    end,
    reading = function() return weather.available() and (weather.temperature() .. "°") or "—" end,
    note = function()
      if not weather.available() then return "no reading yet" end
      local range = weather.high() .. "° / " .. weather.low() .. "°"
      local p = weather.place()
      local d = weather.description()
      local parts = {}
      if p ~= "" then parts[#parts + 1] = p end
      if d ~= "" then parts[#parts + 1] = d end
      parts[#parts + 1] = range
      return table.concat(parts, " · ")
    end,
    filled = true,
    object = function(w, h)
      local s = square and math.min(w, h) or math.floor(h * 0.66)
      local children = {
        sky.build { x = square and (w - s) / 2 or 0, y = (h - s) / 2, size = s, ink = ink,
          kind = now_kind, quiet = function() return not weather.available() end },
      }
      if not square then
        local ts = math.floor(h * 0.92)
        children[#children + 1] = thermometer.build { x = w - thermometer.WIDTH, y = (h - ts) / 2, size = ts, ink = ink,
          -- -10 to 45 degrees, a wall thermometer's range.
          fraction = function() return (weather.temperature() + 10) / 55 end }
      end
      return ui.Item { width = w, height = h, table.unpack(children) }
    end,
    extra = not large and hours(ctx, band and 6 or 3) or nil,
    body = large and chart(ctx, 5) or nil,
  })
end

return M
