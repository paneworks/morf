-- The utilities: the right panel's Settings page -- volume and brightness
-- sliders, a row of quick toggles (Wi-Fi, Bluetooth, the microphone,
-- settings, game mode, do not disturb), the battery and the power profile,
-- and keep awake (an idle inhibitor). Screenshots and the screen recorder
-- are the capture drawer's, at the bottom (capture.lua).
--
-- Every action that reaches outside the shell is a command from the
-- settings (`utilities.*`), run with `morf.run`; CAELESTIA_DRY_RUN=1 logs
-- it instead (the tests set it), keep awake included.
--
-- Measured off the reference at 1920x1080: 430 wide on the frame's right
-- corner, 451 tall; cards 408 wide, 16 in from the left and the top, 6
-- from the right and the bottom, 12 apart, rounded 15: keep awake 86 tall
-- (128 on, with an "Active since" chip), the recorder 208 (376 with its
-- list open), the toggles 111 (56 x 44 buttons, 8 apart: pills off,
-- rounded squares on).

local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local config = require("config")
local services = require("services")
local notifs = require("notifs")

local C = theme.color
local M = {}

M.WIDTH = 430
local LEFT, TOP, GAP, BOTTOM = 16, 16, 12, 6
local CARD_W = 408
M.RADIUS = 15
local motion = { duration = theme.duration.normal, easing = theme.ease.emphasized_decel }

-- ------------------------------------------------------------------ run --

local function dry_run()
  local v = morf.env and morf.env("CAELESTIA_DRY_RUN")
  return v ~= nil and v ~= "" and v ~= "0"
end
M.dry_run = dry_run

local function expand(words)
  local home = morf.fs.home()
  local stamp = morf.time.format("%Y%m%d_%H-%M-%S")
  local out = {}
  for i, w in ipairs(words) do
    out[i] = tostring(w):gsub("^~/", home .. "/"):gsub("%$HOME", home):gsub("%$DATE", stamp)
  end
  return out
end

--- Runs the command the settings keep under `utilities.commands.<name>`
--- (a list of words; `~/`, `$HOME` and `$DATE` expanded). Dry, it logs.
function M.run(name)
  local words = config.get("utilities.commands." .. name)
  if type(words) ~= "table" or #words == 0 then
    morf.log("info", "caelestia: utilities " .. name .. ": nothing to run")
    return false
  end
  local argv = expand(words)
  if dry_run() then
    morf.log("info", "caelestia: utilities " .. name .. " (dry run): " .. table.concat(argv, " "))
    return true
  end
  morf.run(argv, {}, function(result)
    if result and not result.ok then
      morf.log("warn", "caelestia: utilities " .. name .. " failed: " .. tostring(result.stderr or result.code))
    end
  end)
  return true
end

-- ------------------------------------------------------------ keep awake --

M.awake = morf.signal("caelestia.utilities.awake", false)
local awake_since = morf.signal("caelestia.utilities.awake_since", "")

morf.effect("caelestia.utilities.inhibit", function()
  local on = M.awake:get()
  if dry_run() then
    morf.log("info", "caelestia: keep awake " .. (on and "on" or "off") .. " (dry run)")
  elseif morf.idle and morf.idle.inhibit then
    morf.idle.inhibit(on)
  end
end)

local function clock(at)
  if config.get("bar.clock.twelve_hour") then
    return (morf.time.format("%-I:%M %p", at):lower())
  end
  return (morf.time.format("%H:%M", at))
end

function M.set_awake(on)
  if on and not M.awake:get() then awake_since:set(clock(morf.time.now())) end
  M.awake:set(on and true or false)
end

--- A round badge for a card's icon: secondaryContainer, or the secondary
--- colour when `on()`.
local function badge(icon, on)
  local function lit() return on and on() end
  return ui.Rect {
    x = 16, y = 16, width = 54, height = 54, radius = 27,
    color = function() return lit() and C.secondary or C.secondaryContainer end,
    behavior = { color = { duration = theme.duration.small } },
    kit.icon(icon, 28, function() return lit() and C.onSecondary or C.onSecondaryContainer end, {
      anchors = { center_in = true }, fill = lit,
    }),
  }
end

local function heading(title, subtitle, width)
  return ui.Column {
    x = 82, gap = 0, anchors = { vertical_center = true },
    kit.text { text = title, font_size = theme.size.large, width = width, elide = "right" },
    kit.text {
      text = subtitle, font_size = theme.size.larger, width = width, elide = "right",
      color = function() return C.onSurfaceVariant end,
    },
  }
