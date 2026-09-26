-- The drawn face of the two shaded styles: glossy eyes that blink and shut,
-- a touch of colour under them and one mouth per mood.
--
-- Port of PetSoftFace.qml. Everything is placed in fractions of the size, so
-- the body says where the face belongs (`face_y`, `gap`, `mouth_y`) and the
-- face draws itself there.
--
-- The face comes in two halves: what never moves goes into the body's
-- document, and the open eyes are a document of their own, which the pet
-- squashes vertically to blink -- the Scale transform the original puts on
-- each eye, applied once to the band both eyes share.

local draw = require("pets.draw")

local soft = {}

local INK = "#000000"   -- Theme.island
local LIGHT = "#ffffff" -- Theme.indicator

--- `place` = { face_y, gap, mouth_y } as fractions; `mood`; `blush` a hex.
--- Returns the still elements, and the eye band `{ y, h, body }` (nil while
--- asleep), all in the 100-unit square.
function soft.draw(place, mood, blush)
  local s = 100
  local face_y, gap, mouth_y = place.face_y, place.gap, place.mouth_y
  local asleep = mood == "asleep"
  local out = {}

  -- Shut, and curved the way an eye closes.
  if asleep then
    for _, side in ipairs { -1, 1 } do
      local cx = s * (0.5 + side * gap)
      out[#out + 1] = draw.path({
        { "M", cx - s * 0.08, s * (face_y + 0.10) },
        { "Q", cx, s * (face_y + 0.02), cx + s * 0.08, s * (face_y + 0.10) },
      }, { fill = "none", stroke = INK, ["stroke-width"] = s * 0.04, ["stroke-linecap"] = "round" })
    end
  end

  -- Colour under the eyes.
  for _, side in ipairs { -1, 1 } do
    local w, h = s * 0.12, s * 0.06
    out[#out + 1] = draw.rect(s * 0.5 + side * s * 0.28 - w / 2, s * (mouth_y - 0.07), w, h, h / 2,
      { fill = blush, opacity = 0.4 })
  end

  -- The mouth.
  if mood == "beaming" then
    -- Open, flat on top and round below.
    out[#out + 1] = draw.path({
      { "M", s * 0.38, s * (mouth_y - 0.01) },
      { "L", s * 0.62, s * (mouth_y - 0.01) },
      { "C", s * 0.62, s * (mouth_y + 0.15), s * 0.38, s * (mouth_y + 0.15), s * 0.38, s * (mouth_y - 0.01) },
      { "Z" },
    }, { fill = INK })
  elseif mood == "peckish" then
    local w, h = s * 0.16, s * 0.045
    out[#out + 1] = draw.rect(s * 0.5 - w / 2, s * (mouth_y + 0.01), w, h, h / 2, { fill = INK })
  elseif mood == "lonely" then
    out[#out + 1] = draw.path({
      { "M", s * 0.43, s * (mouth_y + 0.05) },
      { "Q", s * 0.5, s * (mouth_y - 0.03), s * 0.57, s * (mouth_y + 0.05) },
    }, { fill = "none", stroke = INK, ["stroke-width"] = s * 0.045, ["stroke-linecap"] = "round" })
  else
    -- Content, or the smaller smile of sleep.
    out[#out + 1] = draw.path({
      { "M", s * 0.43, s * mouth_y },
      { "Q", s * 0.5, s * (mouth_y + (asleep and 0.05 or 0.08)), s * 0.57, s * mouth_y },
    }, { fill = "none", stroke = INK, ["stroke-width"] = s * 0.045, ["stroke-linecap"] = "round" })
  end

  if asleep then return table.concat(out), nil end

  -- Glossy eyes: dark, with a big and a small highlight.
  local ew, eh = s * 0.19, s * 0.24
  local eyes = {}
  for _, side in ipairs { -1, 1 } do
    local x = s * 0.5 + side * s * gap - ew / 2
    eyes[#eyes + 1] = draw.rect(x, 0, ew, eh, ew / 2, { fill = INK })
    local big = ew * 0.42
    eyes[#eyes + 1] = draw.rect(x + ew * 0.14, eh * 0.12, big, big, big / 2, { fill = LIGHT })
    local small = ew * 0.24
    eyes[#eyes + 1] = draw.rect(x + ew * 0.48, eh * 0.64, small, small, small / 2, { fill = LIGHT, opacity = 0.5 })
  end
  return table.concat(out), { y = s * face_y, h = eh, body = table.concat(eyes) }
end

return soft
