-- Masks: a node drawn through another's alpha.
--
--     nixVulkanIntel morf render examples/masks.lua -o masks.png
--
-- `mask` is either a node — any subtree: a rounded rect, a field of shapes,
-- a line of text, a drawing — or a gradient across the node's own box. Only
-- the mask's alpha counts. `mask_invert = true` keeps what it does not
-- cover. The mask is laid out in the masked node's box and moves with it, but
-- is never drawn by itself.
--
-- Six cards, each the same colourful plate seen through a different mask, and
-- a list that fades out at its edges while it scrolls.

local morf = require("morf")
local ui = require("morf.ui")
local core = require("morf.core")

local W, H = 980, 470
morf.surface.width = W
morf.surface.height = H
morf.surface.anchors = { top = true, left = true }
morf.surface.keyboard_focus = "none"

local theme = morf.theme {
  ink = "#11141b",
  card = "#1b202b",
  text = "#e9edf5",
  muted = "#78849a",
}

local CARD = 180

--- What every card masks: a plate of colour with a few rows of stripes, so a
--- mask's edge shows against both.
local function plate(extra)
  local node = {
    width = CARD, height = CARD,
    gradient = { angle = 135, space = "oklch", stops = { "#ff5f6d", "#ffc371", "#47cf73", "#3a86ff" } },
  }
  for row = 0, 5 do
    node[#node + 1] = ui.Rect {
      x = 0, y = 12 + row * 30, width = CARD, height = 8, color = "#ffffff55",
    }
  end
  for key, value in pairs(extra) do node[key] = value end
  return ui.Rect(node)
end

local tree = { width = W, height = H }
local function place(node) tree[#tree + 1] = node end
place(ui.Rect { width = W, height = H, color = theme.ink })

local function card(column, row, caption, masked)
  local x, y = 30 + column * (CARD + 40), 24 + row * (CARD + 50)
  place(ui.Rect {
    x = x - 8, y = y - 8, width = CARD + 16, height = CARD + 16, radius = 14,
    color = theme.card,
  })
  place(ui.Item { x = x, y = y, width = CARD, height = CARD, masked })
  place(ui.Text {
    x = x, y = y + CARD + 12, width = CARD, text = caption,
    font_size = 14, horizontal_alignment = "center", color = theme.muted,
  })
end

-- A rounded rect, inset from the plate's edge.
card(0, 0, "a rounded rect", plate {
  mask = ui.Rect { anchors = { fill = true, margins = 18 }, radius = 40 },
})

-- The same, inverted: a window punched through the plate.
card(1, 0, "inverted", plate {
  mask_invert = true,
  mask = ui.Rect { anchors = { fill = true, margins = 18 }, radius = 40 },
})

-- A field of shapes: a circle with a smaller one bitten out of it.
card(2, 0, "a field of shapes", plate {
  mask = ui.Sdf {
    ui.SdfShape { x = 10, y = 10, width = 160, height = 160, shape = "circle" },
    ui.SdfShape { x = 110, y = 110, width = 70, height = 70, shape = "circle",
                  operation = "smooth_subtract", blend = 10 },
  },
})

-- Text as a stencil.
card(0, 1, "text", plate {
  mask = ui.Text {
    text = "morf", font_size = 76, font_weight = 800,
    anchors = { center_in = true },
  },
})

-- A drawing: a heart from an SVG, as a field, and a translucent ring around
-- it — the mask's alpha is the plate's.
card(1, 1, "a drawing, half-alpha ring", plate {
  mask = ui.Item {
    ui.Rect { anchors = { fill = true }, radius = CARD / 2, color = "#ffffff50" },
    ui.Sdf {
      x = 30, y = 30, width = 120, height = 120,
      ui.SdfShape { width = 120, height = 120, source = core.shell_path("assets/sdf-heart.svg") },
    },
  },
})

-- A radial gradient: a vignette that fades to nothing at the corners.
card(2, 1, "a radial gradient", plate {
  mask = { gradient = { kind = "radial", radius = 0.5, stops = { 1, { 1, 0.4 }, 0 } } },
})

-- A list that fades at its top and bottom edges, scrolled part way. The
-- mask belongs to the Flickable, so the fade stays at its edges while the
-- rows move under it.
local rows = { width = 260, gap = 6 }
for index = 1, 24 do
  rows[#rows + 1] = ui.Rect {
    width = 260, height = 34, radius = 8,
    color = index % 2 == 0 and "#2a3242" or "#232a38",
    ui.Text {
      x = 14, anchors = { vertical_center = true }, font_size = 15, color = theme.text,
      text = string.format("Notification %02d", index),
    },
  }
end
local list = ui.Flickable {
  id = "list",
  x = W - 300, y = 24, width = 260, height = H - 70,
  content_y = 120,
  mask = { gradient = { stops = { 0, { 1, 0.18 }, { 1, 0.82 }, 0 } } },
  ui.Column(rows),
}
place(list)
place(ui.Text {
  x = W - 300, y = H - 34, width = 260, text = "a list that fades at its edges",
  font_size = 14, horizontal_alignment = "center", color = theme.muted,
})

morf.ipc.scroll = function(y) list.content_y = tonumber(y) end

return ui.Item(tree)
