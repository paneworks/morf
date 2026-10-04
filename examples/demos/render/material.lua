-- Material 3 schemes from lib/material.lua: one colour, every variant, in
-- dark and light. A swatch per role that a shell draws with most.
--
--   morf render examples/demos/render/material.lua -o material.png --size 1200x900

morf.surface.width = 1064
morf.surface.height = 956
morf.surface.anchors = { top = true, left = true }

local ui = require("morf.ui")
local material = require("lib.util.material")

local source = morf.env("MATERIAL_SOURCE") or "#4a7fb5"
local ROLES = {
  "primary", "primaryContainer", "secondary", "secondaryContainer",
  "tertiary", "tertiaryContainer", "surface", "surfaceContainer",
  "surfaceContainerHighest", "surfaceVariant", "outline", "error",
}

local function row(variant, mode)
  local s = material.scheme(source, { variant = variant, mode = mode })
  local cells = {}
  for _, role in ipairs(ROLES) do
    cells[#cells + 1] = ui.Rect { width = 64, height = 32, radius = 8, color = s[role] }
  end
  return ui.Rect {
    width = 1040, height = 48, color = s.surface, radius = 12,
    ui.Row {
      anchors = { fill = true, margins = 8 }, gap = 6,
      ui.Text { text = variant, color = s.onSurface, font_size = 13, width = 110 },
      table.unpack(cells),
    },
  }
end

local rows = {}
for _, mode in ipairs { "dark", "light" } do
  for _, variant in ipairs(material.VARIANTS) do rows[#rows + 1] = row(variant, mode) end
end
ui.Rect {
  width = 1064, height = 18 * 52 + 20, color = "#202020",
  ui.Column { anchors = { fill = true, margins = 12 }, gap = 4, table.unpack(rows) },
}
