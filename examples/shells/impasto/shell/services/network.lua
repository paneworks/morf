-- The network: what carries the traffic, how strong the Wi-Fi is, the radio
-- switch and the list of networks in range.
--
-- Port of NetworkService.qml. The original read NetworkManager through
-- Quickshell and shelled out to nmcli and a Python helper for the radio and
-- the list; here all of it is `lib.networkmanager`, one D-Bus client whose
-- state follows NetworkManager's own signals. Nothing polls.
--
-- The actions are wired to clicks only, and pass through `services.act`.

local nm = require("lib.services.networkmanager")
local act = require("services.act")

local M = {}

local net = nm.connect()
M.lib = net
M.state = net.state

local s = net.state

M.busy_ssid = morf.signal("impasto.network.busy", "")
M.scanning = morf.signal("impasto.network.scanning", false)

function M.available() return s.available end
function M.radio_on() return s.wifi_enabled end
M.wifi_enabled = M.radio_on
function M.wifi_connected() return s.wifi.connected == true end
function M.wired_connected() return s.wired.connected == true end
function M.online() return s.connectivity == "full" end

--- The name of whatever carries the traffic. A cable outranks Wi-Fi, which
--- is the order NetworkManager routes them in anyway.
function M.connection_name()
  if M.wired_connected() then return "Wired" end
  if M.wifi_connected() and (s.wifi.ssid or "") ~= "" then return s.wifi.ssid end
  if not s.wifi_enabled then return "Off" end
  return (s.wifi.device or "") ~= "" and "Not connected" or "Unavailable"
end

--- Signal of the connected Wi-Fi network, 0..1.
function M.strength()
  if not M.wifi_connected() then return 0 end
  local raw = tonumber(s.wifi.strength) or 0
  return raw > 1 and raw / 100 or raw
end

--- The link type and state, under the connection name.
function M.state_line()
  if M.wired_connected() then return M.online() and "Ethernet · online" or "Ethernet · no internet" end
  if M.wifi_connected() then return M.online() and "Wi-Fi · online" or "Wi-Fi · no internet" end
  if not s.wifi_enabled then return "Wi-Fi radio off" end
  return M.online() and "Online" or "Nothing reaches the internet"
end

function M.icon()
  if M.wired_connected() then return "󰈁" end
  if not s.wifi_enabled then return "󰤮" end
  return M.wifi_connected() and "󰤨" or "󰤯"
end

function M.strength_icon(signal)
  signal = tonumber(signal) or 0
  if signal >= 75 then return "󰤨" end
  if signal >= 50 then return "󰤥" end
  if signal >= 25 then return "󰤢" end
  return "󰤟"
end

--- The networks in range, a list model keyed by SSID: bind a Repeater to it.
M.networks = s.access_points

-- ---------------------------------------------------------------- actions --

--- Reads again what NetworkManager already knows; asks for nothing.
function M.refresh() net.refresh() end

--- `rescan` makes the card sweep the band, which takes seconds; without it
--- the list is what NetworkManager already has.
function M.scan(rescan)
  if not rescan then net.refresh() return end
  M.scanning:set(true)
  act.run("rescanning Wi-Fi", net.request_scan)
  morf.timer(4000, function() M.scanning:set(false) end, false)
end

-- A row says "Working…" for as long as its operation runs: until the
-- library answers (a disconnect, a forget, a refused connection), or for a
-- connection until NetworkManager reports that network connected -- the
-- answer to `connect` only says the activation has started. A generation
-- keeps a late answer from clearing a newer operation's row; nothing waits
-- longer than a minute.
local operation = 0
local waiting_for = nil

local function finish(mine)
  if mine ~= operation then return end
  waiting_for = nil
  M.busy_ssid:set("")
end

local function busy(ssid, what, fn, arguments, settles_on_connect)
  operation = operation + 1
  local mine = operation
  M.busy_ssid:set(ssid or "")
  waiting_for = nil
  local function done(result, err)
    if err or result == nil or not settles_on_connect then return finish(mine) end
    if M.wifi_connected() and s.wifi.ssid == ssid then return finish(mine) end
    waiting_for = { ssid = ssid, operation = mine }
  end
  -- The library's `done` goes after the arguments, which may hold nils.
  arguments[arguments.n + 1] = done
  local ok, err = act.run(what, fn, table.unpack(arguments, 1, arguments.n + 1))
  -- A dry run (or a call that never went out) has nothing to wait for.
  if act.dry or not ok then morf.timer(1, function() finish(mine) end, false) end
  morf.timer(60000, function() finish(mine) end, false)
  return ok, err
end

morf.effect("impasto.network.settled", function()
  local connected, ssid = s.wifi.connected == true, s.wifi.ssid
  local wanted = waiting_for
  if wanted and connected and ssid == wanted.ssid then
    morf.timer(1, function() finish(wanted.operation) end, false)
  end
end)

function M.connect(row_or_ssid, password)
  local ssid = type(row_or_ssid) == "table" and row_or_ssid.ssid or row_or_ssid
  return busy(ssid, "connecting to " .. tostring(ssid), net.connect,
    { n = 3, row_or_ssid, (password ~= "" and password) or nil, nil }, true)
end

function M.disconnect(ssid)
  return busy(ssid, "disconnecting Wi-Fi", net.disconnect, { n = 1, nil })
end

function M.forget(ssid)
  return busy(ssid, "forgetting " .. tostring(ssid), net.forget, { n = 1, ssid })
end

function M.set_wifi(on)
  return act.run(on and "turning Wi-Fi on" or "turning Wi-Fi off", net.set_wifi, on and true or false)
end

function M.toggle_wifi() return M.set_wifi(not s.wifi_enabled) end

return M
