-- The list behind the Bluetooth tile: BluetoothDetail.qml.
--
-- Connected devices first, then paired, then whatever is in range with a
-- name; devices without one are folded away rather than dropped. A click
-- connects or disconnects. Discovery runs only while this list is open: a
-- lingering scan costs power and floods the list. Pairing needs a PIN
-- agent, which the library does not register, so it is not offered.
--
-- Registered as the "bluetooth" panel and used again as the bluetooth
-- module's detail.

local ui = require("morf.ui")
local theme = require("theme")
local island = require("bar.island")
local kit = require("components.kit")
local controls = require("components.controls")
local bluetooth = require("services.bluetooth")
local wifi = require("bar.panels.wifi")

local C = theme.color
local M = {}

local WIDTH, HEIGHT = 420, 500
local ROW = 48

local function entry(row, width, show_unnamed)
  local device = controls.signal("bt.row", 0)
  local current = row
  local function field(name) device:get() return current[name] end
  local hovered = controls.signal("bt.hover", false)
  local listed = function()
    device:get()
    return bluetooth.listed(current) or show_unnamed:get()
  end
  local connected = function() return field("connected") == true end
  -- A loader, so a folded device takes no room at all: a hidden node keeps
  -- its room, and a size of zero means "no size".
  return ui.Loader {
   active = listed,
   source = function() return ui.Item {
    width = width, height = ROW + 4,
    ui.Rect {
      width = width, height = ROW, radius = theme.radius_medium,
      color = function() return (connected() or hovered:get()) and C.islandSurfaceHover or C.islandSurface end,
      border_width = 1,
      border_color = function() return connected() and C.accent() or C.islandBorder end,
      behavior = { color = theme.behave("fast") },
      ui.Row {
        anchors = { left = true, left_margin = 10, vertical_center = true },
        gap = 10, align = "center",
        kit.glyph {
          glyph = function() device:get() return bluetooth.device_icon(current) end, size = 15, width = 18,
          color = function() return connected() and C.accent() or C.textMuted() end,
        },
        ui.Column {
          gap = 0,
          kit.text {
            -- The address rather than BlueZ's alias for a device with no name.
            text = function()
              device:get()
              return bluetooth.is_named(current) and (current.alias ~= "" and current.alias or current.name)
                or current.address or ""
            end,
            size = theme.size.small, width = width - 70, elide = "right",
            weight = function() return connected() and 600 or 400 end,
            color = function() return connected() and C.accent() or C.text() end,
          },
          kit.text {
            text = function()
              device:get()
              local bits = { current.connected and "Connected" or (current.paired and "Paired" or "Available") }
              if (current.battery or -1) >= 0 then bits[#bits + 1] = current.battery .. "%" end
              return table.concat(bits, " · ")
            end,
            size = 9, color = C.textMuted, width = width - 70, elide = "right",
          },
        },
      },
      kit.glyph {
        anchors = { right = true, right_margin = 12, vertical_center = true },
        glyph = "󰄬", size = 13, color = C.accent, visible = connected,
      },
      ui.MouseArea {
        anchors = { fill = true }, cursor = "pointer",
        on_entered = function() hovered:set(true) end,
        on_exited = function() hovered:set(false) end,
        on_clicked = function() bluetooth.connect_device(current) end,
      },
    },
   } end,
  }, function(next)
    current = next
    device:set(device:get() + 1)
  end
end

function M.build(options)
  options = options or {}
  local width = options.width or (WIDTH - 2 * theme.panel_padding)
  local height = options.height or (HEIGHT - 2 * theme.panel_padding)
  local show_unnamed = controls.signal("bt.unnamed", false)
  if bluetooth.enabled() then bluetooth.set_discovering(true) end
  local list = ui.Repeater {
    as = "column", gap = 0,
    model = bluetooth.devices,
    delegate = function(row) return entry(row, width, show_unnamed) end,
  }
  local unnamed = function()
    local _ = list.layout_height
    local n = 0
    for _, device in ipairs(bluetooth.all_devices()) do
      if not bluetooth.listed(device) then n = n + 1 end
    end
    return n
  end
  local fold_hover = controls.signal("bt.fold", false)
  local fold_label = kit.text {
    text = function()
      local n = unnamed()
      return (show_unnamed:get() and "󰅃  " or "󰅀  ")
        .. (n == 1 and "1 unnamed device nearby" or (n .. " unnamed devices nearby"))
    end,
    size = theme.size.label,
    color = function() return fold_hover:get() and C.text() or C.textMuted() end,
  }
  return ui.Column {
    gap = 12,
    wifi.header({
      backable = options.backable,
      on_back = options.on_back,
      title = "Bluetooth",
      subtitle = function()
        if not bluetooth.enabled() then return "Adapter off" end
        return bluetooth.discovering() and "Looking for devices…" or bluetooth.summary()
      end,
      switch = controls.switch { checked = bluetooth.enabled,
        enabled = bluetooth.available,
        on_toggled = function(on)
          bluetooth.set_powered(on)
          bluetooth.set_discovering(on)
        end },
    }, width),
    ui.ClipRect {
      width = width, color = "#00000000",
      height = function() return height - 32 - 12 - (unnamed() > 0 and 30 or 0) end,
      kit.text {
        anchors = { center_in = true },
        visible = function() return (list.layout_height or 0) < 1 end,
        text = function() return bluetooth.enabled() and "Nothing found yet" or "Turn Bluetooth on to see devices" end,
        size = theme.size.small, color = C.textMuted,
      },
      list,
    },
    -- The fold for devices without a name.
    ui.Item {
      width = width, height = 18,
      visible = function() return unnamed() > 0 end,
      ui.Item {
        anchors = { center_in = true },
        width = function() return (fold_label.layout_width or 0) + 16 end, height = 18,
        ui.Item { anchors = { center_in = true }, width = function() return fold_label.layout_width or 0 end,
          height = 14, fold_label },
        ui.MouseArea {
          anchors = { fill = true }, cursor = "pointer",
          on_entered = function() fold_hover:set(true) end,
          on_exited = function() fold_hover:set(false) end,
          on_clicked = function() show_unnamed:set(not show_unnamed:get()) end,
        },
      },
    },
  }
end

-- Discovery stops when the list closes, whichever way it closes.
local was_open = false
morf.effect("impasto.bluetooth.discovery", function()
  local panel = island.state.open_panel()
  local open = panel == "bluetooth"
    or (panel == "module" and require("services.modules").open_id:get() == "bluetooth")
  if was_open and not open then bluetooth.set_discovering(false) end
  was_open = open
end)

island.register("bluetooth", {
  size = function() return WIDTH, HEIGHT end,
  build = function()
    return M.build { backable = true, on_back = function() island.open("controls") end }
  end,
})

return M
