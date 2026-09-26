-- Drawers that grow out of the screen's edge.
--
-- A thin frame runs round the whole screen: the screen as one box, minus a
-- rounded box inset from it. On each edge a drawer waits, tucked up inside
-- the frame, and slides out when asked. The drawer's background is not a
-- rectangle drawn over the frame -- it is one more shape in the frame's own
-- distance field, following the drawer wherever the drawer is drawn
-- (`track`), and joined to the frame by a circular seam. Where the drawer's
-- side meets the frame's inner edge the join is a concave quarter circle, so
-- the drawer reads as the frame itself bulging inwards.
--
-- While it moves the drawer stretches along its motion and narrows across
-- it (`stretch`), and when it stops it wobbles back to square. The engine
-- measures the drawer's motion itself and bends it -- its text included, and
-- the field layer tracking it -- with nothing in this file running per frame.
--
--     morf examples/demos/motion/drawers.lua
--     morf ipc call open top        -- or bottom, left, right, all
--     morf ipc call close top       -- or all
--     morf ipc call toggle left

local morf = require("morf")
local ui = require("morf.ui")

morf.surface.namespace = "drawers"
morf.surface.anchors = { top = true, bottom = true, left = true, right = true }
-- Zero along an axis anchored at both ends: the compositor's size, the screen.
morf.surface.width = 0
morf.surface.height = 0
morf.surface.layer = "top"
morf.surface.exclusive_zone = -1
morf.surface.keyboard_focus = "none"

local theme = morf.theme {
  frame = "#1c1b22",
  ink = "#e6e1f0",
  muted = "#9d97ad",
  accent = "#c9a7ff",
}

local THICK = 10 -- the frame round the screen
local RADIUS = 22 -- the inside corners of the frame
local SEAM = 18 -- the fillet where a drawer meets the frame

-- A settle with a little overshoot, as one spline: out past the end and
-- back. Qt's BezierSpline points, the last segment ending at (1, 1).
local slide = { spline = { 0.05, 0.7, 0.1, 1.04, 0.62, 1.03, 0.78, 1.01, 0.9, 1.0, 1, 1 } }

-- Where each drawer sits and how far it tucks away. `x`/`y` move it off the
-- edge it hangs from: out of sight, far enough that not even its seam
-- touches the frame.
local EDGES = {
  top = { width = 420, height = 150, anchors = { top = true, horizontal_center = true }, axis = "translate_y", hidden = -1 },
  bottom = { width = 520, height = 120, anchors = { bottom = true, horizontal_center = true }, axis = "translate_y", hidden = 1 },
  left = { width = 260, height = 360, anchors = { left = true, vertical_center = true }, axis = "translate_x", hidden = -1 },
  right = { width = 300, height = 420, anchors = { right = true, vertical_center = true }, axis = "translate_x", hidden = 1 },
}
local ORDER = { "top", "right", "bottom", "left" }

local open = morf.state { top = false, bottom = false, left = false, right = false }

--- How far a drawer moves to be out of sight.
local function tucked(edge)
  local spec = EDGES[edge]
  local size = spec.axis == "translate_y" and spec.height or spec.width
  return spec.hidden * (size + THICK + SEAM + 2)
end

local drawers, shapes = {}, {}
for group, edge in ipairs(ORDER) do
  local spec = EDGES[edge]
  local panel = ui.Item {
    id = "drawer-" .. edge,
    width = spec.width,
    height = spec.height,
    anchors = spec.anchors,
    [spec.axis] = function()
      return open[edge] and 0 or tucked(edge)
    end,
    behavior = { [spec.axis] = { duration = 460, easing = slide } },
    -- Squash and stretch: longer along the slide, narrower across it.
    stretch = { stiffness = 240, damping = 13, scale = 0.16, max = 0.3 },
    ui.Column {
      x = 22, y = 18, gap = 6,
      ui.Text { text = edge, font_size = 20, font_weight = 600, color = theme.accent },
      ui.Text { text = "grows out of the frame", font_size = 13, color = theme.ink },
      ui.Text {
        text = "morf ipc call close " .. edge,
        font_size = 12,
        color = theme.muted,
      },
    },
  }
  drawers[edge] = panel
  -- The drawer's background: a rounded box in the frame's field, wherever
  -- the drawer is drawn. Each drawer is its own blend group, so two drawers
  -- meeting near a corner touch hard while both still merge into the frame.
  shapes[#shapes + 1] = ui.SdfShape {
    shape = "box",
    radius = 18,
    operation = "smooth_union",
    blend_group = group,
    track = panel,
  }
end

local field = {
  anchors = { fill = true },
  fill_color = theme.frame,
  blend = SEAM,
  blend_profile = "circular",
  -- The frame: the whole screen, less the rounded area it surrounds.
  ui.SdfShape { shape = "box", anchors = { fill = true } },
  ui.SdfShape {
    shape = "box",
    anchors = { fill = true, margins = THICK },
    radius = RADIUS,
    operation = "subtract",
  },
}
for _, shape in ipairs(shapes) do
  field[#field + 1] = shape
end

ui.Item {
  anchors = { fill = true },
  ui.Sdf(field),
  -- The drawers' contents, clipped to the inside of the frame so their text
  -- does not ride over it while they slide.
  ui.Item {
    anchors = { fill = true, margins = THICK },
    clip = true,
    drawers.top,
    drawers.right,
    drawers.bottom,
    drawers.left,
  },
}

local function edges_of(which)
  if which == nil or which == "all" then
    return ORDER
  end
  if EDGES[which] == nil then
    error("no drawer on edge `" .. tostring(which) .. "`: top, bottom, left, right or all")
  end
  return { which }
end

morf.ipc.open = function(which)
  for _, edge in ipairs(edges_of(which)) do open[edge] = true end
  return true
end
morf.ipc.close = function(which)
  for _, edge in ipairs(edges_of(which)) do open[edge] = false end
  return true
end
morf.ipc.toggle = function(which)
  for _, edge in ipairs(edges_of(which)) do open[edge] = not open[edge] end
  return true
end
morf.ipc.opened = function()
  local list = {}
  for _, edge in ipairs(ORDER) do
    if open[edge] then list[#list + 1] = edge end
  end
  return table.concat(list, " ")
end
