-- Unit conversion, as a launcher's calculator reads it: "10ft in m",
-- "72 f to c", "5 gb in mib", "90 km/h in mph", "3h in min".
--
--   local units = require("lib.util.units")
--   local value, unit = units.convert("10ft in m")   -- 3.048, "m"
--   units.parse("10 ft to m")                        -- { value, from, to }
--
-- Length, mass, volume, area, time, speed, data and temperature, each a
-- table of factors to its base unit (temperature by formula). Names are
-- matched without case, singular or plural, with common spellings.

local units = {}

local function kind(base, list)
  local out = {}
  for name, factor in pairs(list) do out[name] = { kind = base, factor = factor } end
  return out
end

local TABLES = {
  kind("m", {
    m = 1, meter = 1, metre = 1, km = 1000, kilometer = 1000, kilometre = 1000,
    cm = 0.01, centimeter = 0.01, mm = 0.001, millimeter = 0.001, um = 1e-6, micrometer = 1e-6,
    nm = 1e-9, ["in"] = 0.0254, inch = 0.0254, ft = 0.3048, foot = 0.3048, feet = 0.3048,
    yd = 0.9144, yard = 0.9144, mi = 1609.344, mile = 1609.344, nmi = 1852, ["nautical mile"] = 1852,
  }),
  kind("kg", {
    kg = 1, kilogram = 1, g = 0.001, gram = 0.001, mg = 1e-6, milligram = 1e-6, t = 1000, tonne = 1000,
    lb = 0.45359237, lbs = 0.45359237, pound = 0.45359237, oz = 0.028349523125, ounce = 0.028349523125,
    st = 6.35029318, stone = 6.35029318,
  }),
  kind("l", {
    l = 1, liter = 1, litre = 1, ml = 0.001, milliliter = 0.001, cl = 0.01, dl = 0.1,
    m3 = 1000, gal = 3.785411784, gallon = 3.785411784, qt = 0.946352946, quart = 0.946352946,
    pt = 0.473176473, pint = 0.473176473, cup = 0.2365882365, floz = 0.0295735295625,
    tbsp = 0.01478676478125, tsp = 0.00492892159375,
  }),
  kind("m2", {
    m2 = 1, km2 = 1e6, cm2 = 1e-4, ha = 1e4, hectare = 1e4, acre = 4046.8564224,
    ft2 = 0.09290304, sqft = 0.09290304, mi2 = 2589988.110336,
  }),
  kind("s", {
    s = 1, sec = 1, second = 1, ms = 0.001, millisecond = 0.001, us = 1e-6, min = 60, minute = 60,
    h = 3600, hr = 3600, hour = 3600, d = 86400, day = 86400, wk = 604800, week = 604800,
    mo = 2629800, month = 2629800, y = 31557600, yr = 31557600, year = 31557600,
  }),
  kind("m/s", {
    ["m/s"] = 1, ["km/h"] = 1 / 3.6, kph = 1 / 3.6, kmh = 1 / 3.6, mph = 0.44704,
    ["ft/s"] = 0.3048, kn = 0.514444, knot = 0.514444,
  }),
  kind("B", {
    b = 1 / 8, bit = 1 / 8, byte = 1, B = 1, kb = 1e3, mb = 1e6, gb = 1e9, tb = 1e12, pb = 1e15,
    kib = 1024, mib = 1024 ^ 2, gib = 1024 ^ 3, tib = 1024 ^ 4,
    kbit = 125, mbit = 125e3, gbit = 125e6,
  }),
}

local TEMPERATURE = {
  c = "c", celsius = "c", ["°c"] = "c", f = "f", fahrenheit = "f", ["°f"] = "f", k = "k", kelvin = "k",
}

local function lookup(name)
  name = name:lower():gsub("%s+", " "):gsub("^%s", ""):gsub("%s$", "")
  if TEMPERATURE[name] then return { kind = "temperature", scale = TEMPERATURE[name] } end
  local candidates = { name, (name:gsub("s$", "")), (name:gsub("es$", "")) }
  for _, table_ in ipairs(TABLES) do
    for _, candidate in ipairs(candidates) do
      if table_[candidate] then return table_[candidate] end
    end
  end
  return nil
end

--- "<number> <unit> in|to|as <unit>", or nil.
function units.parse(text)
  local number, from, to = tostring(text):match("^%s*(-?[%d%.]+)%s*([%a°/%d ]-)%s+[iIaAtT][nNsSoO]%s+([%a°/%d ]+)%s*$")
  local value = tonumber(number)
  if not (value and from and to) or from == "" then return nil end
  return { value = value, from = from, to = to }
end

local function to_kelvin(value, scale)
  if scale == "c" then return value + 273.15 end
  if scale == "f" then return (value - 32) * 5 / 9 + 273.15 end
  return value
end

local function from_kelvin(value, scale)
  if scale == "c" then return value - 273.15 end
  if scale == "f" then return (value - 273.15) * 9 / 5 + 32 end
  return value
end

--- The converted value and the target unit as written, or nil, why.
function units.convert(text)
  local q = units.parse(text)
  if not q then return nil, "not a conversion" end
  local a, b = lookup(q.from), lookup(q.to)
  if not (a and b) then return nil, "unknown unit" end
  if a.kind ~= b.kind then return nil, "cannot turn " .. a.kind .. " into " .. b.kind end
  if a.kind == "temperature" then
    return from_kelvin(to_kelvin(q.value, a.scale), b.scale), q.to
  end
  return q.value * a.factor / b.factor, q.to
end

--- A number as a person reads it: up to `digits` significant places, no
--- trailing zeros, thousands apart.
function units.format(value, digits)
  if value ~= value then return "NaN" end
  if value == math.huge or value == -math.huge then return value > 0 and "∞" or "-∞" end
  local text = ("%." .. (digits or 10) .. "g"):format(value)
  if text:find("e") then return text end
  local whole, fraction = text:match("^(-?%d+)(%.?%d*)$")
  if not whole then return text end
  local sign = whole:sub(1, 1) == "-" and "-" or ""
  whole = whole:gsub("^-", "")
  whole = whole:reverse():gsub("(%d%d%d)", "%1,"):reverse():gsub("^,", "")
  return sign .. whole .. fraction
end

return units
