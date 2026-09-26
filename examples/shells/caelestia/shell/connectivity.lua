-- The right panel's Network and Bluetooth pages: what the bar's popouts
-- had, given the panel's height. Network: Wi-Fi on or off, the networks
-- (the one in use first, then by strength; a click joins one) and a
-- rescan. Bluetooth: on or off, discovery, the devices (connected, then
-- paired, then by name; a click connects or disconnects) and the settings
-- program. Both through lib/networkmanager.lua and lib/bluez.lua (see
-- services.lua); without them each says so. CAELESTIA_DRY_RUN=1 logs what
-- would be done instead.

local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local services = require("services")

local C = theme.color
local M = {}

local PAD = 16
local ROW = 44
local MAX_ROWS = 20

local function dry_run()
  local v = morf.env and morf.env("CAELESTIA_DRY_RUN")
  return v ~= nil and v ~= "" and v ~= "0"
end

--- Calls `fn` unless in a dry run, when it logs `what`.
local function act(what, fn, ...)
  if dry_run() then
    morf.log("info", "caelestia: " .. what .. " (dry run)")
    return
  end
  pcall(fn, ...)
end

local function setting(id, label, y, w, on, toggled)
  return ui.Item {
    x = PAD, y = y, width = w - 2 * PAD, height = 36,
    kit.text { anchors = { vertical_center = true }, text = label, font_size = theme.size.normal },
    kit.switch { id = id, anchors = { right = true, vertical_center = true }, on = on, on_toggled = toggled },
  }
end

--- Up to as many rows as fit between `top` and `bottom` of the page, each
--- shown while `count()` reaches it.
local function rows(prefix, w, h, top, bottom, count, row)
  local list = {}
  local function fits() return math.max(0, math.floor((h() - top - bottom) / ROW)) end
  for i = 1, MAX_ROWS do
    list[i] = ui.Item {
      id = prefix .. i, width = w - 2 * PAD, height = ROW,
      visible = function() return i <= math.min(count(), fits()) end,
      row(i),
    }
  end
  return ui.Column { x = PAD, y = top, gap = 0, table.unpack(list) }
end

--- A row's look: a rounded wash on hover, stronger when it is the one in
--- use.
local function row_area(props, on)
  local area
  local children = props
  props.cursor = "pointer"
  props.anchors = { fill = true }
  area = ui.MouseArea(props)
  ui.reparent(ui.Rect {
    anchors = { fill = true, top_margin = 2, bottom_margin = 2 }, z = -1,
    radius = function() return on() and 12 or 20 end,
    color = function()
      if on() then return C.secondaryContainer end
      return area.hovered and C.onSurface:alpha(0.06) or C.onSurface:alpha(0)
    end,
    behavior = { color = { duration = theme.duration.small }, radius = kit.spring(260, 16) },
  }, area)
  return area, children
end

-- ----------------------------------------------------------------- network --

local function net() return services.net end

