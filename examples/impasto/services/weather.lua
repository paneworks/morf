-- The weather, for the desk's faces and anything else that shows it.
--
-- Port of WeatherService.qml over `lib.weather` (Open-Meteo, with wttr.in
-- behind it). The library lands in lua-stdlib separately; until it is there
-- this reads as "no forecast" and every face says so. The place is the
-- `weatherPlace` setting, or wherever wttr.in guesses from the address when
-- it is empty. Nothing is fetched until a face reads it.

local settings = require("services.settings")

local M = {}

local ok, lib = pcall(require, "lib.weather")
if not ok then lib = nil end

local source_handle, source_place = nil, nil
-- Pieces watching the reading (see `subscribe`).
local watchers = 0
local function source()
  if not lib then return nil end
  local place = settings.weatherPlace
  if source_handle == nil or source_place ~= place then
    -- A new place is a new reading; the watchers move to it.
    if source_handle and source_handle.source then source_handle.source:pin(false) end
    source_place = place
    local okn, made = pcall(lib.new, { location = place ~= "" and place or nil, units = "metric" })
    source_handle = okn and made or false
    if source_handle and watchers > 0 then source_handle.source:pin(true) end
  end
  return source_handle or nil
end

local EMPTY = { available = false, hourly = {}, daily = {} }

--- The reading: `{ available, place, temperature, feels_like, condition,
--- code, is_day, high, low, hourly, daily, ... }`. A binding follows it.
function M.now()
  local s = source()
  if not s then return EMPTY end
  local okr, value = pcall(function() return s:get() end)
  if not okr or type(value) ~= "table" then return EMPTY end
  return value
end

local function round(v) return tonumber(v) and math.floor(v + 0.5) or 0 end

-- WMO codes to the Material glyphs the rest of the shell draws in.
local GLYPHS = {
  clear = { "󰖙", "󰖔" }, mostly_clear = { "󰖕", "󰼱" }, partly_cloudy = { "󰖕", "󰼱" },
  overcast = { "󰖐", "󰖐" }, fog = { "󰖑", "󰖑" }, drizzle = { "󰖗", "󰖗" },
  freezing = { "󰙿", "󰙿" }, rain = { "󰖖", "󰖖" }, showers = { "󰖗", "󰖗" },
  snow = { "󰖘", "󰖘" }, storm = { "󰖓", "󰖓" },
}

--- The kind of sky a WMO code is.
function M.kind(code)
  if code == nil then return "overcast" end
  if code == 0 then return "clear" elseif code == 1 then return "mostly_clear"
  elseif code == 2 then return "partly_cloudy" elseif code == 3 then return "overcast"
  elseif code == 45 or code == 48 then return "fog"
  elseif code >= 51 and code <= 55 then return "drizzle"
  elseif code == 56 or code == 57 or code == 66 or code == 67 then return "freezing"
  elseif code >= 61 and code <= 65 then return "rain"
  elseif (code >= 71 and code <= 77) or code == 85 or code == 86 then return "snow"
  elseif code >= 80 and code <= 82 then return "showers"
  elseif code >= 95 then return "storm" end
  return "overcast"
end

--- A Material glyph for a WMO code, by day or night.
function M.glyph_for(code, is_day)
  local pair = GLYPHS[M.kind(code)]
  return is_day == false and pair[2] or pair[1]
end

function M.available() return M.now().available == true end
function M.place()
  if settings.weatherPlace ~= "" then return settings.weatherPlace end
  return M.now().place or ""
end
function M.temperature() return round(M.now().temperature) end
function M.feels_like() local n = M.now() return round(n.feels_like or n.temperature) end
function M.high() return round(M.now().high) end
function M.low() return round(M.now().low) end
function M.description() return M.now().condition or "" end
function M.code() return M.now().code end
function M.is_day() return M.now().is_day ~= false end
function M.glyph() local n = M.now() return M.glyph_for(n.code, n.is_day) end

--- How old the reading is: "just now", "12 min ago", "2 h ago", or "" with
--- none (WeatherService.qml:54-63).
function M.age()
  local n = M.now()
  if not n.available then return "" end
  local at = tonumber(n.updated) or 0
  if at <= 0 then return "" end
  morf.clock:get()
  local minutes = math.floor((morf.time.now() - at) / 60)
  if minutes < 2 then return "just now" end
  if minutes < 60 then return minutes .. " min ago" end
  return math.floor(minutes / 60 + 0.5) .. " h ago"
end

--- The region under the place, when the source names one.
function M.region() return M.now().region or "" end

-- Watchers: a chip on the bar, the detail, the control centre's card. The
-- source polls while anything reads it; a watcher also keeps it polling
-- with nothing drawn, and asks again when the reading is missing or more
-- than ten minutes old (WeatherService.qml:75-85).
function M.subscribe()
  watchers = watchers + 1
  local s = source()
  if not s or not s.source then return end
  if watchers == 1 and s.source.pin then s.source:pin(true) end
  local n = M.now()
  local at = tonumber(n.updated) or 0
  if not n.available or morf.time.now() - at > 600 then
    morf.timer(1, function() s.source:refresh() end, false)
  end
end
function M.release()
  watchers = math.max(0, watchers - 1)
  local s = source()
  if watchers == 0 and s and s.source and s.source.pin then s.source:pin(false) end
end

--- The next hours after this one: `{ hour, tomorrow, glyph, temperature,
--- code, is_day, rain }`; `is_day` is false for an hour after dark, and
--- `rain` the chance of it in percent.
function M.hours_ahead(count)
  local now = M.now()
  if not now.available then return {} end
  morf.clock:get()
  local today = morf.time.date()
  local out = {}
  for _, block in ipairs(now.hourly or {}) do
    if #out >= count then break end
    local at = block.time and morf.time.date(block.time) or nil
    if at and (at.day ~= today.day or at.hour > today.hour) then
      out[#out + 1] = {
        hour = at.hour, tomorrow = at.day ~= today.day,
        glyph = M.glyph_for(block.code, block.is_day), code = block.code,
        is_day = block.is_day ~= false,
        rain = round(block.precipitation),
        temperature = round(block.temperature),
      }
    end
  end
  return out
end

--- The days to come: `{ weekday, glyph, high, low, code }`.
function M.days_ahead(count)
  local now = M.now()
  if not now.available then return {} end
  local out = {}
  for _, day in ipairs(now.daily or {}) do
    if #out >= count then break end
    out[#out + 1] = {
      weekday = day.time and morf.time.format("%a", day.time) or "",
      glyph = M.glyph_for(day.code, true), code = day.code,
      high = round(day.high), low = round(day.low),
    }
  end
  return out
end

return M
