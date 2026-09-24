-- A record on a turntable: a grooved disc, the artwork on the label, the
-- spindle, and a tonearm that is down while playing and lifted otherwise.
-- The disc spins during playback and stops where it is.
--
-- Port of Record.qml. The disc is always the island's black, whatever the
-- palette. The spin is a turn every 1.8 s (the original's), played by the
-- engine one turn at a time while a timer says it is playing; the arm is
-- its own document turning about its pivot.

local ui = require("morf.ui")
local theme = require("theme")
local common = require("desktop.faces.common")
local svg = require("desktop.faces.analogue.svg")

local C = theme.color
local M = {}
local n = svg.n
local TURN = 1800

local function disc_doc(d, ink)
  local dim, text = svg.hex(ink.dim()), svg.hex(ink.text())
  return svg.cached(table.concat({ "disc", d, dim, text }, ":"), function()
    local r = d / 2
    local parts = {
      string.format('<circle cx="%s" cy="%s" r="%s" fill="%s" stroke="%s" stroke-width="1"/>', n(r), n(r), n(r - 0.5), svg.hex(C.island), dim),
    }
    for i = 0, 6 do
      parts[#parts + 1] = string.format('<circle cx="%s" cy="%s" r="%s" fill="none" stroke="%s" stroke-opacity="0.09" stroke-width="1"/>',
        n(r), n(r), n(r * 0.45 + i * r * 0.5 / 7), text)
    end
    return svg.doc(d, d, table.concat(parts))
  end)
end

local function arm_doc(s, ink)
  local text, raised, muted = svg.hex(ink.text()), ink.raised(), svg.hex(ink.muted())
  return svg.cached(table.concat({ "arm", s, text, svg.hex(raised), muted }, ":"), function()
    -- Drawn with the pivot at (s, s): the node is 2s square and turns about
    -- its middle.
    local p = s
    return svg.doc(2 * s, 2 * s, table.concat {
      string.format('<rect x="%s" y="%s" width="4" height="%s" rx="2" fill="%s"/>', n(p - 2), n(p), n(s * 0.5), text),
      string.format('<rect x="%s" y="%s" width="12" height="16" rx="3" fill="%s"/>', n(p - 6), n(p + s * 0.5 - 4), text),
      string.format('<circle cx="%s" cy="%s" r="6" %s stroke="%s" stroke-width="2"/>', n(p), n(p), svg.paint("fill", raised), muted),
    })
  end)
end

--- `values`: `size`, `ink`, `playing` and `art` (functions).
function M.build(values)
  local s, ink = values.size, values.ink
  local r = s * 0.42
  local cx, cy = s * 0.46, s * 0.52
  local label = r * 0.72
  local function playing() return common.read(values.playing) and true or false end
  local disc
  local function spin()
    morf.animation.play {
      { node = disc, property = "rotation", duration = TURN,
        keyframes = { { at = 0, value = 0 }, { at = 1, value = 360, easing = "linear" } } },
    }
  end
  disc = ui.Item {
    x = cx - r, y = cy - r, width = 2 * r, height = 2 * r,
    ui.Image { width = 2 * r, height = 2 * r, source = function() return disc_doc(2 * r, ink) end },
    ui.ClipRect {
      x = r - label / 2, y = r - label / 2, width = label, height = label, radius = label / 2,
      color = ink.accent,
      ui.Image { anchors = { fill = true }, fill_mode = "preserve_aspect_crop",
        source = function() return common.read(values.art) or "" end,
        visible = function() return (common.read(values.art) or "") ~= "" end },
    },
    ui.Rect { x = r - 3, y = r - 3, width = 6, height = 6, radius = 3, color = C.island },
    ui.Timer { interval = TURN, ["repeat"] = true, running = playing, on_triggered = spin },
  }
  return ui.Item {
    x = values.x, y = values.y, width = s, height = s,
    disc,
    ui.Image {
      x = s * 0.9 - s, y = s * 0.1 - s, width = 2 * s, height = 2 * s,
      source = function() return arm_doc(s, ink) end,
      rotation = function() return playing() and 32 or 10 end,
      behavior = { rotation = { duration = math.max(1, theme.duration_medium() * 2), easing = theme.easing() } },
    },
  }
end

return M
