-- The bar's popouts: hovering a status icon grows a panel out of the bar
-- beside it -- the network (Wi-Fi on or off, the networks, a rescan), the
-- Bluetooth (on or off, discovery, the devices) and the power (the battery,
-- the power profile and a picker for it). Moving to another icon turns the
-- panel into that one's: it eases to the new size and place while the
-- contents cross-fade. Items without a popout (the clock, the workspaces)
-- keep the one that is open; leaving the bar and the panel shuts it.
--
-- Measured off the reference at 1920x1080: the network panel 352 x 188,
-- Bluetooth 332 x 228, power 280 x 148, each centred on its icon and kept
-- inside the frame; contents 18 px in; 32 px pill buttons.

local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local drawer = require("drawer")
local services = require("services")

local C = theme.color
local M = {}

M.current = morf.signal("caelestia.popout", "")
-- Where each icon's centre is, measured up from the frame's bottom edge
-- (the bar's status block sits at a fixed distance from it).
local CENTRE = { network = 126, bluetooth = 96, power = 66 }

local PAD = 18
local ROW = 36
local MAX_ROWS = 4
local motion = { duration = theme.duration.normal, easing = theme.ease.emphasized_decel }

-- Every pointer area of the popouts and the bar's items that keep them.
local areas = {}
local function area(props)
  local a = ui.MouseArea(props)
  areas[#areas + 1] = a
  return a
end
M.area = area

-- ----------------------------------------------------------------- network --

local function net() return services.net end

local function networks()
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

local function title(text, y)
  return kit.text { x = PAD, y = y, text = text, font_size = theme.size.large - 1, font_weight = 500 }
end

local function setting(label, y, on, toggled, id, w)
  return ui.Item {
    x = PAD, y = y, width = w - 2 * PAD, height = 32,
    kit.text { anchors = { vertical_center = true }, text = label, font_size = theme.size.normal },
    kit.switch { id = id, anchors = { right = true, vertical_center = true }, on = on, on_toggled = toggled },
  }
end

local function list_rows(prefix, count_fn, row_fn, y, w)
  local rows = {}
  for i = 1, MAX_ROWS do
    rows[#rows + 1] = ui.Item {
      id = prefix .. i, width = w - 2 * PAD, height = ROW,
      visible = function() return i <= count_fn() end,
      row_fn(i),
    }
  end
  return ui.Column { x = PAD, y = y, gap = 0, table.unpack(rows) }
end

local NET_W = 352
local function net_rows() return math.min(MAX_ROWS, #networks()) end
local function net_height() return 188 + ROW * net_rows() end

local function network_page()
  return ui.Item {
    id = "popout-network",
    width = NET_W, height = net_height,
    title("Wireless", 28),
    setting("Enabled", 58, function()
      local n = net()
      return n ~= nil and n.state.available and n.state.wifi_enabled == true
    end, function(now)
      local n = net()
      if n then pcall(n.set_wifi, now) end
    end, "popout-wifi", NET_W),
    kit.text {
      id = "popout-network-count",
      x = PAD, y = 102,
      text = function()
        local count = #networks()
        return ("%d network%s available"):format(count, count == 1 and "" or "s")
      end,
      font_size = theme.size.normal,
    },
    list_rows("popout-network-", net_rows, function(i)
      local function ap() return networks()[i] or {} end
      return area {
        anchors = { fill = true }, cursor = "pointer",
        on_clicked = function()
          local n, a = net(), ap()
          if n and a.ssid and not a.in_use then pcall(n.connect, a) end
        end,
        ui.Row {
          anchors = { vertical_center = true }, gap = 10, align = "center",
          kit.icon(function() return signal_icon(ap().strength) end, 20, function()
            return ap().in_use and C.primary or C.onSurfaceVariant
          end),
          kit.text {
            width = NET_W - 2 * PAD - 60, elide = "right",
            text = function() return ap().ssid or "" end,
            color = function() return ap().in_use and C.primary or C.onSurface end,
          },
          kit.icon(function() return ap().secure and "lock" or "" end, 16, function() return C.onSurfaceVariant end),
        },
      }
    end, 132, NET_W),
    kit.pill {
      id = "popout-rescan",
      x = PAD, width = NET_W - 2 * PAD,
      y = function() return 140 + ROW * net_rows() end,
      icon = "wifi_find", label = "Rescan networks",
      on_clicked = function()
        local n = net()
        if n then pcall(n.request_scan) end
      end,
    },
  }
end

-- --------------------------------------------------------------- bluetooth --

local function bt() return services.bt end

local function devices()
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

local BT_W = 332
local function bt_rows() return math.min(MAX_ROWS, #devices()) end
local function bt_height() return 228 + ROW * bt_rows() end

local function bluetooth_page()
  return ui.Item {
    id = "popout-bluetooth",
    width = BT_W, height = bt_height,
    title("Bluetooth", 27),
    setting("Enabled", 57, function()
      local b = bt()
      return b ~= nil and b.state.available and b.state.powered == true
    end, function(now)
      local b = bt()
      if b then pcall(b.set_powered, now) end
    end, "popout-bluetooth-power", BT_W),
    setting("Discovering", 95, function()
      local b = bt()
      return b ~= nil and b.state.available and b.state.discovering == true
    end, function(now)
      local b = bt()
      if b then pcall(now and b.start_discovery or b.stop_discovery) end
    end, "popout-bluetooth-discover", BT_W),
    kit.text {
      id = "popout-bluetooth-count",
      x = PAD, y = 141,
      text = function()
        local count = #devices()
        return ("%d device%s available"):format(count, count == 1 and "" or "s")
      end,
      font_size = theme.size.normal,
    },
    list_rows("popout-device-", bt_rows, function(i)
      local function dev() return devices()[i] or {} end
      return area {
        anchors = { fill = true }, cursor = "pointer",
        on_clicked = function()
          local b, d = bt(), dev()
          if not b or not d.path then return end
          pcall(d.connected and b.disconnect or b.connect, d)
        end,
        ui.Row {
          anchors = { vertical_center = true }, gap = 10, align = "center",
          kit.icon(function() return dev().connected and "bluetooth_connected" or "bluetooth" end, 20,
            function() return dev().connected and C.primary or C.onSurfaceVariant end),
          kit.text {
            width = BT_W - 2 * PAD - 40, elide = "right",
            text = function() local d = dev() return d.alias or d.name or d.address or "" end,
            color = function() return dev().connected and C.primary or C.onSurface end,
          },
        },
      }
    end, 172, BT_W),
    kit.pill {
      id = "popout-bluetooth-settings",
      x = PAD, width = BT_W - 2 * PAD,
      y = function() return 183 + ROW * bt_rows() end,
      icon = "settings", label = "Open settings",
      on_clicked = function()
        local argv = require("config").get("bar.bluetooth_settings")
        if type(argv) == "table" and #argv > 0 then
          pcall(morf.spawn, { command = argv })
          M.current:set("")
        end
      end,
    },
  }
end

-- ------------------------------------------------------------------- power --

local function up() return services.upower end

local PROFILES = {
  { id = "power-saver", icon = "energy_savings_leaf", name = "Power saver" },
  { id = "balanced", icon = "balance", name = "Balanced" },
  { id = "performance", icon = "rocket_launch", name = "Performance" },
}

local POWER_W = 280
local function battery()
  local u = up()
  local d = u and u.state.available and u.state.display or {}
  return d.present and d or nil
end

local function power_page()
  local buttons = {}
  for _, p in ipairs(PROFILES) do
    local function on()
      local u = up()
      local active = u and u.state.available and u.state.profiles.active or ""
      -- Without power-profiles-daemon the machine runs balanced, and the
      -- reference shows it so.
      if active == "" then active = "balanced" end
      return active == p.id
    end
    buttons[#buttons + 1] = area {
      id = "popout-profile-" .. p.id,
      width = 64, height = 46, cursor = "pointer",
      on_clicked = function()
        local u = up()
        if u then pcall(u.set_profile, p.id) end
      end,
      ui.Rect {
        anchors = { center_in = true }, width = 46, height = 46, radius = 23,
        color = function() return on() and C.primary or C.primary:alpha(0) end,
        behavior = { color = { duration = theme.duration.small } },
      },
      kit.icon(p.icon, 26, function() return on() and C.onPrimary or C.onSurface end, { anchors = { center_in = true } }),
    }
  end
  return ui.Item {
    id = "popout-power",
    width = POWER_W, height = 148,
    ui.Column {
      x = PAD, y = 14, gap = 10,
      kit.text {
        id = "popout-battery",
        text = function()
          local b = battery()
          if not b then return "No battery detected" end
          local pct = math.floor((b.percentage or 0) + 0.5)
          local how = b.charging and "charging" or "discharging"
          return ("Battery: %d%% (%s)"):format(pct, how)
        end,
        font_size = theme.size.normal,
      },
      kit.text {
        id = "popout-profile",
        text = function()
          local u = up()
          local active = u and u.state.available and u.state.profiles.active or ""
          for _, p in ipairs(PROFILES) do
            if p.id == active then return "Power profile: " .. p.name end
          end
          return "Power profile: Balanced"
        end,
        font_size = theme.size.normal,
      },
    },
    ui.Rect {
      anchors = { horizontal_center = true }, y = 81, width = 200, height = 52, radius = 26,
      color = function() return C.surfaceContainer end,
      ui.Row { anchors = { center_in = true }, gap = 0, table.unpack(buttons) },
    },
  }
end

-- ------------------------------------------------------------------- panel --

local pages = {
  network = { node = network_page(), width = function() return NET_W end, height = net_height },
  bluetooth = { node = bluetooth_page(), width = function() return BT_W end, height = bt_height },
  power = { node = power_page(), width = function() return POWER_W end, height = function() return 148 end },
}

-- The page last shown stays drawn while the panel shuts.
local last = morf.signal("caelestia.popout.last", "network")
morf.effect("caelestia.popout.last", function()
  local c = M.current:get()
  if c ~= "" then last:set(c) end
end)
local function page() return pages[last:get()] or pages.network end

local stack = { anchors = { fill = true } }
for name, p in pairs(pages) do
  -- The contents cross-fade as the panel turns into another's.
  stack[#stack + 1] = ui.Item {
    anchors = { fill = true },
    opacity = function() return last:get() == name and 1 or 0 end,
    -- Hidden, a page takes no pointer.
    visible = function() return last:get() == name end,
    behavior = { opacity = { duration = theme.duration.small, easing = theme.ease.standard } },
    p.node,
  }
end
local background = area { anchors = { fill = true }, z = -1 }
stack[#stack + 1] = background

M.drawer = drawer.new {
  name = "popout",
  edge = "left",
  width = function() return page().width() end,
  height = function() return page().height() end,
  content = ui.Item(stack),
  props = {
    anchors = { left = true, bottom = true },
    -- Centred on the icon, kept inside the frame.
    translate_y = function()
      local h = page().height()
      return -math.max(0, (CENTRE[last:get()] or 0) - h / 2)
    end,
    behavior = { width = motion, height = motion, translate_y = motion },
  },
}

morf.effect("caelestia.popout.open", function()
  M.drawer.set(M.current:get() ~= "")
end)

-- ------------------------------------------------------------------- hover --

local function over()
  for _, a in ipairs(areas) do
    if a.hovered then return true end
  end
  return false
end

--- Wraps a bar item in an area that opens popout `name` on hover (or,
--- with no name, keeps the one open).
function M.trigger(name, child, props)
  props = props or {}
  props.width = props.width or child.width
  props.height = props.height or child.height
  props[#props + 1] = child
  local a = area(props)
  morf.effect("caelestia.popout.trigger." .. (props.id or tostring(#areas)), function()
    if a.hovered and name then M.current:set(name) end
  end)
  return a
end

local closing
morf.effect("caelestia.popout.leave", function()
  if M.current:get() == "" then return end
  if over() then
    if closing then closing:cancel() closing = nil end
    return
  end
  if closing then closing:cancel() end
  -- A moment's grace for the pointer crossing from the bar to the panel.
  closing = morf.timer(120, function()
    closing = nil
    if not over() then M.current:set("") end
  end, false)
end)

return M
