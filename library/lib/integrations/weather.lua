-- The weather, from keyless web APIs.
--
-- Open-Meteo answers a latitude and longitude with the current conditions and
-- a forecast, and turns a place name into coordinates, without an account or
-- a key; wttr.in does the same from a name, or from the asker's address when
-- given none, and is the fallback when Open-Meteo is down. Both are asked
-- through `morf.http`, so nothing waits on the network.
--
--   local weather = require("lib.integrations.weather")
--   local here = weather.new { location = "Wageningen", units = "metric" }
--   ui.Text { text = function()
--     local now = here:get()
--     return now.available and ("%s %d%s"):format(now.glyph, now.temperature, now.units.temperature) or ""
--   end }
--
-- An answer is cached on disk with the time it came, so a shell that restarts
-- does not ask again within `ttl`, and a machine that is offline shows the
-- last forecast it had (marked `stale`) rather than nothing.

local morf = require("morf")
local poll = require("lib.util.poll")

local weather = {}

-- ---------------------------------------------------------------------------
-- Conditions: WMO weather codes (Open-Meteo's) to words, icon names and glyphs.

-- One row per group of codes: text, freedesktop icon (day), (night), glyph
-- (day), (night). The icon names are the ones every icon theme ships.
local GROUPS = {
  clear = { "Clear", "weather-clear", "weather-clear-night", "☀", "🌙" },
  mostly_clear = { "Mainly clear", "weather-few-clouds", "weather-few-clouds-night", "🌤", "🌙" },
  partly_cloudy = { "Partly cloudy", "weather-few-clouds", "weather-few-clouds-night", "⛅", "☁" },
  overcast = { "Overcast", "weather-overcast", "weather-overcast", "☁", "☁" },
  fog = { "Fog", "weather-fog", "weather-fog", "🌫", "🌫" },
  drizzle = { "Drizzle", "weather-showers-scattered", "weather-showers-scattered", "🌦", "🌧" },
  freezing = { "Freezing rain", "weather-freezing-rain", "weather-freezing-rain", "🌧", "🌧" },
  rain = { "Rain", "weather-showers", "weather-showers", "🌧", "🌧" },
  showers = { "Rain showers", "weather-showers-scattered", "weather-showers-scattered", "🌦", "🌧" },
  snow = { "Snow", "weather-snow", "weather-snow", "🌨", "🌨" },
  storm = { "Thunderstorm", "weather-storm", "weather-storm", "⛈", "⛈" },
  unknown = { "Unknown", "weather-severe-alert", "weather-severe-alert", "?", "?" },
}

local WMO = {
  [0] = { "clear" }, [1] = { "mostly_clear" }, [2] = { "partly_cloudy" }, [3] = { "overcast" },
  [45] = { "fog" }, [48] = { "fog", "Rime fog" },
  [51] = { "drizzle", "Light drizzle" }, [53] = { "drizzle" }, [55] = { "drizzle", "Dense drizzle" },
  [56] = { "freezing", "Freezing drizzle" }, [57] = { "freezing", "Freezing drizzle" },
  [61] = { "rain", "Light rain" }, [63] = { "rain" }, [65] = { "rain", "Heavy rain" },
  [66] = { "freezing" }, [67] = { "freezing", "Heavy freezing rain" },
  [71] = { "snow", "Light snow" }, [73] = { "snow" }, [75] = { "snow", "Heavy snow" },
  [77] = { "snow", "Snow grains" },
  [80] = { "showers", "Light showers" }, [81] = { "showers" }, [82] = { "showers", "Violent showers" },
  [85] = { "snow", "Snow showers" }, [86] = { "snow", "Heavy snow showers" },
  [95] = { "storm" }, [96] = { "storm", "Thunderstorm with hail" }, [99] = { "storm", "Thunderstorm with hail" },
}

--- `{ code, text, icon, glyph }` for a WMO code, by day or night.
function weather.condition(code, is_day)
  local entry = WMO[code] or { "unknown" }
  local group = GROUPS[entry[1]]
  local night = is_day == false
  return {
    code = code,
    kind = entry[1],
    text = entry[2] or group[1],
    icon = night and group[3] or group[2],
    glyph = night and group[5] or group[4],
  }
end

-- World Weather Online's codes, which wttr.in passes on, to the nearest WMO.
local WWO = {
  [113] = 0, [116] = 2, [119] = 3, [122] = 3, [143] = 45, [248] = 45, [260] = 48,
  [176] = 80, [263] = 51, [266] = 53, [281] = 56, [284] = 57, [293] = 61, [296] = 61,
  [299] = 63, [302] = 63, [305] = 65, [308] = 65, [311] = 66, [314] = 67, [317] = 66,
  [320] = 66, [350] = 77, [353] = 80, [356] = 81, [359] = 82, [362] = 66, [365] = 67,
  [374] = 66, [377] = 67, [179] = 71, [182] = 66, [185] = 56, [227] = 73, [230] = 75,
  [323] = 71, [326] = 71, [329] = 73, [332] = 73, [335] = 75, [338] = 75, [368] = 85,
  [371] = 86, [200] = 95, [386] = 95, [389] = 95, [392] = 96, [395] = 96,
}

-- ---------------------------------------------------------------------------
-- Parsing

local UNITS = {
  metric = { temperature = "°C", wind = "km/h" },
  imperial = { temperature = "°F", wind = "mph" },
}

local function num(value)
  if value == morf.json.null then return nil end
  return tonumber(value)
end

local function with_condition(entry, code, is_day)
  local condition = weather.condition(code, is_day)
  entry.code, entry.condition, entry.icon, entry.glyph = condition.code, condition.text, condition.icon, condition.glyph
  return entry
end

--- Reads an Open-Meteo forecast (asked for with `timeformat=unixtime`).
function weather.parse_open_meteo(data, now)
  now = now or morf.time.now()
  local current = data.current or {}
  local is_day = current.is_day ~= 0
  local out = with_condition({
    available = true,
    source = "open-meteo",
    temperature = num(current.temperature_2m),
    feels_like = num(current.apparent_temperature),
    humidity = num(current.relative_humidity_2m),
    wind_speed = num(current.wind_speed_10m),
    wind_direction = num(current.wind_direction_10m),
    is_day = is_day,
    time = num(current.time),
    timezone = data.timezone,
    hourly = {},
    daily = {},
  }, num(current.weather_code), is_day)

  local hourly = data.hourly or {}
  local times = hourly.time or {}
  for index = 1, #times do
    local at = num(times[index])
    -- The hour in progress and the twenty-three after it.
    if at and at + 3600 > now and #out.hourly < 24 then
      local day = hourly.is_day and hourly.is_day[index] ~= 0
      out.hourly[#out.hourly + 1] = with_condition({
        time = at,
        temperature = num(hourly.temperature_2m and hourly.temperature_2m[index]),
        precipitation = num(hourly.precipitation_probability and hourly.precipitation_probability[index]),
        is_day = day,
      }, num(hourly.weather_code and hourly.weather_code[index]), day)
    end
  end

  local daily = data.daily or {}
  for index = 1, #(daily.time or {}) do
    out.daily[#out.daily + 1] = with_condition({
      time = num(daily.time[index]),
      high = num(daily.temperature_2m_max and daily.temperature_2m_max[index]),
      low = num(daily.temperature_2m_min and daily.temperature_2m_min[index]),
      precipitation = num(daily.precipitation_probability_max and daily.precipitation_probability_max[index]),
      sunrise = num(daily.sunrise and daily.sunrise[index]),
      sunset = num(daily.sunset and daily.sunset[index]),
    }, num(daily.weather_code and daily.weather_code[index]), true)
  end
  if out.daily[1] then out.high, out.low = out.daily[1].high, out.daily[1].low end
  return out
