-- Glass cases: four treatments of the same busy backdrop, side by side.
--
--     morf render examples/demos/render/glass-cases.lua -o glass.png --size 1200x420
--
-- Clear keeps only the blur, frosted greys it, darkened suits light text and
-- lightened suits dark text. All four are the named material in docs/UI.md:
-- `backdrop_blur`, `backdrop_saturation` and `backdrop_tint` over one scene.

local ui = require("morf.ui")

morf.surface.width = 1200
morf.surface.height = 420

local tiles = {}
for i = 0, 23 do
  tiles[#tiles + 1] = ui.Rect {
    x = i * 50, width = 50, height = 420,
    color = i % 2 == 0 and "#cc3333" or "#3366cc",
  }
end

local function panel(x, label, props)
  local p = { x = x, y = 110, width = 260, height = 200, radius = 22, id = label }
  for k, v in pairs(props) do p[k] = v end
  return ui.Item {
    ui.Rect(p),
    ui.Text { x = x + 20, y = 330, width = 220, text = label, font_size = 20, color = "#ffffff" },
  }
end

local children = { ui.Rect { width = 1200, height = 420, color = "#181818" } }
for _, tile in ipairs(tiles) do children[#children + 1] = tile end
children[#children + 1] = panel(30, "clear", { backdrop_blur = 24, color = "#ffffff00" })
children[#children + 1] = panel(320, "frosted",
  { backdrop_blur = 24, backdrop_saturation = 0.25, color = "#ffffff22" })
children[#children + 1] = panel(610, "darkened",
  { backdrop_blur = 24, backdrop_tint = "#00000088", color = "#00000044" })
children[#children + 1] = panel(900, "lightened",
  { backdrop_blur = 24, backdrop_tint = "#ffffff88", color = "#ffffff44" })

ui.Item { width = 1200, height = 420, table.unpack(children) }
