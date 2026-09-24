-- 8x2 faces: a strip. The same grid as the other families, with the
-- reading set large. Suits the bare style best.
--
-- Port of faces/Bands.qml.

local theme = require("theme")
local common = require("desktop.faces.common")
local wides = require("desktop.faces.wides")
local S = require("desktop.sources")
local weather = require("services.weather")

local face = common.widget_face
local glyph = common.glyph
local M = {}

function M.clock(ctx)
  return face(ctx, {
    label = function() return S.clock.format("%A") end,
    reading = function() return S.clock.format(S.clock.pattern()) end,
    note = function() return S.clock.format("%-d %B %Y") end,
    reading_size = theme.size.display,
    mark = glyph { glyph = "󰥔", size = 30, color = ctx.ink.text },
  })
end

function M.weather(ctx)
  return face(ctx, {
    label = function() local p = weather.place() return p ~= "" and p or "Weather" end,
    reading = function() return weather.available() and (weather.temperature() .. "°") or "--°" end,
    note = function()
      if not weather.available() then return "no forecast" end
      local d = weather.description()
      local rest = "feels " .. weather.feels_like() .. "° · " .. weather.high() .. "° / " .. weather.low() .. "°"
      return d ~= "" and (d .. " · " .. rest) or rest
    end,
    reading_size = theme.size.display,
    extra_share = 0.55,
    mark = glyph { glyph = function() return weather.available() and weather.glyph() or "󰅤" end, size = 34, color = ctx.ink.text },
    extra = wides.hour_columns(ctx, 6, 24, true, 3),
  })
end

function M.notes(ctx) return require("desktop.faces.note").build(ctx) end
function M.photo(ctx) return require("desktop.faces.photo").build(ctx) end
function M.github(ctx) return require("desktop.faces.github").build(ctx) end
function M.spectrum(ctx) return require("desktop.faces.spectrum").build(ctx) end

return M