end

local COMPASS = { N = 0, NNE = 22.5, NE = 45, ENE = 67.5, E = 90, ESE = 112.5, SE = 135, SSE = 157.5,
  S = 180, SSW = 202.5, SW = 225, WSW = 247.5, W = 270, WNW = 292.5, NW = 315, NNW = 337.5 }

--- Reads a wttr.in `format=j1` answer. Its hours are three apart and its
--- days three long; that is what it has.
function weather.parse_wttr(data, units, now)
  now = now or morf.time.now()
  local imperial = units == "imperial"
  local current = (data.current_condition or {})[1] or {}
  local temp = function(entry, c, f) return num(entry[imperial and f or c]) end
  local area = (data.nearest_area or {})[1] or {}
  local place = area.areaName and area.areaName[1] and area.areaName[1].value
  local country = area.country and area.country[1] and area.country[1].value
  local hour = tonumber(morf.time.format("%H", now))
  local is_day = hour >= 7 and hour < 19
  local out = with_condition({
    available = true,
    source = "wttr.in",
    temperature = temp(current, "temp_C", "temp_F"),
    feels_like = temp(current, "FeelsLikeC", "FeelsLikeF"),
    humidity = num(current.humidity),
    wind_speed = temp(current, "windspeedKmph", "windspeedMiles"),
    wind_direction = num(current.winddirDegree) or COMPASS[current.winddir16Point],
    is_day = is_day,
    place = place and (country and (place .. ", " .. country) or place) or nil,
    -- The two halves of `place`, for a caption with room for one.
    city = place, region = country,
    hourly = {},
    daily = {},
  }, WWO[num(current.weatherCode)], is_day)
  local description = current.weatherDesc and current.weatherDesc[1] and current.weatherDesc[1].value
  if description and description ~= "" then out.condition = description:match("^%s*(.-)%s*$") end

  for _, day in ipairs(data.weather or {}) do
    local midnight = morf.time.parse(day.date or "")
    out.daily[#out.daily + 1] = with_condition({
      time = midnight,
      high = temp(day, "maxtempC", "maxtempF"),
      low = temp(day, "mintempC", "mintempF"),
    }, WWO[num(day.hourly and day.hourly[5] and day.hourly[5].weatherCode)], true)
    for _, slot in ipairs(day.hourly or {}) do
      local at = midnight and midnight + (num(slot.time) or 0) / 100 * 3600
      if at and at + 3 * 3600 > now and #out.hourly < 8 then
        local slot_day = (num(slot.time) or 0) >= 600 and (num(slot.time) or 0) < 1900
        out.hourly[#out.hourly + 1] = with_condition({
          time = at,
          temperature = temp(slot, "tempC", "tempF"),
          precipitation = num(slot.chanceofrain),
          is_day = slot_day,
        }, WWO[num(slot.weatherCode)], slot_day)
      end
    end
  end
  if out.daily[1] then out.high, out.low = out.daily[1].high, out.daily[1].low end
  return out
