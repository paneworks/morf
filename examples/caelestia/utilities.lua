-- The utilities: the right panel's Settings page -- volume and brightness
-- sliders, a row of quick toggles (Wi-Fi, Bluetooth, the microphone,
-- settings, game mode, do not disturb), the battery and the power profile,
-- keep awake (an idle inhibitor), and the screen recorder with its
-- recordings.
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

-- ---------------------------------------------------------------- recorder --

M.recording = morf.signal("caelestia.utilities.recording", false)
M.mode = morf.signal("caelestia.utilities.mode", "fullscreen")
local list_open = morf.signal("caelestia.utilities.recordings.open", false)
local menu_open = morf.signal("caelestia.utilities.recorder.menu", false)
local MODES = {
  { id = "fullscreen", label = "Fullscreen", icon = "fullscreen" },
  { id = "region", label = "Region", icon = "screenshot_region" },
}
local function mode_of(id)
  for _, m in ipairs(MODES) do if m.id == id then return m end end
  return MODES[1]
end

--- Starts recording in the chosen mode, or stops.
function M.record()
  menu_open:set(false)
  if M.recording:get() then
    M.run("record_stop")
    M.recording:set(false)
  else
    M.run("record_" .. M.mode:get())
    M.recording:set(true)
  end
end

local recordings = morf.signal("caelestia.utilities.recordings", {})
local function scan()
  local dir = expand({ config.get("utilities.recordings") })[1]
  local ok, entries = pcall(morf.fs.list, dir)
  local out = {}
  if ok and type(entries) == "table" then
    for _, e in ipairs(entries) do
      local ext = (e.extension or ""):lower()
      if e.is_file and (ext == "mp4" or ext == "mkv" or ext == "webm") then
        out[#out + 1] = { path = e.path, name = e.name or e.path:match("([^/]+)$") }
      end
    end
  end
  table.sort(out, function(a, b) return a.name > b.name end)
  recordings:set(out)
end

local function recorder_height()
  return list_open:get() and 376 or 208
end

local function split_button()
  -- M3's split button: round outside, a small radius where the halves meet.
  local function half(props, radii)
    local area
    area = ui.MouseArea(props)
    local bg = ui.Rect {
      anchors = { fill = true }, z = -1,
      top_left_radius = radii[1], top_right_radius = radii[2],
      bottom_right_radius = radii[3], bottom_left_radius = radii[4],
      color = function()
        return area.hovered and C.primary:mix(C.onPrimary, 0.08) or C.primary
      end,
      behavior = { color = { duration = theme.duration.small } },
    }
    ui.reparent(bg, area)
    return area
  end
  local main = half({
    id = "utilities-record",
    x = 0, width = 128, height = 40, cursor = "pointer",
    on_clicked = M.record,
    ui.Row {
      anchors = { center_in = true }, gap = 8, align = "center",
      kit.icon(function() return M.recording:get() and "stop" or mode_of(M.mode:get()).icon end, 20,
        function() return C.onPrimary end),
      kit.text {
        text = function() return M.recording:get() and "Stop" or mode_of(M.mode:get()).label end,
        font_size = theme.size.normal + 1, color = function() return C.onPrimary end,
      },
    },
  }, { 20, 5, 5, 20 })
  local more = half({
    id = "utilities-record-mode",
    x = 130, width = 40, height = 40, cursor = "pointer",
    on_clicked = function() menu_open:set(not menu_open:get()) end,
    kit.icon(function() return menu_open:get() and "expand_less" or "expand_more" end, 20,
      function() return C.onPrimary end, { anchors = { center_in = true } }),
  }, { 5, 20, 20, 5 })
  return ui.Item {
    width = 170, height = 40, anchors = { right = true, right_margin = 16 }, y = 23,
    main, more,
  }
end

--- The menu of the recorder's modes, under the split button.
local function mode_menu()
  local rows = {}
  for _, m in ipairs(MODES) do
    rows[#rows + 1] = ui.MouseArea {
      id = "utilities-mode-" .. m.id,
      width = 160, height = 40, cursor = "pointer",
      on_clicked = function() M.mode:set(m.id) menu_open:set(false) end,
      ui.Row {
        x = 14, anchors = { vertical_center = true }, gap = 10, align = "center",
        kit.icon(m.icon, 20, function() return C.onSurface end),
        kit.text { text = m.label, font_size = theme.size.normal },
      },
    }
  end
  -- The selection is one blob under the rows: it slides to the row under
  -- the pointer (else the chosen mode's) and settles there, square to it.
  local function at()
    for i, r in ipairs(rows) do if r.hovered then return i end end
    for i, m in ipairs(MODES) do if m.id == M.mode:get() then return i end end
    return 1
  end
  local blob = ui.Rect {
    id = "utilities-mode-blob",
    x = 6, width = 160, height = 40, radius = 12,
    y = function() return 6 + (at() - 1) * 40 end,
    color = function() return C.secondaryContainer end,
    behavior = { y = ui.spring { stiffness = 420, damping = 41 } },
  }
  local menu = ui.Rect {
    id = "utilities-mode-menu",
    anchors = { right = true, right_margin = 16 }, y = 68, z = 10,
    width = 172, height = #MODES * 40 + 12, radius = 16,
    color = function() return C.surfaceContainerHigh end,
    visible = function() return menu_open:get() end,
    blob,
    ui.Column { x = 6, y = 6, gap = 0, table.unpack(rows) },
  }
  morf.effect("caelestia.utilities.menu.bud", function()
    if menu_open:get() then kit.bud({ menu }, true, { delay = 0 }) end
  end)
  return menu
end

local function recorder()
  -- Nothing recorded: a line under the list's heading, shut; opened, a
  -- large mark over it.
  local function none() return #recordings:get() == 0 end
  local empty = ui.Item {
    id = "utilities-recordings-empty",
    width = CARD_W, height = 1,
    ui.Row {
      id = "utilities-recordings-none",
      anchors = { horizontal_center = true }, y = 147, gap = 8, align = "center",
      visible = function() return none() and not list_open:get() end,
      kit.icon("scan_delete", 18, function() return C.outline end),
      kit.text { text = "No recordings found", font_size = theme.size.normal + 1, color = function() return C.outline end },
    },
    kit.icon("scan_delete", 48, function() return C.outline end, {
      anchors = { horizontal_center = true }, y = 199,
      visible = function() return none() and list_open:get() end,
    }),
    kit.text {
      anchors = { horizontal_center = true }, y = 260,
      text = "No recordings found", font_size = theme.size.normal + 1, color = function() return C.outline end,
      visible = function() return none() and list_open:get() end,
    },
  }
  local rows = {}
  for i = 1, 6 do
    local function r() return recordings:get()[i] end
    rows[i] = ui.Row {
      height = 32, gap = 10, align = "center",
      visible = function() return r() ~= nil and (i <= 2 or list_open:get()) end,
      kit.icon("movie", 18, function() return C.onSurfaceVariant end),
      kit.text { text = function() local x = r() return x and x.name or "" end, width = 330, elide = "middle" },
    }
  end
  local listed = ui.Column { x = 16, y = 128, gap = 4, visible = function() return #recordings:get() > 0 end, table.unpack(rows) }
  return kit.card {
    id = "utilities-recorder",
    width = CARD_W, radius = M.RADIUS, clip = true,
    height = recorder_height,
    behavior = { height = motion },
    ui.Item {
      width = CARD_W, height = 86,
      badge("screen_record", function() return M.recording:get() end),
      heading("Screen recorder", function() return M.recording:get() and "Recording" or "Ready" end, 112),
    },
    split_button(),
    kit.hover(ui.MouseArea {
      id = "utilities-recordings",
      x = 8, y = 81, width = CARD_W - 16, height = 40, cursor = "pointer",
      on_clicked = function()
        scan()
        list_open:set(not list_open:get())
      end,
      kit.icon("list", 24, function() return C.onSurface end, { x = 8, anchors = { vertical_center = true } }),
      kit.text { x = 49, anchors = { vertical_center = true }, text = "Recordings", font_size = theme.size.large },
      kit.icon(function() return list_open:get() and "unfold_less" or "unfold_more" end, 22,
        function() return C.onSurface end, { anchors = { right = true, right_margin = 8, vertical_center = true } }),
    }, function(hovered) return hovered and C.onSurface:alpha(0.06) or C.onSurface:alpha(0) end, 20),
    empty,
    listed,
    mode_menu(),
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
    id = "mic", icon = function() return "mic" end, name = "Microphone", detail = "sound",
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

-- The battery and the power profile, as the bar's power popout had them.
local PROFILES = {
  { id = "power-saver", icon = "energy_savings_leaf", name = "Power saver" },
  { id = "balanced", icon = "balance", name = "Balanced" },
  { id = "performance", icon = "rocket_launch", name = "Performance" },
}
local POWER_H = 124

local function power()
  local function up() return services.upower end
  local function active()
    local u = up()
    local a = u and u.state.available and u.state.profiles.active or ""
    -- Without power-profiles-daemon the machine runs balanced.
    return a == "" and "balanced" or a
  end
  local function battery()
    local u = up()
    local d = u and u.state.available and u.state.display or {}
    return d.present and d or nil
  end
  local buttons = {}
  for _, p in ipairs(PROFILES) do
    local function on() return active() == p.id end
    local area
    area = ui.MouseArea {
      id = "utilities-profile-" .. p.id,
      width = (CARD_W - 32 - 16) / 3, height = 44, cursor = "pointer",
      on_clicked = function()
        local u = up()
        if dry_run() then morf.log("info", "caelestia: power profile " .. p.id .. " (dry run)") return end
        if u then pcall(u.set_profile, p.id) end
      end,
      ui.Rect {
        anchors = { fill = true },
        radius = function() return on() and 12 or 22 end,
        color = function()
          local base = on() and C.primary or C.surfaceContainerHighest
          if area and area.hovered then return base:mix(on() and C.onPrimary or C.onSurface, 0.08) end
          return base
        end,
        behavior = { color = { duration = theme.duration.small }, radius = kit.spring(260, 16) },
      },
      ui.Row {
        anchors = { center_in = true }, gap = 6, align = "center",
        kit.icon(p.icon, 20, function() return on() and C.onPrimary or C.onSurfaceVariant end),
        kit.text {
          text = p.name, font_size = theme.size.small,
          color = function() return on() and C.onPrimary or C.onSurface end,
        },
      },
    }
    buttons[#buttons + 1] = area
  end
  return kit.card {
    id = "utilities-power",
    width = CARD_W, height = POWER_H, radius = M.RADIUS,
    kit.icon(function()
      local b = battery()
      if not b then return "power" end
      if b.charging then return "battery_charging_full" end
      local pct = b.percentage or 0
      if pct > 90 then return "battery_full" end
      if pct > 50 then return "battery_5_bar" end
      if pct > 20 then return "battery_3_bar" end
      return "battery_alert"
    end, 22, function() return C.onSurfaceVariant end, { x = 16, y = 17 }),
    kit.text {
      id = "utilities-battery",
      x = 46, y = 16,
      text = function()
        local b = battery()
        if not b then return "On mains power" end
        local pct = math.floor((b.percentage or 0) + 0.5)
        return ("Battery %d%%%s"):format(pct, b.charging and ", charging" or "")
      end,
      font_size = theme.size.large,
    },
    ui.Row { x = 16, y = 62, gap = 8, table.unpack(buttons) },
  }
end

-- -------------------------------------------------------------------- page --

local cards = { sliders(), toggles(), power(), keep_awake(), recorder() }

--- The page's height: the cards and their gaps.
function M.height()
  local h = 4 * GAP + SLIDERS_H + TILES_H + POWER_H
  h = h + (M.awake:get() and 128 or 86)
  h = h + recorder_height()
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
  { key = "sound", name = "Sound", build = function(w, h) return require("sound_page").build(w, h) end },
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
  if open then scan() else menu_open:set(false) M.detail:set("") end
  running = kit.bud(cards, open)
end

return M
