-- The egg, drawn soft: a speckled shell in its future colour.
--
-- Port of PetEgg.qml. An ovoid lit from above, speckled in the coat the
-- species inside will wear, so the shelf hints at what is coming. Shared by
-- the two shaded styles (creature and plush).

local draw = require("pets.draw")

local egg = {}

local SHELL = "#ffffff" -- Theme.indicator: the island's white, on every palette

--- The shell's elements in the 100-unit square, and its gradient.
function egg.draw(coat)
  local defs = draw.gradient("egg", 0, 6, 0, 97, {
    { 0.0, SHELL },
    { 0.6, draw.darker(SHELL, 1.08) },
    { 1.0, draw.darker(SHELL, 1.26) },
  })
  local out = {
    draw.path({
      { "M", 50, 6 },
      { "C", 74, 20, 84, 50, 84, 66 },
      { "C", 84, 86, 70, 97, 50, 97 },
      { "C", 30, 97, 16, 86, 16, 66 },
      { "C", 16, 50, 26, 20, 50, 6 },
      { "Z" },
    }, { fill = "url(#egg)" }),
  }
  for _, speck in ipairs {
    { 30, 34, 9 }, { 58, 26, 6 }, { 62, 52, 10 }, { 34, 66, 7 }, { 50, 80, 5 },
  } do
    out[#out + 1] = draw.rect(speck[1], speck[2], speck[3], speck[3], speck[3] / 2,
      { fill = coat, opacity = 0.85 })
  end
  -- The light it is lit by.
  out[#out + 1] = draw.rect(28, 18, 16, 7, 3.5, { fill = SHELL, opacity = 0.9 }, -34)
  return table.concat(out), defs
end

return egg
