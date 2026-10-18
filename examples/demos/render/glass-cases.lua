-- Glass cases: the default kit's four glass surfaces over one busy backdrop,
-- side by side.
--
--     morf render examples/demos/render/glass-cases.lua -o glass.png --size 1200x420
--
-- Clear bends the stripes at its rim and blurs none of them, frosted blurs
-- and greys them, darkened carries light text and lightened dark text. Each
-- is a surface style asked for by name (`kit.surface { style = ... }`, the
-- default kit's `tokens.surfaces`), and each style is the glass material
-- in docs/UI.md ("Glass"): refraction, frost, tint and rim.

local ui = require("morf.ui")
local kit = require("lib.kit.skins.default").kit

morf.surface.width = 1200
morf.surface.height = 420

local STRIPE = 25
local children = {}
for i = 0, 1200 / STRIPE - 1 do
  children[#children + 1] = ui.Rect {
    x = i * STRIPE, width = STRIPE, height = 420,
    color = i % 2 == 0 and "#e9c46a" or "#264653",
  }
end

local CASES = {
  { "clear", "clear_glass" },
  { "frosted", "frosted_glass" },
  { "darkened", "dark_glass" },
  { "lightened", "light_glass" },
}
for index, case in ipairs(CASES) do
  local label, style = case[1], case[2]
  local x = 30 + (index - 1) * 290
  children[#children + 1] = kit.surface {
    id = label, style = style, x = x, y = 110, width = 260, height = 200, radius = 22,
  }
  children[#children + 1] = ui.Text {
    x = x + 20, y = 266, width = 220, text = label, font_size = 20, font_weight = 700,
    color = kit.surface_ink(style),
  }
end

ui.Item { width = 1200, height = 420, table.unpack(children) }
