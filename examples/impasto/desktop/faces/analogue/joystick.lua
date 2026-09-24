-- An arcade stick with an accent ball and two buttons. Static: it is the
-- arcade's mark.
--
-- Port of Joystick.qml, as one document.

local ui = require("morf.ui")
local svg = require("desktop.faces.analogue.svg")

local M = {}
local n = svg.n

function M.build(values)
  local s, ink = values.size, values.ink
  return ui.Image {
    x = values.x, y = values.y, width = s, height = s,
    source = function()
      local raised, border = ink.raised(), ink.border()
      local text, muted, accent = svg.hex(ink.text()), svg.hex(ink.muted()), svg.hex(ink.accent())
      return svg.cached(table.concat({ "stick", s, svg.hex(raised), text, muted, accent }, ":"), function()
        return svg.doc(s, s, table.concat {
          string.format('<rect x="%s" y="%s" width="%s" height="%s" rx="8" %s %s stroke-width="1"/>',
            n(s * 0.08), n(s * 0.6), n(s * 0.84), n(s * 0.28), svg.paint("fill", raised), svg.paint("stroke", border)),
          string.format('<rect x="%s" y="%s" width="6" height="%s" rx="3" fill="%s" transform="rotate(-10 %s %s)"/>',
            n(s * 0.34 - 3), n(s * 0.28), n(s * 0.36), text, n(s * 0.34), n(s * 0.64)),
          string.format('<circle cx="%s" cy="%s" r="%s" fill="%s"/>', n(s * 0.28), n(s * 0.26), n(s * 0.1), accent),
          string.format('<circle cx="%s" cy="%s" r="%s" fill="%s"/>', n(s * 0.65), n(s * 0.72), n(s * 0.05), text),
          string.format('<circle cx="%s" cy="%s" r="%s" fill="%s"/>', n(s * 0.8), n(s * 0.72), n(s * 0.05), muted),
        })
      end)
    end,
  }
end

return M