end

--- The reference's switch, 52 x 30: off, a dark track and a grey thumb
--- nearly its height with a cross; on, a primary track and a dark thumb
--- with a tick. Travelling, the thumb turns into a nine-lobed cookie and
--- rolls across, then settles back into a circle.
local function switch(spec)
  local function on() return spec.on() == true end
  local rolling = morf.signal("caelestia.utilities.switch." .. spec.id, false)
  local settle
  local area
  area = ui.MouseArea {
    id = spec.id, width = 52, height = 30, cursor = "pointer",
    anchors = spec.anchors,
    on_clicked = function()
      rolling:set(true)
      if settle then settle:cancel() end
      settle = morf.timer(260, function() settle = nil rolling:set(false) end, false)
      spec.on_toggled(not on())
    end,
    ui.Rect {
      anchors = { fill = true }, radius = 15,
      color = function() return on() and C.primary or C.surfaceContainerHighest end,
      behavior = { color = { duration = theme.duration.small } },
    },
    ui.Item {
      y = 1, width = 28, height = 28,
      x = function() return on() and 23 or 1 end,
      behavior = { x = { duration = 300, easing = theme.ease.standard } },
      ui.Item {
        anchors = { fill = true },
        -- A whole turn across, so at rest it is as it was.
        rotation = function() return on() and 360 or 0 end,
        behavior = { rotation = { duration = 300, easing = theme.ease.standard } },
        kit.shape {
          id = spec.id .. "-thumb",
          anchors = { fill = true },
          shape = function() return rolling:get() and "cookie9" or "circle" end,
          duration = 200,
          color = function() return on() and C.onPrimary or C.outline end,
        },
      },
      kit.icon(function() return on() and "check" or "close" end, 20, function()
        return on() and C.primary or C.surfaceContainerHighest
      end, { anchors = { center_in = true } }),
    },
  }
  return area
end

-- ----------------------------------------------------------------- toggles --

-- Off the machine's services (none in a sandbox) a toggle keeps its own
-- state, so it still answers.
local local_state = {
  wifi = morf.signal("caelestia.utilities.wifi", false),
  bluetooth = morf.signal("caelestia.utilities.bluetooth", false),
  mic = morf.signal("caelestia.utilities.mic", true),
  gamemode = morf.signal("caelestia.utilities.gamemode", false),
}

