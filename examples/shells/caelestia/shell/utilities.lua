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

local function keep_awake()
  local chip_text = kit.text {
    text = function() return "Active since " .. awake_since:get() end,
    font_size = theme.size.small, color = function() return C.onPrimary end,
    anchors = { center_in = true },
  }
  return kit.card {
    id = "utilities-awake",
    width = CARD_W, radius = M.RADIUS, clip = true,
    height = function() return M.awake:get() and 128 or 86 end,
    behavior = { height = motion },
    ui.Item {
      width = CARD_W, height = 86,
      badge("coffee", function() return M.awake:get() end),
      heading("Keep awake", function()
        return M.awake:get() and "Preventing sleep mode" or "Normal power management"
      end, 250),
      switch {
        id = "utilities-awake-switch",
        anchors = { right = true, right_margin = 16, vertical_center = true },
        on = function() return M.awake:get() end,
        on_toggled = M.set_awake,
      },
    },
    ui.Rect {
      id = "utilities-awake-chip",
      x = 16, y = 86, height = 26, radius = 13,
      width = function() return (chip_text.layout_width or 150) + 24 end,
      color = function() return C.primary end,
      opacity = function() return M.awake:get() and 1 or 0 end,
      behavior = { opacity = { duration = theme.duration.small } },
      chip_text,
    },
  }
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
    id = "settings", icon = "settings", fill = true, name = "Settings",
    status = function() return "Open" end,
    on = function() return false end,
    set = function() M.run("settings") end,
  },
  {
    id = "gamemode", icon = "gamepad", name = "Game mode",
    on = function() return local_state.gamemode:get() end,
    set = function(now)
      M.run(now and "gamemode_on" or "gamemode_off")
      local_state.gamemode:set(now)
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
local TILES_H = 24 + 4 * TILE_H + 3 * TILE_GAP

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

-- The battery, as one row: its charge and state, and a ">" to the Power
-- page (utilities' detail "power"), where the profile, the battery's health
-- and the rest of power live.
local POWER_H = 64

local function power()
  local function battery()
    local u = services.upower
    local d = u and u.state.available and u.state.display or {}
    return d.present and d or nil
  end
  local area
  area = ui.MouseArea {
    id = "utilities-power",
    width = CARD_W, height = POWER_H, cursor = "pointer",
    on_clicked = function() M.detail:set("power") end,
    kit.card {
      anchors = { fill = true }, radius = M.RADIUS,
      color = function()
        local base = C.surfaceContainer
        return (area and area.hovered) and base:mix(C.onSurface, 0.04) or base
      end,
    },
    kit.icon(function()
      local b = battery()
      if not b then return "power" end
      if b.charging then return "battery_charging_full" end
      local pct = b.percentage or 0
      if pct > 90 then return "battery_full" end
      if pct > 50 then return "battery_5_bar" end
      if pct > 20 then return "battery_3_bar" end
      return "battery_alert"
    end, 24, function() return C.primary end, { x = 18, anchors = { vertical_center = true }, fill = true }),
    kit.text {
      id = "utilities-battery",
      x = 54, anchors = { vertical_center = true },
      text = function()
        local b = battery()
        if not b then return "On mains power" end
        local pct = math.floor((b.percentage or 0) + 0.5)
        return ("Battery %d%%%s"):format(pct, b.charging and ", charging" or "")
      end,
      font_size = theme.size.large,
    },
    kit.icon("chevron_right", 24, function() return C.onSurfaceVariant end,
      { anchors = { right = true, right_margin = 16, vertical_center = true } }),
  }
  return area
end

-- -------------------------------------------------------------------- page --

local cards = { sliders(), toggles(), power(), keep_awake() }

--- The page's height: the cards and their gaps.
function M.height()
  local h = 3 * GAP + SLIDERS_H + TILES_H + POWER_H
  h = h + (M.awake:get() and 128 or 86)
  return h
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
