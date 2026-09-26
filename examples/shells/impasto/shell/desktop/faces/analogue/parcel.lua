-- A box in outline, taped over the seam, with a paper label bearing the
-- count in the signature's script.
--
-- Port of Parcel.qml: the box as one document, the label a rotated paper
-- node with text on it, so the script is the shell's own font.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local common = require("desktop.faces.common")
local svg = require("desktop.faces.analogue.svg")

local C = theme.color
local M = {}
local n = svg.n

local function box_doc(s, ink)
  local text, muted = svg.hex(ink.text()), svg.hex(ink.muted())
  return svg.cached(table.concat({ "parcel", s, text, muted }, ":"), function()
    local cx, cy, w, d, tall = s / 2, s * 0.36, s * 0.42, s * 0.2, s * 0.4
    local function facet(points, shade)
      local out = {}
      for _, p in ipairs(points) do out[#out + 1] = n(p[1]) .. "," .. n(p[2]) end
      return string.format('<polygon points="%s" fill="%s" fill-opacity="%s" stroke="%s" stroke-width="2.5" stroke-linejoin="round"/>',
        table.concat(out, " "), text, n(shade), text)
    end
    return svg.doc(s, s, table.concat {
      facet({ { cx - w, cy }, { cx, cy - d }, { cx + w, cy }, { cx, cy + d } }, 0.18),
      facet({ { cx - w, cy }, { cx, cy + d }, { cx, cy + d + tall }, { cx - w, cy + tall } }, 0.05),
      facet({ { cx + w, cy }, { cx, cy + d }, { cx, cy + d + tall }, { cx + w, cy + tall } }, 0.11),
      string.format('<line x1="%s" y1="%s" x2="%s" y2="%s" stroke="%s" stroke-width="3"/>',
        n(cx - w / 2), n(cy - d / 2), n(cx + w / 2), n(cy + d / 2), muted),
      string.format('<line x1="%s" y1="%s" x2="%s" y2="%s" stroke="%s" stroke-width="3"/>',
        n(cx), n(cy + d), n(cx), n(cy + d + tall), muted),
    })
  end)
end

--- `values`: `size`, `ink`, `count` (a function returning text).
function M.build(values)
  local s, ink = values.size, values.ink
  local cx, cy, w, d = s / 2, s * 0.36, s * 0.42, s * 0.2
  return ui.Item {
    x = values.x, y = values.y, width = s, height = s,
    ui.Image { width = s, height = s, source = function() return box_doc(s, ink) end },
    ui.Rect {
      x = cx + 6, y = cy + d + 4, width = w * 0.72, height = w * 0.56, radius = 3, rotation = -6,
      color = function() return svg.paper(ink) end,
      kit.text { anchors = { fill = true }, horizontal_alignment = "center", vertical_alignment = "center",
        font_family = function() return theme.font_signature() end,
        size = math.floor(w * 0.42 + 0.5), color = C.paperInk,
        text = function() return tostring(common.read(values.count) or "") end },
    },
  }
end

return M