--- The VPNs of `kind` that are up, by name: the mesh ones and the tunnel
--- apps by their links (no command run), and for tunnels NetworkManager's
--- own VPN and WireGuard profiles.
function M.vpn_names(kind)
  local names = {}
  local n = services.net
  local ok, vpns = pcall(require, "lib.vpns")
  if kind == "tunnel" and n and n.state.available then
    local model = n.state.vpn_connections
    for i = 1, model:len() do
      local v = model:get(i)
      if v.active and not (ok and vpns.is_mesh_link(v.id)) then names[#names + 1] = v.id end
    end
  end
  if ok then
    -- A link is read, not watched: re-read as the network changes.
    if n and n.state.available then n.state.devices:len() end
    for _, name in ipairs(vpns.links(kind)) do names[#names + 1] = name end
  end
  return names
end

--- Airplane mode on: what was on is remembered, then every radio is shut;
--- off, what was on comes back.
function M.set_airplane(on)
  local net, bt, modem = services.net, services.bt, services.modem
  if dry_run() then
    morf.log("info", "caelestia: airplane mode " .. (on and "on" or "off") .. " (dry run)")
    config.set("airplane.on", on == true)
    return
  end
  if on then
    config.set("airplane.was", {
      wifi = net ~= nil and net.state.available and net.state.wifi_enabled == true,
      bluetooth = bt ~= nil and bt.state.available and bt.state.powered == true,
      mobile = modem ~= nil and modem.state.data == true,
    })
    if net and net.state.available then pcall(net.set_wifi, false) pcall(net.set_wwan, false) end
    if bt and bt.state.available then pcall(bt.set_powered, false) end
  else
    local was = config.get("airplane.was") or {}
    if net and net.state.available then
      if was.wifi then pcall(net.set_wifi, true) end
      if was.mobile then pcall(net.set_wwan, true) end
    end
    if bt and bt.state.available and was.bluetooth then pcall(bt.set_powered, true) end
  end
  config.set("airplane.on", on == true)
end

M.TOGGLES = {
  {
    id = "wifi", icon = "wifi", name = "Wi-Fi", detail = "network",
    status = function()
      local n = services.net
      if n and n.state.available then
        local model = n.state.access_points
        for i = 1, model:len() do
          local ap = model:get(i)
          if ap.in_use then return ap.ssid or "Connected" end
        end
      end
      return nil
    end,
    on = function()
      local n = services.net
      if n and n.state.available then return n.state.wifi_enabled == true end
      return local_state.wifi:get()
    end,
    set = function(now)
      local n = services.net
      if n and n.state.available and not dry_run() then pcall(n.set_wifi, now) return end
      if dry_run() then morf.log("info", "caelestia: wifi " .. (now and "on" or "off") .. " (dry run)") end
      local_state.wifi:set(now)
    end,
  },
  {
    id = "bluetooth", icon = "bluetooth", name = "Bluetooth", detail = "bluetooth",
    status = function()
      local b = services.bt
      if b and b.state.available then
        local model = b.state.devices
        for i = 1, model:len() do
          local d = model:get(i)
          if d.connected then return d.alias or d.name or "Connected" end
        end
      end
      return nil
    end,
    on = function()
      local b = services.bt
      if b and b.state.available then return b.state.powered == true end
      return local_state.bluetooth:get()
    end,
    set = function(now)
      local b = services.bt
      if b and b.state.available and not dry_run() then pcall(b.set_powered, now) return end
      if dry_run() then morf.log("info", "caelestia: bluetooth " .. (now and "on" or "off") .. " (dry run)") end
      local_state.bluetooth:set(now)
    end,
  },
  {
    -- Wired: the first wired port's state, a click connects or disconnects
    -- it; ">" lists every port.
    id = "wired", name = "Wired", detail = "wired",
    icon = function()
      local n = services.net
      return (n and n.state.available and n.state.wired.carrier == false) and "settings_ethernet" or "lan"
    end,
    on = function()
      local n = services.net
      return n ~= nil and n.state.available and n.state.wired.connected == true
    end,
    set = function(now)
      local n = services.net
      if not (n and n.state.available) then return end
      local port = n.state.wired.device
      if port == "" then return end
      if dry_run() then morf.log("info", "caelestia: wired " .. tostring(now) .. " (dry run)") return end
      if now then pcall(n.connect_device, port) else pcall(n.disconnect, port) end
    end,
    status = function()
      local n = services.net
      if not (n and n.state.available) then return nil end
      local w = n.state.wired
      if w.device == "" then return "No port" end
      if w.connected then return w.ip4 ~= "" and w.ip4 or "Connected" end
      return w.carrier and "Disconnected" or "No cable"
    end,
  },
  {
    -- A phone's mobile data. Without a modem it says so, crossed out.
    id = "mobile", name = "Mobile data",
    icon = function()
      local m = services.modem
      if not m then return "signal_cellular_nodata" end
      return m.state.data and "signal_cellular_alt" or "signal_cellular_off"
    end,
    on = function() return services.modem ~= nil and services.modem.state.data end,
    set = function(now)
      local m = services.modem
      if not m then return end
      if dry_run() then morf.log("info", "caelestia: mobile data " .. tostring(now) .. " (dry run)") return end
      pcall(m.set_data, now)
    end,
    status = function()
      local m = services.modem
      if not m then return "No modem" end
      local s = m.state
      if s.locked then return "SIM locked" end
      if not s.data then return "Off" end
      return (s.technology ~= "" and (s.technology .. " · ") or "") .. (s.operator ~= "" and s.operator or "On")
    end,
  },
  {
    -- Mesh VPNs: one's own machines (NetBird, Tailscale, ZeroTier).
    id = "mesh", icon = "hub", name = "Mesh", detail = "mesh",
    on = function() return #M.vpn_names("mesh") > 0 end,
    set = function() M.detail:set("mesh") end,
    status = function()
      local names = M.vpn_names("mesh")
      return #names == 0 and "Off" or table.concat(names, ", ")
    end,
  },
  {
    -- Tunnel VPNs: out to the internet through elsewhere (Mullvad, Proton).
    id = "tunnel", icon = "vpn_lock", name = "Tunnel", detail = "tunnel",
    on = function() return #M.vpn_names("tunnel") > 0 end,
    set = function() M.detail:set("tunnel") end,
    status = function()
      local names = M.vpn_names("tunnel")
      return #names == 0 and "Off" or table.concat(names, ", ")
    end,
  },
  {
    -- Airplane mode: every radio off at once -- Wi-Fi, Bluetooth, mobile
    -- data -- and back as they were.
    id = "airplane", icon = "flight", name = "Airplane mode",
    on = function() return config.get("airplane.on") == true end,
    set = function(now) M.set_airplane(now) end,
    status = function() return config.get("airplane.on") == true and "On" or "Off" end,
  },
  {
    id = "sound", name = "Sound", detail = "sound",
    icon = function()
      local ok, sink = pcall(function() return morf.audio.available() and morf.audio.default_sink() end)
      return (ok and sink and sink.muted) and "volume_off" or "volume_up"
    end,
    on = function()
      local ok, sink = pcall(function() return morf.audio.available() and morf.audio.default_sink() end)
      if ok and sink then return not sink.muted end
      return true
    end,
    set = function(now)
      local ok, sink = pcall(function() return morf.audio.available() and morf.audio.default_sink() end)
      if not (ok and sink) then return end
      if dry_run() then morf.log("info", "caelestia: sound " .. (now and "on" or "off") .. " (dry run)") return end
      morf.audio.set_mute(sink.id, not now)
    end,
    status = function()
      local ok, sink = pcall(function() return morf.audio.available() and morf.audio.default_sink() end)
      if ok and sink then return ("%d%%"):format(math.floor(sink.volume * 100 + 0.5)) end
      return nil
    end,
  },
  {
    id = "mic", icon = function() return "mic" end, name = "Microphone", detail = "microphone",
    on = function() return local_state.mic:get() end,
    set = function(now)
      M.run(now and "mic_on" or "mic_off")
      local_state.mic:set(now)
    end,
  },
  {
    -- The battery: its charge, and a ">" to the Power page (profiles,
    -- health, how it charges). A tap opens the page too.
    id = "battery", name = "Battery", detail = "power",
    icon = function()
      local u = services.upower
      local d = u and u.state.available and u.state.display or {}
      if not d.present then return "power" end
      if d.charging then return "battery_charging_full" end
      local pct = d.percentage or 0
      if pct > 90 then return "battery_full" end
      if pct > 50 then return "battery_5_bar" end
      if pct > 20 then return "battery_3_bar" end
      return "battery_alert"
    end,
    on = function()
      local u = services.upower
      local d = u and u.state.available and u.state.display or {}
      return d.charging == true
    end,
    set = function() M.detail:set("power") end,
    status = function()
      local u = services.upower
      local d = u and u.state.available and u.state.display or {}
      if not d.present then return "On mains power" end
      return ("%d%%%s"):format(math.floor((d.percentage or 0) + 0.5), d.charging and ", charging" or "")
    end,
  },
  {
    id = "awake", icon = "coffee", name = "Keep awake",
    on = function() return M.awake:get() end,
    set = function(now) M.set_awake(now) end,
    status = function()
      if not M.awake:get() then return "Off" end
      return "Since " .. awake_since:get()
    end,
  },
  {
    -- How it calls for attention: sound, vibrate (on a phone) or silent.
    id = "ringer", name = "Ring mode",
    icon = function()
      local r = services.ringer
      return require("lib.ringer").icon(r and r.state.mode or "sound")
    end,
    on = function() local r = services.ringer return r ~= nil and r.state.mode ~= "sound" end,
    set = function() local r = services.ringer if r then r.next() end end,
    status = function()
      local r = services.ringer
      local mode = r and r.state.mode or "sound"
      return mode:sub(1, 1):upper() .. mode:sub(2)
    end,
  },
  {
    -- The bar: up or down, and a ">" to where it goes.
    id = "bar", icon = "toolbar", name = "Bar", detail = "bar",
    on = function() return require("bar").on() end,
    set = function(now) require("bar").set_on(now) end,
    status = function()
      local b = require("bar")
      if not b.on() then return "Off" end
      local side = b.side()
      return side:sub(1, 1):upper() .. side:sub(2)
    end,
  },
  {
    id = "dnd", icon = "notifications_off", name = "Do not disturb",
    on = function() return notifs.dnd:get() end,
    set = function(now) notifs.dnd:set(now) end,
  },
}

--- The detail page on show over the settings ("" for none): a tile's
--- ">" opens one, the back arrow shuts it.
M.detail = morf.signal("caelestia.settings.detail", "")

local TILE_H, TILE_GAP = 60, 8
local TILE_W = (CARD_W - 24 - TILE_GAP) / 2
-- Tiles for what this machine has: mobile data only with a modem.
do
  local kept = {}
  for _, t in ipairs(M.TOGGLES) do
    if not t.present or t.present() then kept[#kept + 1] = t end
  end
  M.TOGGLES = kept
end
local TILE_ROWS = math.ceil(#M.TOGGLES / 2)
local TILES_H = 24 + TILE_ROWS * TILE_H + (TILE_ROWS - 1) * TILE_GAP

--- A quick setting as Android draws one: a tile with its icon, its name
--- and how it is, filled when on. A click on it toggles it; the ">" at its
--- end, when it has more, opens its page. On, it rounds a little less, and
--- squares up a touch under the finger (M3 expressive's shape morph).
local function tile(t)
  local area, more
  local function on() return t.on() == true end
  local fg = function() return on() and C.onPrimary or C.onSurface end
  local sub = function() return on() and C.onPrimary:alpha(0.8) or C.onSurfaceVariant end
  area = ui.MouseArea {
    id = "utilities-toggle-" .. t.id,
    width = TILE_W, height = TILE_H, cursor = "pointer",
    on_clicked = function() t.set(not on()) end,
    ui.Rect {
      id = "utilities-toggle-" .. t.id .. "-shape",
      anchors = { fill = true },
      radius = function()
        if (area and area.pressed) or (more and more.pressed) then return 10 end
        return on() and 18 or TILE_H / 2
      end,
      color = function()
        local base = on() and C.primary or C.surfaceContainerHighest
        if area and area.hovered then return base:mix(on() and C.onPrimary or C.onSurface, 0.08) end
        return base
      end,
      behavior = {
        radius = ui.spring { stiffness = 420, damping = 26 },
        color = { duration = theme.duration.small },
      },
    },
    kit.icon(t.icon, 22, fg, { x = 16, anchors = { vertical_center = true }, fill = t.fill or on }),
    ui.Column {
      x = 48, anchors = { vertical_center = true }, gap = 0,
      kit.text {
        width = TILE_W - 48 - (t.detail and 36 or 12), elide = "right",
        text = t.name or t.id, font_size = theme.size.normal, font_weight = 500, color = fg,
      },
      kit.text {
        width = TILE_W - 48 - (t.detail and 36 or 12), elide = "right",
        text = function()
          local s = t.status and t.status()
          if s and s ~= "" then return s end
          return on() and "On" or "Off"
        end,
        font_size = theme.size.small, color = sub,
      },
    },
  }
  if t.detail then
    local wash = ui.Rect {
      anchors = { fill = true, top_margin = 10, bottom_margin = 10, right_margin = 6 }, radius = 10,
      behavior = { color = { duration = theme.duration.small } },
    }
    more = ui.MouseArea {
      id = "utilities-more-" .. t.id,
      anchors = { right = true, top = true, bottom = true }, width = 36, cursor = "pointer",
      on_clicked = function() M.detail:set(t.detail) end,
      wash,
      kit.icon("chevron_right", 22, fg, { anchors = { center_in = true, horizontal_center_offset = -3 } }),
    }
    -- Bound once `more` exists: it reads its hover.
    wash.color = function() return more.hovered and fg():alpha(0.12) or fg():alpha(0) end
    ui.reparent(more, area)
  end
  return area
end

local function toggles()
  local rows = {}
  for i = 1, #M.TOGGLES, 2 do
    local row = { gap = TILE_GAP }
    row[#row + 1] = tile(M.TOGGLES[i])
    if M.TOGGLES[i + 1] then row[#row + 1] = tile(M.TOGGLES[i + 1]) end
    rows[#rows + 1] = ui.Row(row)
  end
  return kit.card {
    id = "utilities-toggles",
    width = CARD_W, height = TILES_H, radius = M.RADIUS,
    ui.Column { x = 12, y = 12, gap = TILE_GAP, table.unpack(rows) },
  }
end

-- ----------------------------------------------------------------- sliders --

-- The output's volume and the screen's brightness, as Material 3
-- expressive sliders: a tall rounded track, the active part in the primary
-- colour up to a slim handle with a gap either side, the icon inside the
-- track's start and the value at its end. The level rides a spring; the
-- handle narrows while held. They read and set what the OSD does.
local SLIDER_H = 44
local SLIDERS_H = 16 + SLIDER_H + 12 + SLIDER_H + 16

local function slider(id, value, set, icon)
  return kit.slider { id = id, width = CARD_W - 32, value = value, set = set, icon = icon }
end

local function sliders()
  local osd = require("osd")
  return kit.card {
    id = "utilities-sliders",
    width = CARD_W, height = SLIDERS_H, radius = M.RADIUS,
    ui.Column {
      x = 16, y = 12, gap = 4,
      slider("utilities-volume", function() return (osd.volume()) end, osd.set_volume, osd.volume_icon),
      slider("utilities-brightness", function() return (osd.brightness()) end, osd.set_brightness, osd.brightness_icon),
    },
  }
end

-- ------------------------------------------------------------------- power --

-- -------------------------------------------------------------------- page --

local cards = { sliders(), toggles() }

--- The page's height: the cards and their gaps.
function M.height()
  return GAP + SLIDERS_H + TILES_H
end


--- The Settings page of the right panel: the cards, top down, and the
--- detail pages (Network, Bluetooth, Sound) beside them, which slide in
--- over them as a tile's ">" opens one.
local DETAIL_HEAD = 52
local SWITCH = { duration = theme.duration.normal, easing = theme.ease.emphasized_decel }
M.DETAILS = {
  { key = "network", name = "Network", build = function(w, h) return require("connectivity").network_page(w, h) end },
  { key = "bluetooth", name = "Bluetooth", build = function(w, h) return require("connectivity").bluetooth_page(w, h) end },
  { key = "sound", name = "Sound", build = function(w, h) return require("sound_page").output_page(w, h) end },
  { key = "microphone", name = "Microphone", build = function(w, h) return require("sound_page").input_page(w, h) end },
  { key = "power", name = "Power", build = function(w, h) return require("power_page").page(w, h) end },
  { key = "bar", name = "Bar", build = function(w, h) return require("bar_page").page(w, h) end },
  { key = "wired", name = "Wired", build = function(w, h) return require("net_pages").wired_page(w, h) end },
  { key = "mesh", name = "Mesh", build = function(w, h) return require("net_pages").vpn_page("mesh", w, h, M.detail) end },
  { key = "tunnel", name = "Tunnel", build = function(w, h) return require("net_pages").vpn_page("tunnel", w, h, M.detail) end },
}
function M.page(w, h)
  local main = ui.Item {
    id = "utilities", width = w, height = h, clip = true,
    ui.Column { gap = GAP, table.unpack(cards) },
  }
  local dh = function() return h() - DETAIL_HEAD end
  local stack = {}
  local names = {}
  for _, d in ipairs(M.DETAILS) do
    names[d.key] = d.name
    stack[#stack + 1] = ui.Item {
      id = "settings-detail-" .. d.key,
      y = DETAIL_HEAD, width = w, height = dh,
      visible = function() return M.detail:get() == d.key end,
      d.build(w, dh),
    }
  end
  local back_wash = ui.Rect {
    anchors = { fill = true }, radius = 20,
    behavior = { color = { duration = theme.duration.small } },
  }
  local back = ui.MouseArea {
    id = "settings-back",
    width = 40, height = 40, y = 2, cursor = "pointer",
    on_clicked = function() M.detail:set("") end,
    back_wash,
    kit.icon("arrow_back", 22, function() return C.onSurface end, { anchors = { center_in = true } }),
  }
  back_wash.color = function() return back.hovered and C.onSurface:alpha(0.08) or C.onSurface:alpha(0) end
  local detail = ui.Item {
    id = "settings-detail", width = w, height = h,
    back,
    kit.text {
      x = 50, y = 10,
      text = function() return names[M.detail:get()] or "" end,
      font_size = theme.size.large, font_weight = 500,
    },
    table.unpack(stack),
  }
  -- The last detail shown stays drawn while it slides away.
  return ui.Item {
    width = w, height = h, clip = true,
    ui.Row {
      gap = 22,
      translate_x = function() return M.detail:get() ~= "" and -(w + 22) or 0 end,
      behavior = { translate_x = SWITCH },
      main, detail,
    },
  }
end

--- The page coming into view (`true`) or leaving: the cards come in one
--- after the other, each growing evenly about its centre as it fades in.
local running = {}
function M.shown(open)
  for _, h in ipairs(running) do h:stop() end
  if not open then M.detail:set("") end
  running = kit.bud(cards, open)
end

return M
