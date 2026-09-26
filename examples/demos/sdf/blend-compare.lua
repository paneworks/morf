-- One design, blended two ways: linear light on the left, sRGB on the right.
--
--     nixVulkan target/release/examples/frame_bench examples/demos/sdf/blend-compare.lua gpu 900x400 out.png
--
-- morf blends in linear light unless a surface asks otherwise. That is the
-- physically right way to mix light, and it is not what a browser, Qt or GTK
-- does: they mix the sRGB-encoded values. The two agree on every opaque
-- colour and disagree on every translucent one, and the disagreement is not
-- small — a 3.5% white hairline over a dark panel is a hint in a browser and a
-- visible line here, and 50% white over black is #808080 there and #bcbcbc
-- here. A design drawn in one and rebuilt in the other comes out brighter.
--
-- `blend = "srgb"` on a surface — `morf.surface.blend`, or on any
-- `morf.window.*` surface — makes it mix the way the design was made. The
-- left half is the shell's own surface, linear; the right half is a layer
-- surface in sRGB, drawing the same tree of the same colours.

local morf = require("morf")
local ui = require("morf.ui")

local W, H = 900, 400
local HALF = W / 2

morf.surface.width = W
morf.surface.height = H
morf.surface.layer = "overlay"
morf.surface.keyboard_focus = "none"
morf.surface.exclusive_zone = -1
-- The default, written out for the comparison.
morf.surface.blend = "linear"

local GROUND = "#0e1213"
local INK = "#e8eef0"

-- The design: a dark panel carrying the translucent things a shell is made
-- of. Built by a function so each surface gets its own copy of the tree.
local function design(title)
  local function veil(y, alpha, label)
    return ui.Item {
      x = 24, y = y, width = HALF - 48, height = 30,
      ui.Rect { x = 0, y = 0, width = 150, height = 30, radius = 6, color = GROUND },
      ui.Rect {
        x = 0, y = 0, width = 150, height = 30, radius = 6,
        color = morf.color("#ffffff"):alpha(alpha),
      },
      ui.Text {
        x = 164, y = 6, text = label, color = INK, font_size = 14,
      },
    }
  end
  return ui.Rect {
    x = 0, y = 0, width = HALF, height = H, color = GROUND,
    ui.Text { x = 24, y = 18, text = title, color = INK, font_size = 20, font_weight = 600 },
    -- A hairline under the title, as a panel separates its header.
    ui.Rect { x = 24, y = 52, width = HALF - 48, height = 1, color = morf.color("#ffffff"):alpha(0.035) },
    veil(68, 0.035, "white 3.5%"),
    veil(106, 0.10, "white 10%"),
    veil(144, 0.50, "white 50%"),
    -- The quickshell ribbon's empty pill: color240 at 60%.
    ui.Rect {
      x = 24, y = 186, width = 150, height = 30, radius = 15,
      color = morf.color("#6a8389"):alpha(0.6),
    },
    ui.Text { x = 188, y = 192, text = "#6a8389 at 60%", color = INK, font_size = 14 },
    -- Muted text: ink at half strength.
    ui.Text {
      x = 24, y = 230, text = "secondary text at 50% ink",
      color = morf.color(INK):alpha(0.5), font_size = 16,
    },
    -- A translucent accent fading out, over a stripe it half covers.
    ui.Rect { x = 24, y = 270, width = HALF - 48, height = 12, color = "#3c78d8" },
    ui.Rect {
      x = 24, y = 262, width = HALF - 48, height = 28, radius = 8,
      gradient = { angle = 90, stops = { morf.color("#ff9f43"):alpha(0.8), morf.color("#ff9f43"):alpha(0.0) } },
    },
    -- Opaque colours: these must match exactly across the two halves.
    ui.Rect { x = 24, y = 312, width = 60, height = 60, radius = 8, color = "#3c78d8" },
    ui.Rect { x = 96, y = 312, width = 60, height = 60, radius = 8, color = "#ff9f43" },
    ui.Rect { x = 168, y = 312, width = 60, height = 60, radius = 8, color = "#6a8389" },
    ui.Text { x = 244, y = 334, text = "opaque: identical", color = INK, font_size = 14 },
  }
end

ui.Item {
  width = W, height = H,
  design("linear (morf's default)"),
}

morf.window.layer {
  namespace = "blend-compare-srgb",
  layer = "overlay",
  keyboard_focus = "none",
  anchors = { right = true },
  width = HALF,
  height = H,
  blend = "srgb",
  visible = true,
  root = design("srgb (browser, Qt)"),
}
