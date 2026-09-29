local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local osk = require("lib.osk")
local C = theme.color
local V = {}
function V.build(model)
  local W, PAD = 1060, 12
  local kb = osk.new {
    prefix = "caelestia.osk", action = kit.action, width = W - 2 * PAD,
    mode = "full", numbers = false, send = model.send, active = model.active,
    look = {
      panel = function() return C.surfaceContainer:alpha(0) end,
      key = function() return C.surfaceContainerHighest end,
      key_dim = function() return C.surfaceContainerHigh end,
      accent = function() return C.primary end, on_accent = function() return C.onPrimary end,
      text = function() return C.onSurface end, dim = function() return C.onSurfaceVariant end,
      press = function() return C.secondaryContainer end,
      font = theme.font, radius = theme.key_radius, icons = theme.icon_font,
    },
  }
  return {width = W, height = function() return kb.height() + 2 * PAD end, keys = kb,
    content = ui.Item {x = PAD, y = PAD, width = W - 2 * PAD, height = kb.height, kb.node}}
end
return V