function M.networks()
  local n = net()
  if not n or not n.state.available then return {} end
  local model = n.state.access_points
  local list = {}
  for i = 1, model:len() do list[#list + 1] = model:get(i) end
  table.sort(list, function(a, b)
    if a.in_use ~= b.in_use then return a.in_use end
    return (a.strength or 0) > (b.strength or 0)
  end)
  return list
end

local function signal_icon(strength)
  strength = strength or 0
  if strength >= 80 then return "signal_wifi_4_bar" end
  if strength >= 55 then return "network_wifi_3_bar" end
  if strength >= 30 then return "network_wifi_2_bar" end
  if strength >= 10 then return "network_wifi_1_bar" end
  return "signal_wifi_0_bar"
end

function M.network_page(w, h)
  local function available() local n = net() return n ~= nil and n.state.available end
  return kit.card {
    id = "network-page",
    width = w, height = h, clip = true,
    kit.text { x = PAD, y = PAD, text = "Wi-Fi", font_size = theme.size.large, font_weight = 500 },
    setting("network-wifi", "Enabled", PAD + 34, w, function()
      local n = net()
      return n ~= nil and n.state.available and n.state.wifi_enabled == true
    end, function(now)
      local n = net()
      if n then act("wifi " .. (now and "on" or "off"), n.set_wifi, now) end
    end),
    kit.text {
      id = "network-count",
      x = PAD, y = PAD + 84,
      text = function()
        if not available() then return "No network manager" end
        local count = #M.networks()
        return ("%d network%s available"):format(count, count == 1 and "" or "s")
      end,
      font_size = theme.size.normal,
      color = function() return C.onSurfaceVariant end,
    },
    rows("network-row-", w, h, PAD + 116, 76, function() return #M.networks() end, function(i)
      local function ap() return M.networks()[i] or {} end
      local function on() return ap().in_use == true end
      local area = row_area({
        on_clicked = function()
          local n, a = net(), ap()
          if n and a.ssid and not a.in_use then act("connect " .. a.ssid, n.connect, a) end
        end,
      }, on)
      ui.reparent(ui.Row {
        x = 12, anchors = { vertical_center = true }, gap = 12, align = "center",
        kit.icon(function() return signal_icon(ap().strength) end, 22, function()
          return on() and C.onSecondaryContainer or C.onSurfaceVariant
        end),
        kit.text {
          width = w - 2 * PAD - 90, elide = "right",
          text = function() return ap().ssid or "" end,
          color = function() return on() and C.onSecondaryContainer or C.onSurface end,
        },
        kit.icon(function() return ap().secure and "lock" or "" end, 16, function() return C.onSurfaceVariant end),
      }, area)
      return area
    end),
    kit.pill {
      id = "network-rescan",
      x = PAD, width = w - 2 * PAD,
      y = function() return h() - PAD - 44 end,
      icon = "wifi_find", label = "Rescan networks",
      on_clicked = function()
        local n = net()
        if n then act("rescan networks", n.request_scan) end
      end,
    },
  }
end

-- --------------------------------------------------------------- bluetooth --

local function bt() return services.bt end

function M.devices()
  local b = bt()
  if not b or not b.state.available then return {} end
  local model = b.state.devices
  local list = {}
  for i = 1, model:len() do
    local d = model:get(i)
    if d.named ~= false then list[#list + 1] = d end
  end
  table.sort(list, function(a, c)
    if a.connected ~= c.connected then return a.connected end
    if a.paired ~= c.paired then return a.paired end
    return (a.alias or a.name or "") < (c.alias or c.name or "")
  end)
  return list
end

function M.bluetooth_page(w, h)
  local function available() local b = bt() return b ~= nil and b.state.available end
  return kit.card {
    id = "bluetooth-page",
    width = w, height = h, clip = true,
    kit.text { x = PAD, y = PAD, text = "Bluetooth", font_size = theme.size.large, font_weight = 500 },
    setting("bluetooth-power", "Enabled", PAD + 34, w, function()
      local b = bt()
      return b ~= nil and b.state.available and b.state.powered == true
    end, function(now)
      local b = bt()
      if b then act("bluetooth " .. (now and "on" or "off"), b.set_powered, now) end
    end),
    setting("bluetooth-discover", "Discovering", PAD + 74, w, function()
      local b = bt()
      return b ~= nil and b.state.available and b.state.discovering == true
    end, function(now)
      local b = bt()
      if b then act("bluetooth discovery " .. (now and "on" or "off"), now and b.start_discovery or b.stop_discovery) end
    end),
    kit.text {
      id = "bluetooth-count",
      x = PAD, y = PAD + 124,
      text = function()
        if not available() then return "No Bluetooth adapter" end
        local count = #M.devices()
        return ("%d device%s"):format(count, count == 1 and "" or "s")
      end,
      font_size = theme.size.normal,
      color = function() return C.onSurfaceVariant end,
    },
    rows("bluetooth-row-", w, h, PAD + 156, 76, function() return #M.devices() end, function(i)
      local function dev() return M.devices()[i] or {} end
      local function on() return dev().connected == true end
      local area = row_area({
        on_clicked = function()
          local b, d = bt(), dev()
          if not b or not d.path then return end
          local name = d.alias or d.name or d.address or "?"
          act((d.connected and "disconnect " or "connect ") .. name, d.connected and b.disconnect or b.connect, d)
        end,
      }, on)
      ui.reparent(ui.Row {
        x = 12, anchors = { vertical_center = true }, gap = 12, align = "center",
        kit.icon(function() return on() and "bluetooth_connected" or "bluetooth" end, 22,
          function() return on() and C.onSecondaryContainer or C.onSurfaceVariant end),
        kit.text {
          width = w - 2 * PAD - 110, elide = "right",
          text = function() local d = dev() return d.alias or d.name or d.address or "" end,
          color = function() return on() and C.onSecondaryContainer or C.onSurface end,
        },
        kit.text {
          text = function()
            local d = dev()
            if d.connected then return "Connected" end
            return d.paired and "Paired" or ""
          end,
          font_size = theme.size.small,
          color = function() return on() and C.onSecondaryContainer or C.onSurfaceVariant end,
        },
      }, area)
      return area
    end),
    kit.pill {
      id = "bluetooth-settings",
      x = PAD, width = w - 2 * PAD,
      y = function() return h() - PAD - 44 end,
      icon = "settings", label = "Open settings",
      on_clicked = function()
        local argv = require("config").get("bar.bluetooth_settings")
        if type(argv) == "table" and #argv > 0 then
          act("open " .. tostring(argv[1]), morf.spawn, { command = argv })
        end
      end,
    },
  }
end

return M