end

-- ---------------------------------------------------------------------------
-- A place's weather

local Weather = {}
Weather.__index = Weather

--- Options:
---   location       -- a place name, geocoded once; or
---   latitude, longitude, name  -- coordinates and what to call them;
---                     with neither, wttr.in guesses from the address
---   units          -- "metric" (default) or "imperial"
---   interval       -- ms between refreshes while read (default 30 min)
---   ttl            -- seconds an answer is fresh (default 30 min)
---   cache_dir      -- where answers are kept (default poll.cache_dir())
---   geocoding_url, forecast_url, wttr_url -- the services, for tests and
---                     mirrors
---   fallback       -- ask wttr.in when Open-Meteo fails (default true)
function weather.new(options)
  options = options or {}
  local self = setmetatable({
    location = options.location,
    latitude = options.latitude,
    longitude = options.longitude,
    name = options.name,
    units = options.units == "imperial" and "imperial" or "metric",
    ttl = options.ttl or 1800,
    cache_dir = options.cache_dir,
    geocoding_url = options.geocoding_url or "https://geocoding-api.open-meteo.com/v1/search",
    forecast_url = options.forecast_url or "https://api.open-meteo.com/v1/forecast",
    wttr_url = options.wttr_url or "https://wttr.in",
    fallback = options.fallback ~= false,
  }, Weather)
  -- The cache is keyed by what was asked, not by the coordinates a name
  -- turned into, so a restart finds it before geocoding again.
  self._key = table.concat({ self.location or "", tostring(self.latitude), tostring(self.longitude), self.units }, "|")
  self.source = poll.source {
    name = "weather",
    interval = options.interval or 30 * 60 * 1000,
    linger = 1,
    initial = { available = false, hourly = {}, daily = {}, units = UNITS[self.units] },
    sample = function(done) self:_fetch(done) end,
  }
  return self
end

--- The weather, read so a binding follows it. Before the first answer,
--- `{ available = false }`. Fields: available, source, place, temperature,
--- feels_like, humidity, wind_speed, wind_direction, code, condition, icon,
--- glyph, is_day, high, low, hourly (next 24 h), daily (7 days), units, stale.
function Weather:get() return self.source:get() end

--- Asks again now, past the cache.
function Weather:refresh()
  self._force = true
  self.source:refresh()
end

function Weather:_cache_path()
  local dir = self.cache_dir or poll.cache_dir()
  return dir .. "/weather-" .. morf.encoding.sha1(self._key):sub(1, 16) .. ".json"
end

function Weather:_finish(done, value)
  value.units = UNITS[self.units]
  value.updated = morf.time.now()
  value.place = value.place or self.place_name or self.name or self.location
  value.region = value.region or self.region_name
  poll.cache_write(self:_cache_path(), value)
  done(value)
end

