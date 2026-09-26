-- The pet drawn as itself: one silhouette per species.
--
-- Port of PetCreature.qml. A different animal for each species rather than
-- one body in five colours: a cat with a tail, a seed under two leaves, a
-- flame, a sun in its corona, a cloud with long ears. Shaded like the plush
-- and wearing the same face, so the two styles differ in shape and not in
-- finish. An unknown species comes out round.

local draw = require("pets.draw")
local egg = require("pets.egg")
local soft = require("pets.soft_face")

local creature = {}

local LIGHT = "#ffffff" -- Theme.indicator

-- Where the face sits, and whether the body carries a belly.
local builds = {
  dot = { face_y = 0.44, gap = 0.16, mouth_y = 0.70, belly = false },
  sprout = { face_y = 0.50, gap = 0.15, mouth_y = 0.76, belly = true },
  ember = { face_y = 0.50, gap = 0.15, mouth_y = 0.76, belly = false },
  sol = { face_y = 0.46, gap = 0.17, mouth_y = 0.72, belly = false },
  drift = { face_y = 0.48, gap = 0.16, mouth_y = 0.74, belly = false },
}
local round_build = { face_y = 0.45, gap = 0.16, mouth_y = 0.72, belly = false }

-- The bodies, as Qt wrote them: a start point and cubic segments.
local bodies = {
  dot = { 16, 96, { 50, 16 }, {
    { 74, 16, 84, 38, 84, 58 }, { 84, 84, 72, 96, 50, 96 },
    { 28, 96, 16, 84, 16, 58 }, { 16, 38, 26, 16, 50, 16 } } },
  sprout = { 22, 97, { 50, 22 }, {
    { 66, 22, 84, 44, 84, 66 }, { 84, 87, 70, 97, 50, 97 },
    { 30, 97, 16, 87, 16, 66 }, { 16, 44, 34, 22, 50, 22 } } },
  ember = { 2, 97, { 50, 2 }, {
    { 60, 22, 86, 36, 86, 64 }, { 86, 86, 72, 97, 50, 97 },
    { 28, 97, 14, 86, 14, 64 }, { 14, 36, 40, 22, 50, 2 } } },
  sol = { 20, 92, { 50, 20 }, {
    { 70, 20, 86, 36, 86, 56 }, { 86, 76, 70, 92, 50, 92 },
    { 30, 92, 14, 76, 14, 56 }, { 14, 36, 30, 20, 50, 20 } } },
  drift = { 22, 94, { 12, 66 }, {
    { 6, 46, 16, 30, 32, 32 }, { 36, 16, 60, 14, 66, 30 },
    { 84, 28, 94, 44, 88, 66 }, { 92, 88, 74, 96, 50, 96 },
    { 26, 96, 8, 88, 12, 66 } } },
}

local function body_path(shape)
  local steps = { { "M", shape[3][1], shape[3][2] } }
  for _, c in ipairs(shape[4]) do steps[#steps + 1] = { "C", c[1], c[2], c[3], c[4], c[5], c[6] } end
  steps[#steps + 1] = { "Z" }
  return draw.path(steps, { fill = "url(#body)" })
end

--- `{ kind, coat, mood, egg }` -> elements, defs, eye band.
function creature.draw(p)
  if p.egg then
    local body, defs = egg.draw(p.coat)
    return body, defs, nil
  end
  local coat = p.coat
  local dark = draw.darker(coat, 1.75)
  local mid = draw.darker(coat, 1.28)
  local light = draw.lighter(coat, 1.35)
  local id = p.kind and p.kind.id or "dot"
  local build = builds[id] or round_build
  local shape = bodies[id] or bodies.dot
  local out = {}

  -- Behind the body.
  if id == "dot" then
    -- The tail, then two pointed ears.
    out[#out + 1] = draw.path({ { "M", 72, 86 }, { "C", 100, 92, 102, 50, 86, 38 } },
      { fill = "none", stroke = mid, ["stroke-width"] = 8, ["stroke-linecap"] = "round" })
    for _, side in ipairs { -1, 1 } do
      out[#out + 1] = draw.path({
        { "M", 50 + side * 26, 6 }, { "L", 50 + side * 38, 34 },
        { "L", 50 + side * 14, 30 }, { "L", 50 + side * 26, 6 }, { "Z" },
      }, { fill = mid, stroke = mid, ["stroke-width"] = 7, ["stroke-linejoin"] = "round" })
    end
  elseif id == "sol" then
    for i = 0, 11 do
      local angle = i * (math.pi * 2 / 12) - math.pi / 2
      local w, h = 12, 5
      out[#out + 1] = draw.rect(50 + math.cos(angle) * 44 - w / 2, 56 + math.sin(angle) * 44 - h / 2,
        w, h, h / 2, { fill = light }, angle * 180 / math.pi)
    end
  elseif id == "drift" then
    for _, side in ipairs { -1, 1 } do
      local w, h = 16, 46
      out[#out + 1] = draw.rect(50 + side * 43 - w / 2, 46, w, h, w / 2, { fill = mid }, side * 24)
    end
  end

  -- The body.
  local defs
  if id == "ember" then
    defs = draw.gradient("body", 0, 2, 0, 97, {
      { 0.0, draw.lighter(coat, 1.7) }, { 0.32, light }, { 0.62, coat }, { 1.0, mid },
    })
  else
    local middle = id == "drift" and 0.5 or 0.48
    defs = draw.gradient("body", 0, shape[1], 0, shape[2], { { 0.0, light }, { middle, coat }, { 1.0, mid } })
  end
  out[#out + 1] = body_path(shape)

  -- A seed's stem and its two leaves, over the body.
  if id == "sprout" then
    out[#out + 1] = draw.rect(48, 6, 4, 22, 2, { fill = mid })
    for _, side in ipairs { -1, 1 } do
      local w, h = 30, 17
      local left = side < 0
      out[#out + 1] = draw.rect(50 + side * 16 - w / 2, 2, w, h, 0, { fill = light }, side * 22,
        { left and h or 0, left and 0 or h, left and h or 0, left and 0 or h })
    end
  end

  -- Shading and face.
  if build.belly then
    out[#out + 1] = draw.rect(32, 62, 36, 30, 18, { fill = light, opacity = 0.45 })
  end
  out[#out + 1] = draw.rect(26, id == "ember" and 28 or 22, 26, 10, 5, { fill = LIGHT, opacity = 0.22 }, -26)

  local face, eyes = soft.draw(build, p.mood, dark)
  out[#out + 1] = face
  return table.concat(out), defs, eyes
end

return creature
