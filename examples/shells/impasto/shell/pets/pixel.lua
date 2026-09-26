-- The pet as a sprite: sixteen cells across, lit from the left.
--
-- Port of PetPixel.qml. One map per species for the body, and the face
-- painted over it a cell at a time, so a mood is a handful of blocks rather
-- than a map of its own. The cell is a whole number of pixels and the grid is
-- centred, so the sprite never lands on half a pixel; each row is written as
-- runs of one colour rather than as cells.

local draw = require("pets.draw")

local pixel = {}

local INK = "#000000"   -- Theme.island
local LIGHT = "#ffffff" -- Theme.indicator

-- `#` coat · `l` lit · `o` shaded · `a` the crown · `.` nothing
local base = {
  ".l############o.", ".l############o.", ".l############o.", ".l############o.",
  ".l############o.", ".l############o.", ".l############o.", ".l############o.",
  ".o############o.", "..oooooooooooo..", "...oo......oo...", "................",
}
local function body(top)
  local out = {}
  for _, row in ipairs(top) do out[#out + 1] = row end
  for _, row in ipairs(base) do out[#out + 1] = row end
  return out
end

local bodies = {
  dot = body { "................", "...##......##...", "..####....####..", "..############.." },
  sprout = body { "....aa....aa....", ".....aa..aa.....", "......a##a......", "..############.." },
  ember = body { ".......aa.......", "......aaaa......", ".....aa##aa.....", "..############.." },
  sol = {
    "...a..a..a..a...", "................", "..############..", ".l############o.",
    ".l############o.", "al############oa", ".l############o.", ".l############o.",
    ".l############o.", "al############oa", ".l############o.", ".l############o.",
    ".o############o.", "..oooooooooooo..", "...oo......oo...", "...a..a..a..a...",
  },
  drift = {
    "................", "..############..", ".l############o.", "ol############oo",
    "ol############oo", "ol############oo", "ol############oo", "ol############oo",
    "ol############oo", "ol############oo", "ol############oo", ".l############o.",
    ".o############o.", "..oooooooooooo..", "...oo......oo...", "................",
  },
}

-- The shell, speckled in the coat the species inside will wear.
local shell = {
  "................", "......l##o......", ".....l####o.....", "....l######o....",
  "...l#a######o...", "..l##########o..", "..l#######a##o..", ".l############o.",
  ".l############o.", ".l##a#########o.", ".l############o.", "..l##########o..",
  "..l####a#####o..", "...l########o...", ".....l####o.....", "................",
}

-- The mouth, a cell at a time: { x, y, w }.
local mouths = {
  beaming = { { 5, 9, 6 }, { 6, 10, 4 } },
  content = { { 5, 9, 1 }, { 10, 9, 1 }, { 6, 10, 4 } },
  peckish = { { 6, 10, 4 } },
  lonely = { { 6, 9, 4 }, { 5, 10, 1 }, { 10, 10, 1 } },
  asleep = { { 7, 10, 2 } },
}

--- `p.size` is the drawn size in pixels, which decides the cell.
function pixel.draw(p)
  local cell = math.max(1, math.floor(p.size / 16 + 0.5))
  local u = 100 / p.size * cell          -- one cell in the 100-unit square
  local o = (100 - 16 * u) / 2           -- the grid, centred
  local egg = p.egg
  local ground = egg and LIGHT or p.coat
  local inks = {
    ["#"] = ground,
    l = draw.lighter(ground, 1.3),
    o = draw.darker(ground, egg and 1.22 or 1.5),
    a = egg and p.coat or draw.lighter(ground, 1.45),
  }
  local map = egg and shell or bodies[p.kind and p.kind.id] or bodies.dot
  -- A block of cells; `top` replaces the grid's own top, for the eye band.
  local function block(x, y, w, h, fill, top)
    return string.format('<rect x="%s" y="%s" width="%s" height="%s" fill="%s" shape-rendering="crispEdges"/>',
      draw.n(o + x * u), draw.n((top or o) + y * u), draw.n(w * u), draw.n(h * u), fill)
  end
  local out = {}
  for y, row in ipairs(map) do
    local x = 1
    while x <= #row do
      local c = row:sub(x, x)
      local n = 1
      while x + n <= #row and row:sub(x + n, x + n) == c do n = n + 1 end
      if c ~= "." then out[#out + 1] = block(x - 1, y - 1, n, 1, inks[c]) end
      x = x + n
    end
  end
  if egg then return table.concat(out), "", nil end

  local asleep = p.mood == "asleep"
  if asleep then
    out[#out + 1] = block(3, 7, 3, 1, INK)
    out[#out + 1] = block(10, 7, 3, 1, INK)
  end
  for _, m in ipairs(mouths[p.mood] or mouths.content) do
    out[#out + 1] = block(m[1], m[2], m[3], 1, INK)
  end
  if asleep then return table.concat(out), "", nil end

  -- The eyes, relative to their band (rows 6 to 8): dark, a lit cell at the
  -- top left.
  local eyes = {}
  for _, x in ipairs { 4, 10 } do
    eyes[#eyes + 1] = block(x, 0, 2, 3, INK, 0)
    eyes[#eyes + 1] = block(x, 0, 1, 1, LIGHT, 0)
  end
  return table.concat(out), "", { y = o + 6 * u, h = 3 * u, body = table.concat(eyes) }
end

return pixel
