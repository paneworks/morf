-- Network: the chip is the link's glyph and the network's name. Opened
-- from the bar it shows the control centre's Wi-Fi list (DetailFace.qml);
-- the card of NetworkModule.qml -- link type, reachability, the radio
-- switch -- is kept as `card` for the desktop's face.

local ui = require("morf.ui")
local theme = require("theme")
local modules = require("services.modules")
local network = require("services.network")
local controls = require("components.controls")
local detail = require("bar.modules.detail")

local C = theme.color
local M = {}

-- A menu is laid out for a panel's margins; the island gives a detail 4.
M.MENU_MARGIN = theme.panel_padding - 4

function M.card()
  morf.timer(1, network.refresh, false)
  return detail.card("network", {
    mark = controls.ring_glyph {
      size = 44, thickness = 2.5, progress = 0, track_color = C.indicatorDim,
      glyph = network.icon, glyph_size = 18,
      glyph_color = function() return network.online() and C.indicator or C.textMuted() end,
    },
    title = network.connection_name,
    subtitle = network.state_line,
    pill_width = 64,
    figures = {
      { label = "WI-FI", value = function() return network.radio_on() and "On" or "Off" end,
        note = function() return network.wifi_connected() and "connected" or "" end },
      { label = "LINK",
        value = function()
          if network.wired_connected() then return "Wired" end
          return network.wifi_connected() and "Wireless" or "None"
        end,
        note = function() return network.online() and "internet reached" or "" end },
    },
    pill = controls.pill { text = "Wi-Fi", width = 64, active = network.radio_on,
      on_click = network.toggle_wifi },
  })
end

modules.define("network", {
  glyph = network.icon,
  value = network.connection_name,
  detail = function()
    -- A fresh reading as it opens (NetworkModule.qml:26).
    morf.timer(1, network.refresh, false)
    local w, h = modules.open_size("network")
    return ui.Inset {
      anchors = { fill = true }, margin = M.MENU_MARGIN,
      require("bar.panels.wifi").build {
        backable = false,
        width = w - 8 - 2 * M.MENU_MARGIN, height = h - 8 - 2 * M.MENU_MARGIN,
      },
    }
  end,
})

return M