function Weather:_wttr(done, why)
  local place = self.location or (self.latitude and (self.latitude .. "," .. self.longitude)) or ""
  local url = self.wttr_url .. "/" .. morf.http.url_encode(place) .. "?format=j1"
  morf.http.get(url, { headers = { ["user-agent"] = "curl/8" }, timeout_ms = 20000 }, function(response)
    local data = response.ok and response.json() or nil
    if not data or not data.current_condition then
      self:_stale(done, why or response.error or ("wttr.in answered " .. response.status))
      return
    end
    -- Asked for no place, wttr.in placed the asker by address, and says
    -- where: those coordinates get Open-Meteo's seven days, not wttr.in's
    -- three. Once per instance; after that the forecast is asked directly.
    local area = type(data.nearest_area) == "table" and data.nearest_area[1]
    local lat = area and tonumber(area.latitude)
    local lon = area and tonumber(area.longitude)
    if not why and not self.latitude and lat and lon then
      self.latitude, self.longitude = lat, lon
      local function value(list)
        return type(list) == "table" and type(list[1]) == "table" and list[1].value or nil
      end
      local name, country = value(area.areaName), value(area.country)
      self.place_name = name and (country and (name .. ", " .. country) or name) or nil
      self.region_name = country
      self:_forecast(done)
      return
    end
    self:_finish(done, weather.parse_wttr(data, self.units))
  end)
end

--- Offline: the last answer, however old, is better than none.
function Weather:_stale(done, why)
  local _, _, old = poll.cache_read(self:_cache_path(), 0)
  if old then
    old.stale = true
    done(old)
  else
    done(nil, why)
  end
end

function Weather:_forecast(done)
  local query = morf.http.query {
    latitude = self.latitude,
    longitude = self.longitude,
    current = "temperature_2m,relative_humidity_2m,apparent_temperature,is_day,weather_code,wind_speed_10m,wind_direction_10m",
    hourly = "temperature_2m,weather_code,precipitation_probability,is_day",
    daily = "weather_code,temperature_2m_max,temperature_2m_min,sunrise,sunset,precipitation_probability_max",
    timezone = "auto",
    forecast_days = 7,
    timeformat = "unixtime",
    temperature_unit = self.units == "imperial" and "fahrenheit" or "celsius",
    wind_speed_unit = self.units == "imperial" and "mph" or "kmh",
  }
  morf.http.get(self.forecast_url .. "?" .. query, { timeout_ms = 20000 }, function(response)
    local data = response.ok and response.json() or nil
    if not data or not data.current then
      local why = response.error or ("open-meteo answered " .. response.status)
      if self.fallback then self:_wttr(done, why) else self:_stale(done, why) end
      return
    end
    self:_finish(done, weather.parse_open_meteo(data))
  end)
end

function Weather:_fetch(done)
  if not self._force then
    local cached = poll.cache_read(self:_cache_path(), self.ttl)
    if cached then
      done(cached)
      return
    end
  end
  self._force = false
  if self.latitude and self.longitude then
    self:_forecast(done)
  elseif self.location then
    -- The name is geocoded once per instance; coordinates do not move.
    local query = morf.http.query { name = self.location, count = 1, language = "en", format = "json" }
    morf.http.get(self.geocoding_url .. "?" .. query, { timeout_ms = 20000 }, function(response)
      local data = response.ok and response.json() or nil
      local found = data and data.results and data.results[1]
      if not found then
        if self.fallback then self:_wttr(done) else self:_stale(done, "no place called " .. self.location) end
        return
      end
      self.latitude, self.longitude = found.latitude, found.longitude
      local parts = { found.name }
      if found.country and found.country ~= morf.json.null then
        parts[#parts + 1] = found.country
        self.region_name = found.country
      end
      self.place_name = table.concat(parts, ", ")
      self:_forecast(done)
    end)
  else
    self:_wttr(done)
  end
end

--- A Material Symbols name for a WMO weather code, day or night.
function weather.material_symbol(code, is_day)
  code = tonumber(code) or -1
  if code == 0 then return is_day == false and "clear_night" or "clear_day" end
  if code == 1 or code == 2 then return is_day == false and "partly_cloudy_night" or "partly_cloudy_day" end
  if code == 3 then return "cloud" end
  if code == 45 or code == 48 then return "foggy" end
  if (code >= 51 and code <= 67) or (code >= 80 and code <= 82) then return "rainy" end
  if (code >= 71 and code <= 77) or code == 85 or code == 86 then return "weather_snowy" end
  if code >= 95 then return "thunderstorm" end
  return "cloud"
end

return weather
