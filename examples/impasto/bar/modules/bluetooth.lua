-- Bluetooth: the chip names the connected device, so audio that fell back
-- to the speakers shows at a glance. Opened from the bar it is the control
-- centre's device list; the card of BluetoothModule.qml is kept as `card`.

local ui = require("morf.ui")
local theme = require("theme")
local modules = require("services.modules")
local bluetooth = require("services.bluetooth")
local controls = require("components.controls")
local detail = require("bar.modules.detail")
local network_module = require("bar.modules.network")

local C = theme.color
local M = {}

function M.card()
  return detail.card("bluetooth", {
    mark = controls.ring_glyph {
      size = 44, thickness = 2.5, progress = 0, track_color = C.indicatorDim,
      glyph = bluetooth.icon, glyph_size = 18,
      glyph_color = function() return bluetooth.enabled() and C.indicator or C.textMuted() end,
    },
    title = bluetooth.summary,
    subtitle = function()
      if not bluetooth.enabled() then return "Adapter off" end
      local count = bluetooth.connected_count()
      if count == 0 then return "On · nothing connected" end
      return count == 1 and "On · 1 device connected" or ("On · " .. count .. " devices connected")
    end,
    pill_width = 90,
    figures = {
      { label = "DEVICES", value = function() return tostring(bluetooth.connected_count()) end,
        note = "pairing lives in the control centre" },
    },
    pill = controls.pill { text = "Bluetooth", width = 90, active = bluetooth.enabled,
      on_click = bluetooth.toggle },
  })
end

modules.define("bluetooth", {
  glyph = bluetooth.icon,
  value = bluetooth.summary,
  has = bluetooth.available,
  detail = function()
    local w, h = modules.open_size("bluetooth")
    local margin = network_module.MENU_MARGIN
    return ui.Inset {
      anchors = { fill = true }, margin = margin,
      require("bar.panels.bluetooth").build {
        backable = false,
        width = w - 8 - 2 * margin, height = h - 8 - 2 * margin,
      },
    }
  end,
})

return M
