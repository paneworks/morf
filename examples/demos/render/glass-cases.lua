-- Glass cases: four glass materials over one busy backdrop, side by side.
--
--     morf render examples/demos/render/glass-cases.lua -o glass.png --size 1200x420
--
-- Clear bends the stripes at its rim and blurs none of them, frosted blurs
-- and greys them, darkened carries light text and lightened dark text.
-- Each is a `ui.Rect`'s `glass` (docs/UI.md, "Glass"): refraction, frost,
-- tint and rim.

local ui = require("morf.ui")

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
  { "clear", { refraction = 18, frost = 0, rim = 0.55 }, "#ffffff" },
  { "frosted", { blur = 24, frost = 1, saturation = 0.5, tint = "#ffffff26", rim = 0.35 }, "#ffffff" },
  { "darkened", { blur = 24, frost = "auto", tint = "#121214b3", rim = 0.2 }, "#ffffff" },
  { "lightened", { blur = 24, frost = "auto", tint = "#fafafbb8", rim = 0.45 }, "#1d1d20" },
}
for index, case in ipairs(CASES) do
  local label, glass, ink = case[1], case[2], case[3]
  local x = 30 + (index - 1) * 290
  children[#children + 1] = ui.Rect {
    id = label, x = x, y = 110, width = 260, height = 200, radius = 22, color = "#00000000", glass = glass,
  }
  children[#children + 1] = ui.Text {
    x = x + 20, y = 266, width = 220, text = label, font_size = 20, font_weight = 700, color = ink,
  }
end

ui.Item { width = 1200, height = 420, table.unpack(children) }
