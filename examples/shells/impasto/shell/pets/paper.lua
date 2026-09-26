-- The pet cut flat: two tones, a hard edge between them.
--
-- Port of PetPaper.qml. No gradient but one hard diagonal between the lit
-- half and the shaded one, geometric crowns and white eyes whose pupils look
-- out. The flattest of the styles, and the one that reads smallest.

local draw = require("pets.draw")

local paper = {}

local INK = "#000000"   -- Theme.island
local LIGHT = "#ffffff" -- Theme.indicator

-- The ovoid both the egg and nothing else use; the body is rounder.
local function egg(coat)
  local shade = draw.darker(LIGHT, 1.3)
  local defs = draw.gradient("egg", 10, 10, 90, 96, {
    { 0, LIGHT }, { 0.62, LIGHT }, { 0.621, shade }, { 1, shade },
  })
  local out = {
    draw.path({
      { "M", 50, 6 },
      { "C", 74, 20, 84, 50, 84, 66 }, { "C", 84, 86, 70, 97, 50, 97 },
      { "C", 30, 97, 16, 86, 16, 66 }, { "C", 16, 50, 26, 20, 50, 6 }, { "Z" },
    }, { fill = "url(#egg)" }),
  }
  for _, speck in ipairs { { 30, 34, 10 }, { 58, 26, 7 }, { 60, 54, 11 }, { 34, 68, 8 } } do
    out[#out + 1] = draw.rect(speck[1], speck[2], speck[3], speck[3], speck[3] / 2, { fill = coat })
  end
  return table.concat(out), defs, nil
end

-- The mouth: one flat block, shaped by mood (rounded below, or above when
-- lonely, so it turns down). It is not in the drawing: the face draws it
-- live (`paper.mouth`), so a change of mood reshapes it rather than
-- swapping it, as the original's Behaviors on its width and height do.
local mouths = {
  beaming = { 22, 11 }, peckish = { 16, 4.5 }, lonely = { 18, 8.5 }, asleep = { 12, 4.5 },
}

local K = 0.5523 -- a quarter circle's control distance, as a fraction of its radius

--- The mouth for a mood as path data in the pet's 0..100 square: always the
--- same run of moves and curves (a square corner is a curve of no size), so
--- one mood's mouth walks into the next's.
function paper.mouth(mood)
  local size = mouths[mood] or { 18, 8.5 }
  local w, h = size[1], size[2]
  local x, y = 50 - w / 2, 76
  local limit = math.min(w, h) / 2
  local round = math.min(h, limit)
  local top = mood == "lonely" and round or 0
  local bottom = mood == "lonely" and 0 or round
  local tl, tr, br, bl = top, top, bottom, bottom
  local n = draw.n
  local function pt(px, py) return n(px) .. " " .. n(py) end
  return table.concat({
    "M", pt(x + tl, y),
    "L", pt(x + w - tr, y),
    "C", pt(x + w - tr + tr * K, y), pt(x + w, y + tr - tr * K), pt(x + w, y + tr),
    "L", pt(x + w, y + h - br),
    "C", pt(x + w, y + h - br + br * K), pt(x + w - br + br * K, y + h), pt(x + w - br, y + h),
    "L", pt(x + bl, y + h),
    "C", pt(x + bl - bl * K, y + h), pt(x, y + h - bl + bl * K), pt(x, y + h - bl),
    "L", pt(x, y + tl),
    "C", pt(x, y + tl - tl * K), pt(x + tl - tl * K, y), pt(x + tl, y),
    "Z",
  }, " ")
end

function paper.draw(p)
  if p.egg then return egg(p.coat) end
  local coat = p.coat
  local mid = draw.darker(coat, 1.45)
  local ears = p.kind and p.kind.ears or "round"
  local asleep = p.mood == "asleep"
  local out = {}

  if ears == "round" then
    for _, side in ipairs { -1, 1 } do
      out[#out + 1] = draw.path({
        { "M", 50 + side * 30, 0 }, { "L", 50 + side * 46, 42 },
        { "L", 50 + side * 12, 32 }, { "L", 50 + side * 30, 0 }, { "Z" },
      }, { fill = mid })
    end
  elseif ears == "droop" then
    for _, side in ipairs { -1, 1 } do
      local w, h = 18, 54
      out[#out + 1] = draw.rect(50 + side * 38 - w / 2, 6, w, h, w / 2, { fill = mid }, side * 12)
    end
  elseif ears == "leaf" then
    for _, side in ipairs { -1, 1 } do
      local w, h = 32, 18
      local left = side < 0
      out[#out + 1] = draw.rect(50 + side * 16 - w / 2, -2, w, h, 0, { fill = mid }, side * 20,
        { left and h or 0, left and 0 or h, left and h or 0, left and 0 or h })
    end
  elseif ears == "tuft" then
    out[#out + 1] = draw.path({ { "M", 50, -2 }, { "L", 76, 34 }, { "L", 24, 34 }, { "Z" } }, { fill = mid })
  elseif ears == "none" then
    for i = 0, 7 do
      local a = i * (math.pi * 2 / 8) - math.pi / 2
      out[#out + 1] = draw.path({
        { "M", 50 + math.cos(a) * 50, 55 + math.sin(a) * 50 },
        { "L", 50 + math.cos(a + 0.26) * 34, 55 + math.sin(a + 0.26) * 34 },
        { "L", 50 + math.cos(a - 0.26) * 34, 55 + math.sin(a - 0.26) * 34 }, { "Z" },
      }, { fill = mid })
    end
  end

  -- One silhouette, cut in two along a hard edge.
  local defs = draw.gradient("body", 6, 10, 94, 98, {
    { 0, coat }, { 0.60, coat }, { 0.601, mid }, { 1, mid },
  })
  out[#out + 1] = draw.path({
    { "M", 50, 16 },
    { "C", 88, 16, 94, 46, 94, 66 }, { "C", 94, 88, 75, 97, 50, 97 },
    { "C", 25, 97, 6, 88, 6, 66 }, { "C", 6, 46, 12, 16, 50, 16 }, { "Z" },
  }, { fill = "url(#body)" })

  if asleep then
    for _, side in ipairs { -1, 1 } do
      local cx = 50 + side * 19
      out[#out + 1] = draw.path({ { "M", cx - 9, 54 }, { "Q", cx, 46, cx + 9, 54 } },
        { fill = "none", stroke = INK, ["stroke-width"] = 5, ["stroke-linecap"] = "round" })
    end
  end

  local lonely = p.mood == "lonely"

  if asleep then return table.concat(out), defs, nil end

  -- White eyes; the pupil looks out, away from the other eye, and down when
  -- lonely.
  local ew, eh = 24, 30
  local eyes = {}
  for _, side in ipairs { -1, 1 } do
    local x = 50 + side * 19 - ew / 2
    eyes[#eyes + 1] = draw.rect(x, 0, ew, eh, ew / 2, { fill = LIGHT })
    local pupil = ew * 0.46
    eyes[#eyes + 1] = draw.rect(x + ew * (side < 0 and 0.14 or 0.40), eh * (lonely and 0.44 or 0.32),
      pupil, pupil, pupil / 2, { fill = INK })
  end
  return table.concat(out), defs, { y = 40, h = eh, body = table.concat(eyes) }
end

return paper
