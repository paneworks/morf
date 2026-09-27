-- caelestia's greeter, in two stages like the lock -- a phone's, and a
-- desk's too.
--
-- At rest: the time, the date, and the accounts as a row of cookies -- the
-- chosen one scalloped and turning -- over caelestia's shapes drifting on
-- a deep surface, the machine's name and the power buttons in the corners,
-- and a bud on the frame's bottom edge. The arrows or a click choose the
-- account; Return, a key, a click on the chosen one or a swipe up swells the
-- bud into the sheet: the account, the password pill, the session to start
-- (a chip; F2), what greetd says, and on a phone the on-screen keyboard.
-- Escape on an empty field sinks it back to choose someone else.
--
-- greetd runs it inside cage, as the user `greeter`:
--
--     [default_session]
--     command = "cage -s -- morf -c caelestia/greet"      (or a bundle of it)
--     user = "greeter"
--
-- Nested, in a session already logged into, it draws the same and says there
-- is no greetd to ask:   cage -- morf -c caelestia/greet
--
-- Its own file. What it shares with the lock is in the library: lib.auth
-- (greetd's conversation), lib.accounts, lib.sessions, lib.material, lib.osk.

local morf = require("morf")
local ui = require("morf.ui")
local material = require("lib.material")
local shapes = require("lib.m3shapes")
local accounts = require("lib.accounts")
local sessions = require("lib.sessions")
local auth = require("lib.auth")
local osk = require("lib.osk")

-- `-- preview`: as if a pattern were set, for pictures and tests.
local PREVIEW = morf.operands[1] == "preview"

local screen = morf.screens[1]
local W = (screen and screen.width) or 1920
local H = (screen and screen.height) or 1080
local S = math.max(0.75, math.min(2.4, math.min(W / 1920, H / 1080)))
local function s(n) return math.floor(n * S + 0.5) end

morf.surface.width = W
morf.surface.height = H
morf.surface.anchors = { top = true, left = true, right = true, bottom = true }
morf.surface.layer = "overlay"
morf.surface.keyboard_focus = "exclusive"
-- Colours mix as the shell's do: a translucent tint is as faint as it says.
morf.surface.blend = "srgb"
morf.surface.namespace = "caelestia-greet"

-- ----------------------------------------------------------------- colour --

-- The greeter's user has no desk of its own to take colours from: caelestia's
-- own blue, or whatever colour `/etc/morf/caelestia-accent` holds.
local accent = "#9ccbfb"
do
  local ok, words = pcall(morf.fs.read, "/etc/morf/caelestia-accent")
  local wanted = ok and type(words) == "string" and words:match("#%x%x%x%x%x%x")
  if wanted then accent = wanted end
end
local scheme = material.scheme(accent, { variant = "tonal_spot", mode = "dark" })
local C = setmetatable({}, { __index = function(_, role) return scheme[role] or morf.color("#888888") end })

-- The face the desk actually draws caelestia in (Google Sans Flex and Rubik
-- when installed; Roboto is what they fall back to here, and a bundle must
-- name a face it can carry).
local FONT = "Roboto"
local ICONS = "Material Symbols Rounded"

local function text(props)
  props.font_family = props.font_family or FONT
  props.font_size = props.font_size or s(15)
  if props.color == nil then props.color = C.onSurface end
  return ui.Text(props)
end
local function icon(name, size, color, props)
  props = props or {}
  props.text = name
  props.font_family = ICONS
  props.font_size = size
  props.color = color or C.onSurface
  props.axes = props.axes or { FILL = 1 }
  return ui.Text(props)
end

-- ------------------------------------------------------------------ state --

local people = accounts.list()
if #people == 0 then people = { { name = "", label = "Nobody to log in", initial = "?" } } end
local list = sessions.list()

local who = morf.signal("greet.who", 1)
local which = morf.signal("greet.session", sessions.default_index(list))
local password = ""
local typed = morf.signal("greet.typed", 0)
local busy = morf.signal("greet.busy", false)
local message = morf.signal("greet.message", "")
local bad = morf.signal("greet.bad", false)
local shake = morf.signal("greet.shake", 0)
-- "pattern" or "password": what the sheet takes (a pattern where one is set).
local method = morf.signal("greet.method", "password")
local has_pattern -- below, with the pattern pad
-- "closed" (coming in), "rest" (choosing), "sheet" (the way in is open),
-- "leaving" (the session is starting).
local stage = morf.signal("greet.stage", "closed")
local pull = morf.signal("greet.pull", 0)

local function person() return people[who:get()] or people[1] end
local function session() return list[which:get()] end

local function say(words, wrong)
  message:set(words or "")
  bad:set(wrong == true)
end
local function clear()
  password = ""
  typed:set(0)
end

local door = auth.greeter {
  user = person().name,
  session = session(),
  on_busy = function(b) busy:set(b) end,
  on_info = function(words, wrong) if not busy:get() then say(words, wrong) end end,
  on_failed = function(why)
    say(why ~= "" and why or "Wrong password", true)
    clear()
    shake:set(1)
    morf.timer(70, function() shake:set(0) end, false)
  end,
  -- The session is starting: everything sinks away while greetd replaces
  -- this process with it.
  on_open = function() stage:set("leaving") end,
}
local NOT_GREETD = "Not started by greetd: nothing to log in to"

local function choose(index)
  if index == who:get() or not people[index] then return end
  who:set(index)
  clear()
  say("")
  door:switch(person().name)
end
local function step_person(by)
  if #people < 2 then return end
  choose(((who:get() - 1 + by) % #people) + 1)
end
local function step_session(by)
  if #list < 2 then return end
  which:set(((which:get() - 1 + by) % #list) + 1)
  door.session = session()
end

local idle
local function poke()
  if idle then idle:cancel() end
  idle = morf.timer(30000, function()
    idle = nil
    if stage:get() == "sheet" and typed:get() == 0 and not busy:get() then
      stage:set("rest")
      say("")
    end
  end, false)
end
local function open_sheet()
  if stage:get() ~= "rest" then return end
  stage:set("sheet")
  pull:set(0)
  method:set(has_pattern() and "pattern" or "password")
  if not door.available then say(NOT_GREETD, false) end
  poke()
end

local function submit()
  if busy:get() or stage:get() ~= "sheet" then return end
  if not session() then
    say("No session installed to start", true)
    return
  end
  door.session = session()
  say("")
  door:submit(password)
end

local MAX_DOTS = 20
local function type_text(t)
  if #password >= 256 then return end
  if method:get() == "pattern" then method:set("password") clear() end
  password = password .. t
  typed:set(math.min(#password, MAX_DOTS))
  if bad:get() then say("") end
  poke()
end
local function backspace()
  password = password:sub(1, -2)
  typed:set(math.min(#password, MAX_DOTS))
  poke()
end
local function escape()
  if #password > 0 then clear() say("") return end
  if stage:get() == "sheet" then stage:set("rest") say("") end
end

local function power(method, words)
  local ok, why = auth.power(method)
  if not ok then say(why or ("Could not " .. words), true) end
end

-- --------------------------------------------------------------- the time --

local function now(format) return morf.time.format(format, morf.time.now()) end
local clock = morf.signal("greet.clock", now("%H:%M"))
local day = morf.signal("greet.day", now("%A, %-d %B"))
morf.timer(1000, function()
  clock:set(now("%H:%M"))
  day:set(now("%A, %-d %B"))
end, true)

-- -------------------------------------------------------------- geometry --

local BORDER = s(10)
local ROUND = s(25)
local PORTRAIT = H > W
-- The on-screen keyboard: on a phone, or wherever no keyboard is attached.
local ONSCREEN = PORTRAIT or not select(2, pcall(function() return require("lib.keyboards").attached() end))

local SW = math.min(s(600), W - 2 * s(20))
local AV = s(96)
local FIELD_W, FIELD_H = math.min(s(380), SW - s(48)), s(58)

-- The pattern (tools/pattern): offered where the stack takes one and one
-- is set for this account. Anywhere else a drawn pattern would only be a
-- wrong password, and a failed login counted.
local PATTERN_STACK = (function()
  local ok, t = pcall(morf.fs.read, "/etc/pam.d/greetd")
  return ok and type(t) == "string" and t:find("morf-pattern-check", 1, true) ~= nil
end)()
function has_pattern()
  if PREVIEW then return true end
  local name = person().name
  return PATTERN_STACK and name ~= nil and name ~= "" and morf.fs.exists("/etc/morf/pattern/" .. name)
end
local function look()
  return {
    panel = function() return C.surfaceContainer end,
    key = function() return C.surfaceContainerHighest end,
    key_dim = function() return C.surfaceContainerHigh end,
    accent = function() return C.primary end,
    on_accent = function() return C.onPrimary end,
    text = function() return C.onSurface end,
    dim = function() return C.onSurfaceVariant end,
    press = function() return C.secondaryContainer end,
    font = FONT, icons = ICONS,
  }
end
local PAD_W = math.min(s(300), SW - s(48))
local pad = osk.new {
  prefix = "greet.pattern", width = PAD_W, mode = "pattern", look = look(),
  on_pattern = function(dots)
    if busy:get() then return end
    -- A tap or two is not an attempt: nothing is spent on it.
    if #dots < 4 then say("Connect at least four dots", false) return end
    password = table.concat(dots)
    submit()
  end,
}
local function entry_h() return method:get() == "pattern" and pad.height() or FIELD_H end
local function chip_h() return has_pattern() and s(44) or 0 end
local kb
if ONSCREEN then
  kb = osk.new {
    prefix = "greet.osk", width = SW - s(24), mode = "full", numbers = true,
    look = {
      panel = function() return C.surfaceContainer end,
      key = function() return C.surfaceContainerHighest end,
      key_dim = function() return C.surfaceContainerHigh end,
      accent = function() return C.primary end,
      on_accent = function() return C.onPrimary end,
      text = function() return C.onSurface end,
      dim = function() return C.onSurfaceVariant end,
      press = function() return C.secondaryContainer end,
      font = FONT, icons = ICONS,
    },
    send = function(event)
      if event.text then type_text(event.text)
      elseif event.key == "backspace" then backspace()
      elseif event.key == "enter" then submit()
      elseif event.key == "escape" then escape() end
    end,
  }
end
local function kb_h() return (kb and method:get() == "password") and (kb.height() + s(16)) or 0 end
local function sheet_h()
  return s(28) + AV + s(12) + s(30) + s(20) + entry_h() + s(14) + s(40) + s(10) + s(24) + chip_h() + s(24) + kb_h()
end

local BUD_W, BUD_H = s(132), s(16)
local function up()
  local st = stage:get()
  if st == "sheet" then return 1 end
  if st == "rest" then return pull:get() end
  return 0
end
local function swell_h()
  local st = stage:get()
  if st == "closed" or st == "leaving" then return BORDER end
  return BORDER + BUD_H + (sheet_h() - BUD_H) * up()
end
local function swell_w()
  local st = stage:get()
  if st == "closed" or st == "leaving" then return BUD_W end
  return BUD_W + (SW - BUD_W) * up()
end
local GROW = { duration = 420, easing = "out_back" }
local SETTLE = { duration = 340, easing = "out_cubic" }
local function showing() return stage:get() == "rest" or stage:get() == "sheet" end

-- ------------------------------------------------------------- the frame --

local frame = ui.Sdf {
  anchors = { fill = true },
  ui.SdfShape {
    shape = "box", x = 0, y = 0, width = W, height = H,
    fill_color = function() return C.surface end,
  },
  ui.SdfShape {
    shape = "box", operation = "subtract", radius = ROUND,
    x = function() return stage:get() == "closed" and 0 or BORDER end,
    y = function() return stage:get() == "closed" and 0 or BORDER end,
    width = function() return stage:get() == "closed" and W or W - 2 * BORDER end,
    height = function() return stage:get() == "closed" and H or H - 2 * BORDER end,
    behavior = { x = SETTLE, y = SETTLE, width = SETTLE, height = SETTLE },
  },
}

-- The swell has a field of its own, in a band along the bottom edge: the
-- frame above stays still, and a swell growing redraws the band alone,
-- not the whole screen every frame (a 4K screen of field was the lag).
local function band_h() return sheet_h() + s(90) end
local band = ui.Item {
  x = 0, width = W,
  y = function() return H - band_h() end,
  height = band_h,
  ui.Sdf {
    anchors = { fill = true },
    -- The frame's bottom edge, for the swell to melt into.
    ui.SdfShape {
      shape = "box", x = 0, width = W,
      y = function() return band_h() - BORDER end,
      height = BORDER + s(40),
      fill_color = function() return C.surface end,
    },
    ui.SdfShape {
      id = "greet-swell",
      shape = "box", operation = "smooth_union", blend = s(26),
      radius = function() return up() > 0.5 and s(38) or s(12) end,
      fill_color = function() return up() > 0.5 and C.surfaceContainer or C.surface end,
      x = function() return math.floor((W - swell_w()) / 2) end,
      y = function() return band_h() - swell_h() end,
      width = swell_w,
      height = function() return swell_h() + s(40) end,
      behavior = { x = GROW, y = GROW, width = GROW, height = GROW, radius = SETTLE,
        fill_color = { duration = 240 } },
    },
  },
}

-- No wallpaper for the greeter's user: the screen behind the frame is a
-- deep surface with caelestia's shapes drifting across it.
local DRIFT = {
  { "cookie9", 0.08, 0.14, 180, 0 }, { "clover4", 0.82, 0.12, 150, 30 }, { "pentagon", 0.14, 0.74, 200, 8 },
  { "cookie12", 0.86, 0.72, 220, 0 }, { "gem", 0.30, 0.40, 110, -12 }, { "flower", 0.68, 0.44, 130, 0 },
  { "sunny", 0.50, 0.86, 120, 0 }, { "pill", 0.44, 0.10, 140, 25 },
}
local drift = { anchors = { fill = true } }
for i, d in ipairs(DRIFT) do
  local size = s(d[4])
  drift[#drift + 1] = ui.Path {
    x = math.floor(W * d[2] - size / 2), y = math.floor(H * d[3] - size / 2),
    width = size, height = size, view_box = { 0, 0, 100, 100 },
    d = shapes.path(d[1], { segments = false }), rotation = d[5],
    fill_color = function() return C.primary:alpha(0.05) end,
    loop = { rotation = { from = d[5], to = d[5] + (i % 2 == 0 and 360 or -360), duration = 90000 + i * 9000 } },
  }
end
local backdrop = ui.Item {
  anchors = { fill = true },
  ui.Rect { anchors = { fill = true }, color = function() return C.surfaceContainerLowest end },
  ui.Item(drift),
}

-- ---------------------------------------------------------------- pieces --

local function round_button(id, name, on_clicked, size)
  size = size or s(44)
  local area
  area = ui.MouseArea {
    id = id, width = size, height = size, cursor = "pointer",
    on_clicked = on_clicked,
    scale = function() return (area and area.pressed) and 0.9 or 1 end,
    behavior = { scale = ui.spring { stiffness = 700, damping = 18 } },
    ui.Rect {
      anchors = { fill = true }, radius = size / 2,
      color = function()
        return (area and area.hovered) and C.surfaceContainerHighest or C.surfaceContainerHigh
      end,
      behavior = { color = { duration = 160 } },
    },
    icon(name, math.floor(size * 0.5), C.onSurface, { anchors = { center_in = true } }),
  }
  return area
end

--- An account's face in a cookie (or its initial on one), `size` across.
local function face(p, size, shape, fill, ink)
  local path = shapes.path(shape, { segments = false })
  return ui.Item {
    width = size, height = size,
    ui.Path {
      anchors = { fill = true }, view_box = { 0, 0, 100, 100 }, d = path,
      fill_color = fill,
    },
    p.face and ui.Image {
      anchors = { fill = true }, fill_mode = "preserve_aspect_crop", source = p.face,
      mask = ui.Path { anchors = { fill = true }, view_box = { 0, 0, 100, 100 }, d = path, fill_color = "#ffffff" },
    } or text {
      anchors = { center_in = true }, font_size = math.floor(size * 0.42), font_weight = 600,
      color = ink, text = p.initial or "?",
    },
  }
end

-- ------------------------------------------------------------ at rest --

-- The accounts, a row of cookies: the chosen one larger, scalloped and
-- turning, its name bold under it.
local PEOPLE_AV = s(84)
local row = { gap = s(28), align = "start" }
for index, p in ipairs(people) do
  local chosen = function() return who:get() == index end
  local area
  area = ui.MouseArea {
    id = "greet-person-" .. index, width = PEOPLE_AV + s(40), height = PEOPLE_AV + s(44), cursor = "pointer",
    on_clicked = function()
      if chosen() then open_sheet() else choose(index) end
    end,
    ui.Item {
      anchors = { horizontal_center = true }, width = PEOPLE_AV, height = PEOPLE_AV,
      scale = function()
        if chosen() then return 1.12 end
        return (area and area.hovered) and 1.04 or 0.9
      end,
      behavior = { scale = ui.spring { stiffness = 420, damping = 16 } },
      ui.Item {
        anchors = { fill = true },
        loop = function()
          if not chosen() then return nil end
          return { rotation = { to = 360, duration = 40000, hold = true } }
        end,
        shapes.Shape {
          anchors = { fill = true },
          shape = function() return chosen() and "cookie12" or "circle" end,
          color = function() return chosen() and C.primaryContainer or C.surfaceContainerHigh end,
          duration = 450, easing = "out_back",
        },
      },
      p.face and ui.Image {
        anchors = { fill = true }, fill_mode = "preserve_aspect_crop", source = p.face,
        mask = ui.Path { anchors = { fill = true }, view_box = { 0, 0, 100, 100 },
          d = shapes.path("circle", { segments = false }), fill_color = "#ffffff" },
      } or text {
        anchors = { center_in = true }, font_size = s(34), font_weight = 600,
        color = function() return chosen() and C.onPrimaryContainer or C.onSurfaceVariant end,
        text = p.initial or "?",
      },
    },
    text {
      anchors = { horizontal_center = true, bottom = true }, width = PEOPLE_AV + s(40),
      horizontal_alignment = "center", elide = "right", font_size = s(15),
      font_weight = function() return chosen() and 600 or 400 end,
      color = function() return chosen() and C.onSurface or C.onSurfaceVariant end,
      text = p.label ~= "" and p.label or p.name,
    },
  }
  row[#row + 1] = area
end

local CLOCK_Y = PORTRAIT and math.floor(H * 0.12) or math.floor(H * 0.18)
local glance = ui.Column {
  id = "greet-glance",
  anchors = { horizontal_center = true }, gap = s(6), align = "center",
  y = function()
    if stage:get() == "sheet" then
      return math.max(s(40), math.floor((H - BORDER - sheet_h()) / 2 - s(110)))
    end
    return CLOCK_Y
  end,
  scale = function() return stage:get() == "sheet" and 0.72 or 1 end,
  opacity = function() return showing() and 1 or 0 end,
  behavior = { y = GROW, scale = GROW, opacity = { duration = 320 } },
  text { id = "greet-clock", text = function() return clock:get() end,
    font_size = PORTRAIT and s(132) or s(160), font_weight = 600, color = C.primary },
  text { text = function() return day:get() end, font_size = s(22), color = C.onSurfaceVariant },
}
-- The accounts go when the sheet comes: it carries the chosen one.
local chooser = ui.Row(row)
local choosing = ui.Item {
  id = "greet-people",
  anchors = { horizontal_center = true },
  width = function() return chooser.layout_width or 0 end,
  height = PEOPLE_AV + s(44),
  y = CLOCK_Y + (PORTRAIT and s(210) or s(250)),
  opacity = function() return stage:get() == "rest" and 1 or 0 end,
  translate_y = function() return stage:get() == "rest" and 0 or s(40) end,
  behavior = { opacity = { duration = 260 }, translate_y = GROW },
  chooser,
}

local hint = ui.Column {
  anchors = { horizontal_center = true },
  y = H - BORDER - BUD_H - s(74), gap = s(2), align = "center",
  opacity = function() return (stage:get() == "rest" and pull:get() < 0.1) and 1 or 0 end,
  behavior = { opacity = { duration = 260 } },
  icon("keyboard_arrow_up", s(30), function() return C.onSurfaceVariant end, {
    loop = { translate_y = { from = 0, to = -s(6), duration = 900, alternate = true, easing = "in_out_sine" } },
  }),
  text {
    text = ONSCREEN and "Swipe up to log in" or "Press Enter to log in",
    font_size = s(14), color = function() return C.onSurfaceVariant end,
  },
}

-- Power, in the top-right corner of the frame; the machine's name, top-left.
local power_row = ui.Row {
  anchors = { right = true, right_margin = BORDER + s(24), top = true, top_margin = BORDER + s(22) },
  gap = s(10),
  opacity = function() return showing() and 1 or 0 end,
  behavior = { opacity = { duration = 320, delay = 200 } },
  round_button("greet-suspend", "bedtime", function() power("Suspend", "suspend") end),
  round_button("greet-reboot", "restart_alt", function() power("Reboot", "reboot") end),
  round_button("greet-poweroff", "power_settings_new", function() power("PowerOff", "power off") end),
}
local host = ""
do
  local ok, name = pcall(morf.fs.read, "/proc/sys/kernel/hostname")
  if ok and type(name) == "string" then host = name:match("^%s*(.-)%s*$") end
end
local host_label = ui.Row {
  x = BORDER + s(28), y = BORDER + s(30), gap = s(10), align = "center",
  opacity = function() return showing() and 1 or 0 end,
  behavior = { opacity = { duration = 320, delay = 200 } },
  icon("computer", s(22), C.onSurfaceVariant),
  text { font_size = s(18), font_weight = 600, color = C.onSurfaceVariant, text = host },
}

-- -------------------------------------------------------------- the sheet --

local DOT = s(12)
local dots = {}
for i = 1, MAX_DOTS do
  dots[i] = ui.Rect {
    width = DOT, height = DOT, radius = DOT / 2,
    color = function() return C.primary end,
    visible = function() return typed:get() >= i end,
    scale = function() return typed:get() >= i and 1 or 0 end,
    behavior = { scale = { duration = 260, easing = "out_back" } },
  }
end
local field = ui.Item {
  id = "greet-field",
  width = FIELD_W, height = FIELD_H,
  translate_x = function() return shake:get() == 1 and s(12) or 0 end,
  behavior = { translate_x = ui.spring { stiffness = 900, damping = 9 } },
  ui.Rect {
    anchors = { fill = true }, radius = FIELD_H / 2,
    color = function() return C.surfaceContainerHighest end,
    border_width = function() return bad:get() and s(2) or 0 end,
    border_color = function() return C.error end,
  },
  icon(function() return busy:get() and "hourglass" or "key" end, s(22), C.onSurfaceVariant,
    { x = s(20), anchors = { vertical_center = true } }),
  text {
    anchors = { vertical_center = true }, x = s(56),
    text = "Password", color = C.onSurfaceVariant, font_size = s(16),
    visible = function() return typed:get() == 0 end,
  },
  ui.Row { x = s(56), anchors = { vertical_center = true }, gap = s(7), table.unpack(dots) },
  ui.MouseArea {
    id = "greet-submit",
    anchors = { right = true, right_margin = s(7), vertical_center = true },
    width = FIELD_H - s(14), height = FIELD_H - s(14), cursor = "pointer",
    on_clicked = submit,
    ui.Rect {
      anchors = { fill = true }, radius = (FIELD_H - s(14)) / 2,
      color = function() return typed:get() > 0 and C.primary or C.surfaceContainerHigh end,
      behavior = { color = { duration = 200 } },
    },
    icon("arrow_forward", s(22), function() return typed:get() > 0 and C.onPrimary or C.onSurfaceVariant end,
      { anchors = { center_in = true } }),
  },
}

-- The session to start: a chip; a click (or F2) moves to the next.
local session_chip = ui.MouseArea {
  id = "greet-session",
  width = s(260), height = s(40), cursor = "pointer",
  on_clicked = function() step_session(1) end,
  ui.Rect { anchors = { fill = true }, radius = s(20), color = function() return C.secondaryContainer end },
  ui.Row {
    anchors = { center_in = true }, gap = s(8), align = "center",
    icon("desktop_windows", s(18), C.onSecondaryContainer),
    text {
      font_size = s(15), font_weight = 500, color = C.onSecondaryContainer,
      text = function()
        local sn = session()
        return sn and sn.name or "No sessions"
      end,
    },
    icon("unfold_more", s(18), C.onSecondaryContainer, { visible = #list > 1 }),
  },
}

-- The chosen account, on the sheet: one face per account, the chosen shown.
local sheet_faces = { width = AV, height = AV }
for index, p in ipairs(people) do
  local f = face(p, AV, "cookie9", function() return C.primaryContainer end, C.onPrimaryContainer)
  f.visible = function() return who:get() == index end
  sheet_faces[#sheet_faces + 1] = f
end
local sheet_avatar = ui.Item(sheet_faces)

local sheet_nodes = {
  x = s(12), y = s(28), width = SW - s(24), gap = 0, align = "center",
  sheet_avatar,
  ui.Item { width = 1, height = s(12) },
  text { id = "greet-name", font_size = s(20), font_weight = 600, height = s(30),
    text = function() local p = person() return p.label ~= "" and p.label or p.name end },
  ui.Item { width = 1, height = s(20) },
  ui.Item {
    width = SW - s(24), height = entry_h,
    ui.Item { anchors = { horizontal_center = true }, width = FIELD_W, height = FIELD_H,
      visible = function() return method:get() == "password" end, field },
    ui.Item { anchors = { horizontal_center = true }, width = PAD_W, height = pad.height,
      visible = function() return method:get() == "pattern" end, pad.node },
  },
  ui.Item { width = 1, height = s(14) },
  session_chip,
  ui.Item { width = 1, height = s(10) },
  text {
    id = "greet-message", height = s(24), width = SW - s(48), horizontal_alignment = "center", elide = "right",
    text = function() return message:get() end, font_size = s(15),
    color = function() return bad:get() and C.error or C.onSurfaceVariant end,
  },
  ui.Item {
    width = SW - s(24), height = chip_h, visible = has_pattern,
    (function()
      local area
      area = ui.MouseArea {
        id = "greet-method", anchors = { horizontal_center = true, bottom = true },
        width = s(180), height = s(36), cursor = "pointer",
        on_clicked = function()
          method:set(method:get() == "pattern" and "password" or "pattern")
          clear()
          say("")
        end,
        ui.Rect {
          anchors = { fill = true }, radius = s(18),
          color = function() return (area and area.hovered) and C.surfaceContainerHighest or C.surfaceContainerHigh end,
        },
        ui.Row {
          anchors = { center_in = true }, gap = s(6), align = "center",
          icon(function() return method:get() == "pattern" and "password" or "pattern" end, s(18), C.onSurfaceVariant),
          text { font_size = s(14), color = C.onSurfaceVariant,
            text = function() return method:get() == "pattern" and "Use password" or "Use pattern" end },
        },
      }
      return area
    end)(),
  },
  ui.Item { width = 1, height = s(24) },
}
if kb then
  sheet_nodes[#sheet_nodes + 1] = ui.Item {
    width = SW - s(24),
    height = function() return method:get() == "password" and kb.height() or 0 end,
    visible = function() return method:get() == "password" end,
    kb.node,
  }
end
local sheet = ui.Item {
  id = "greet-sheet",
  x = math.floor((W - SW) / 2), width = SW,
  y = function() return H - BORDER - sheet_h() end,
  height = sheet_h,
  opacity = function() return stage:get() == "sheet" and 1 or 0 end,
  translate_y = function() return stage:get() == "sheet" and 0 or s(60) end,
  behavior = { opacity = { duration = 260, delay = 120 }, translate_y = GROW },
  visible = function() return stage:get() == "sheet" or stage:get() == "leaving" end,
  ui.Column(sheet_nodes),
}

-- ------------------------------------------------------------ the screen --

ui.Item {
  anchors = { fill = true },
  backdrop,
  frame,
  band,
  glance,
  choosing,
  hint,
  sheet,
  power_row,
  host_label,
  ui.MouseArea {
    id = "greet-open",
    anchors = { fill = true }, z = -1,
    on_clicked = function() open_sheet() end,
    on_dragged = function(_, _, _, dy)
      if stage:get() ~= "rest" then return end
      pull:set(math.max(0, math.min(1, -dy / s(360))))
    end,
    on_drag_finished = function()
      if stage:get() ~= "rest" then return end
      if pull:get() > 0.3 then open_sheet() else pull:set(0) end
    end,
    on_key_pressed = function(keysym, typed_text)
      local RETURN, KP_ENTER, BACKSPACE, ESCAPE = 0xff0d, 0xff8d, 0xff08, 0xff1b
      local LEFT, RIGHT, F2 = 0xff51, 0xff53, 0xffbf
      local st = stage:get()
      if st ~= "rest" and st ~= "sheet" then return end
      if busy:get() then return end
      if keysym == ESCAPE then escape() return end
      if keysym == F2 then step_session(1) return end
      if st == "rest" then
        if keysym == LEFT then step_person(-1) return end
        if keysym == RIGHT then step_person(1) return end
        open_sheet()
        -- A character is the password's first; space, Return and the rest
        -- only open the sheet.
        if not (typed_text and typed_text ~= "" and typed_text:byte(1) > 32) then return end
      end
      if keysym == RETURN or keysym == KP_ENTER then
        submit()
      elseif keysym == BACKSPACE then
        backspace()
      elseif typed_text and typed_text ~= "" and typed_text:byte(1) >= 32 then
        type_text(typed_text)
      end
    end,
  },
}

-- For a test or a picture: `morf ipc call stage sheet`.
morf.ipc.stage = function(to)
  if to == "sheet" then stage:set("rest") open_sheet() elseif to == "rest" then stage:set("rest") end
  return stage:get()
end

morf.timer(30, function() stage:set("rest") end, false)
