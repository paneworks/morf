-- Paths: shapes that are geometry until they are drawn.
--
-- A face, a ring gauge, a dashed line, a pixel sprite — each is SVG path
-- data on a `ui.Path`, and every number on it is a property: the stroke's
-- width, where its trim ends, the dash's phase, the colours, and the outline
-- itself, which walks onto another outline with the same run of segments.
-- They are drawn at the pixels they cover, so the one scaled four times over
-- in the corner is as sharp as the rest.
--
--   nixVulkan target/release/examples/frame_bench examples/path.lua gpu 800x600 out.png

local morf = require("morf")
local ui = require("morf.ui")

local W, H = 800, 600
morf.surface.width = W
morf.surface.height = H
morf.surface.anchors = { top = true, left = true }

local ink = "#e9edf5"
local muted = "#78849a"
local accent = "#6cc4ff"
local warm = "#ffb86b"

local function card(title, body)
  return ui.Rect {
    width = 180, height = 180, radius = 14, color = "#1b2230",
    ui.Text { x = 14, y = 12, text = title, color = muted, font_size = 13 },
    ui.Item { x = 20, y = 36, width = 140, height = 130, body },
  }
end

-- A heart in a 24-unit box, stretched over whatever size the node is.
local heart = ui.Path {
  anchors = { fill = true },
  view_box = { x = 0, y = 0, w = 24, h = 24 },
  fill_mode = "preserve_aspect_fit",
  d = "M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z",
  fill_color = "#ff5d7a",
  stroke_color = "#ffd0da", stroke_width = 0.8, stroke_join = "round",
}

-- A ring gauge: one circle, stroked, trimmed to how full it is. The trim
-- loops so the ring fills and empties on its own.
local ring = ui.Item {
  anchors = { fill = true },
  ui.Path {
    anchors = { fill = true },
    view_box = { 0, 0, 100, 100 }, fill_mode = "preserve_aspect_fit",
    d = "M50 8 A42 42 0 1 1 49.99 8",
    fill_color = "transparent",
    stroke_color = "#2c3648", stroke_width = 12,
  },
  ui.Path {
    anchors = { fill = true },
    view_box = { 0, 0, 100, 100 }, fill_mode = "preserve_aspect_fit",
    d = "M50 8 A42 42 0 1 1 49.99 8",
    fill_color = "transparent",
    stroke_color = accent, stroke_width = 12, stroke_cap = "round",
    trim_end = 0.72,
    loop = { trim_end = { from = 0.15, to = 0.95, duration = 2400, easing = "in_out_sine", alternate = true } },
  },
}

-- A rounded polyline, dashed, its dashes marching.
local dashes = ui.Path {
  anchors = { fill = true },
  d = "M6 110 L40 30 L74 90 L108 20 L136 70",
  fill_color = "transparent",
  stroke_color = warm, stroke_width = 5, stroke_cap = "round", stroke_join = "round",
  dash = { 14, 10 },
  loop = { dash_offset = { from = 0, to = -24, duration = 600 } },
}

-- A face whose mouth is a morph: a smile and a frown are the same two
-- curves with their points moved, so the one walks onto the other.
local smile = "M34 82 C50 102 90 102 106 82"
local frown = "M34 96 C50 76 90 76 106 96"
local face = ui.Item {
  anchors = { fill = true },
  ui.Path {
    anchors = { fill = true },
    d = "M70 6 C106 6 134 34 134 66 C134 100 106 126 70 126 C34 126 6 100 6 66 C6 34 34 6 70 6 Z",
    fill_color = "#ffd166", stroke_color = "#c9962a", stroke_width = 3,
  },
  ui.Path { anchors = { fill = true }, d = "M44 50 m-8 0 a8 10 0 1 0 16 0 a8 10 0 1 0 -16 0", fill_color = "#2b2118" },
  ui.Path { anchors = { fill = true }, d = "M96 50 m-8 0 a8 10 0 1 0 16 0 a8 10 0 1 0 -16 0", fill_color = "#2b2118" },
  ui.Path {
    anchors = { fill = true },
    d = smile, morph_to = frown, morph_progress = 0.3,
    fill_color = "transparent", stroke_color = "#2b2118", stroke_width = 6, stroke_cap = "round",
  },
}

-- Pixel art is squares, and a path of squares scales without a blur.
local sprite = ui.Path {
  anchors = { fill = true },
  view_box = { x = 0, y = 0, w = 8, h = 8 }, fill_mode = "preserve_aspect_fit",
  d = "M2 0h4v1h1v1h1v3h-1v1h-1v1h-1v1h-2v-1h-1v-1h-1v-1h-1v-3h1v-1h1z M2 2h1v2h-1z M5 2h1v2h-1z",
  fill_rule = "evenodd",
  fill_color = "#7ee787",
}

-- A line drawing itself on: the same trim, from the start.
local signature = ui.Path {
  anchors = { fill = true },
  d = "M6 90 C20 20 40 20 44 70 C48 110 70 110 76 60 C80 30 96 20 104 50 C110 72 124 80 136 40",
  fill_color = "transparent",
  stroke_color = ink, stroke_width = 3, stroke_cap = "round", stroke_join = "round",
  trim_end = 0.6,
}

-- A star at a fifth of the size, scaled back up four times: drawn at the
-- pixels it covers after the scale, not stretched from the small picture.
local star = ui.Item {
  anchors = { fill = true },
  ui.Path {
    x = 52, y = 47, width = 36, height = 36, scale = 4,
    view_box = { 0, 0, 24, 24 },
    d = "M12 2 L15.09 8.26 L22 9.27 L17 14.14 L18.18 21.02 L12 17.77 L5.82 21.02 L7 14.14 L2 9.27 L8.91 8.26 Z",
    fill_color = "#b392f0", stroke_color = "#e6d9ff", stroke_width = 0.6, stroke_join = "miter",
  },
}

local grid = ui.Grid {
  x = 24, y = 24, columns = 4, gap = 12,
  card("fill", heart),
  card("trim · ring", ring),
  card("dash", dashes),
  card("morph", face),
  card("pixels", sprite),
  card("trim · drawing on", signature),
  card("scale x4", star),
  card("stroke joins", ui.Item {
    anchors = { fill = true },
    ui.Path { anchors = { fill = true }, d = "M10 30 L40 6 L70 30", fill_color = "transparent", stroke_color = ink, stroke_width = 10, stroke_join = "miter" },
    ui.Path { anchors = { fill = true }, d = "M10 70 L40 46 L70 70", fill_color = "transparent", stroke_color = ink, stroke_width = 10, stroke_join = "round" },
    ui.Path { anchors = { fill = true }, d = "M10 110 L40 86 L70 110", fill_color = "transparent", stroke_color = ink, stroke_width = 10, stroke_join = "bevel" },
  }),
}

ui.Rect {
  width = W, height = H, color = "#0f131b",
  grid,
  ui.Text {
    x = 24, y = 420, width = W - 48, wrap = true,
    text = "ui.Path — SVG path data, filled and stroked; every number and colour animates, the outline morphs, and it is drawn at the pixels it covers.",
    color = muted, font_size = 14,
  },
}
