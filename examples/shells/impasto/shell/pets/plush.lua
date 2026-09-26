-- The pet drawn soft: one round body, shaded, with the crown its species
-- wears.
--
-- Port of PetPlush.qml. One body for every species, lit from above; round
-- ears, leaves, a flame, a corona or long droopy ears on top. The shades are
-- derived from the coat rather than named, so a palette change takes the
-- whole creature with it.

local draw = require("pets.draw")
local egg = require("pets.egg")
local soft = require("pets.soft_face")

local plush = {}

local LIGHT = "#ffffff" -- Theme.indicator

function plush.draw(p)
  if p.egg then
    local body, defs = egg.draw(p.coat)
    return body, defs, nil
  end
  local coat = p.coat
  local dark = draw.darker(coat, 1.7)
  local mid = draw.darker(coat, 1.25)
  local light = draw.lighter(coat, 1.32)
  local ears = p.kind and p.kind.ears or "round"
  local out = {}
  local defs = {}

  -- Behind the body: a tail for the round and droopy ones, and two feet.
  if ears == "round" or ears == "droop" then
    out[#out + 1] = draw.path({ { "M", 72, 86 }, { "C", 95, 90, 99, 60, 88, 50 } },
      { fill = "none", stroke = mid, ["stroke-width"] = 8.5, ["stroke-linecap"] = "round" })
  end
  for _, side in ipairs { -1, 1 } do
    local w, h = 21, 11
    out[#out + 1] = draw.rect(50 + side * 17 - w / 2, 85, w, h, h / 2, { fill = mid })
  end

  -- The crown.
  if ears == "round" then
    for _, side in ipairs { -1, 1 } do
      local w = 27
      local x = 50 + side * 24 - w / 2
      out[#out + 1] = draw.rect(x, 6, w, w, w / 2, { fill = coat })
      local inner = w * 0.48
      out[#out + 1] = draw.rect(x + (w - inner) / 2, 6 + (w - inner) / 2, inner, inner, inner / 2,
        { fill = dark, opacity = 0.5 })
    end
  elseif ears == "droop" then
    for _, side in ipairs { -1, 1 } do
      local w, h = 16, 50
      out[#out + 1] = draw.rect(50 + side * 36 - w / 2, 20, w, h, w / 2, { fill = mid }, side * 16)
    end
  elseif ears == "leaf" then
    out[#out + 1] = draw.rect(48.5, 4, 4, 22, 2, { fill = mid })
    for _, side in ipairs { -1, 1 } do
      local w, h = 28, 17
      local left = side < 0
      out[#out + 1] = draw.rect(50 + side * 15 - w / 2, 0, w, h, 0, { fill = light }, side * 22,
        { left and h or 0, left and 0 or h, left and h or 0, left and 0 or h })
    end
  elseif ears == "tuft" then
    defs[#defs + 1] = draw.gradient("tuft", 0, 0, 0, 32, { { 0, draw.lighter(coat, 1.6) }, { 1, light } })
    out[#out + 1] = draw.path({
      { "M", 50, -2 }, { "C", 76, 10, 72, 26, 50, 32 }, { "C", 28, 26, 24, 10, 50, -2 }, { "Z" },
    }, { fill = "url(#tuft)" })
  elseif ears == "none" then
    for i = 0, 10 do
      local angle = i * (math.pi * 2 / 11) - math.pi / 2
      local w, h = 11, 5
      out[#out + 1] = draw.rect(50 + math.cos(angle) * 45 - w / 2, 55 + math.sin(angle) * 45 - h / 2,
        w, h, h / 2, { fill = light, opacity = 0.9 }, angle * 180 / math.pi)
    end
  end

  -- The body.
  defs[#defs + 1] = draw.gradient("body", 0, 14, 0, 96, { { 0, light }, { 0.5, coat }, { 1, mid } })
  out[#out + 1] = draw.path({
    { "M", 50, 14 },
    { "C", 78, 14, 87, 40, 87, 61 }, { "C", 87, 85, 73, 95, 50, 95 },
    { "C", 27, 95, 13, 85, 13, 61 }, { "C", 13, 40, 22, 14, 50, 14 }, { "Z" },
  }, { fill = "url(#body)" })
  out[#out + 1] = draw.rect(30, 60, 40, 32, 20, { fill = light, opacity = 0.5 })
  -- The light it is lit by.
  out[#out + 1] = draw.rect(24, 18, 28, 11, 5.5, { fill = LIGHT, opacity = 0.24 }, -24)

  local face, eyes = soft.draw({ face_y = 0.45, gap = 0.16, mouth_y = 0.73 }, p.mood, dark)
  out[#out + 1] = face
  return table.concat(out), table.concat(defs), eyes
end

return plush
