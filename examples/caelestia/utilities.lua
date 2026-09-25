-- The utilities: a drawer out of the frame's bottom edge at the right --
-- keep awake (an idle inhibitor), the screen recorder with its recordings,
-- and a row of quick toggles (Wi-Fi, Bluetooth, the microphone, settings,
-- game mode, do not disturb). The sidebar opens it too, under the
-- notifications.
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
local drawer = require("drawer")
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
    id = "wifi", icon = "wifi",
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
    id = "bluetooth", icon = "bluetooth",
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
    id = "mic", icon = function() return "mic" end,
    on = function() return local_state.mic:get() end,
    set = function(now)
      M.run(now and "mic_on" or "mic_off")
      local_state.mic:set(now)
    end,
  },
  {
    id = "settings", icon = "settings", fill = true,
    on = function() return false end,
    set = function() M.run("settings") end,
  },
  {
    id = "gamemode", icon = "gamepad",
    on = function() return local_state.gamemode:get() end,
    set = function(now)
      M.run(now and "gamemode_on" or "gamemode_off")
      local_state.gamemode:set(now)
    end,
  },
  {
    id = "dnd", icon = "notifications_off",
    on = function() return notifs.dnd:get() end,
    set = function(now) notifs.dnd:set(now) end,
  },
}

--- A quick toggle: a pill off, a rounded square on, morphing between the
--- two (and squaring up a little under the finger) as M3 expressive's
--- toggle buttons do.
local function toggle(t)
  local area
  local function on() return t.on() == true end
  area = ui.MouseArea {
    id = "utilities-toggle-" .. t.id,
    width = 56, height = 44, cursor = "pointer",
    on_clicked = function() t.set(not on()) end,
    ui.Rect {
      id = "utilities-toggle-" .. t.id .. "-shape",
      anchors = { fill = true },
      radius = function()
        if area and area.pressed then return 8 end
        return on() and 10 or 22
      end,
      color = function()
        local base = on() and C.primary or C.surfaceContainerHighest
        if area and area.hovered then return base:mix(on() and C.onPrimary or C.onSurface, 0.08) end
        return base
      end,
      behavior = {
        radius = ui.spring { stiffness = 480, damping = 34 },
        color = { duration = theme.duration.small },
      },
    },
    kit.icon(t.icon, 24, function() return on() and C.onPrimary or C.onSurfaceVariant end, {
      anchors = { center_in = true }, fill = t.fill or on,
      behavior = { color = { duration = theme.duration.small } },
    }),
  }
  return area
end

local function toggles()
  local buttons = {}
  for _, t in ipairs(M.TOGGLES) do buttons[#buttons + 1] = toggle(t) end
  return kit.card {
    id = "utilities-toggles",
    width = CARD_W, height = 111, radius = M.RADIUS,
    kit.text { x = 16, y = 16, text = "Quick toggles", font_size = theme.size.large },
    ui.Row { x = 16, y = 51, gap = 8, table.unpack(buttons) },
  }
end

-- ------------------------------------------------------------------ drawer --

local cards = { keep_awake(), recorder(), toggles() }

--- The drawer's height: the cards, their gaps and its padding.
function M.height()
  local h = TOP + BOTTOM + 2 * GAP + 111
  h = h + (M.awake:get() and 128 or 86)
  h = h + recorder_height()
  return h
end

local content = ui.Item {
  anchors = { fill = true },
  -- Behind everything, so the whole panel takes the pointer.
  ui.MouseArea { anchors = { fill = true }, z = -1 },
  ui.Column {
    x = LEFT, anchors = { bottom = true, bottom_margin = BOTTOM }, gap = GAP,
    table.unpack(cards),
  },
}

M.drawer = drawer.new {
  name = "utilities",
  edge = "bottom",
  width = M.WIDTH,
  height = M.height,
  content = content,
  props = { anchors = { bottom = true, right = true } },
}

-- Opening, the cards come in one after the other, each growing evenly
-- about its centre as it fades in; closing, they fade as the drawer goes.
local running = {}
morf.effect("caelestia.utilities.bud", function()
  local open = M.drawer.open:get()
  for _, h in ipairs(running) do h:stop() end
  if open then scan() else menu_open:set(false) end
  running = kit.bud(cards, open)
end)

return M
