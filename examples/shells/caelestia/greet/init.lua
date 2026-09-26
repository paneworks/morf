-- caelestia's greeter: the login screen, in the lock's language -- the
-- frame round the screen, a panel risen out of its bottom edge, the time,
-- the account in a cookie and a pill for the password -- and what a login
-- needs besides: which account (the arrows, or a click on the name), which
-- session (the chip under the field, or F2), and the power buttons in the
-- top corner.
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
-- (greetd's conversation), lib.accounts, lib.sessions, lib.material.

local morf = require("morf")
local ui = require("morf.ui")
local material = require("lib.material")
local shapes = require("lib.m3shapes")
local accounts = require("lib.accounts")
local sessions = require("lib.sessions")
local auth = require("lib.auth")

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
local phase = morf.signal("greet.phase", "closed")

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
    shake:set(shake:get() + 1)
  end,
  -- The session is starting: everything sinks away while greetd replaces
  -- this process with it.
  on_open = function() phase:set("leaving") end,
}
if not door.available then say("Not started by greetd: nothing to log in to", false) end

local function step_person(by)
  if #people < 2 then return end
  who:set(((who:get() - 1 + by) % #people) + 1)
  clear()
  say("")
  door:switch(person().name)
end
local function step_session(by)
  if #list < 2 then return end
  which:set(((which:get() - 1 + by) % #list) + 1)
  door.session = session()
end

local function submit()
  if busy:get() or phase:get() ~= "in" then return end
  if not session() then
    say("No session installed to start", true)
    return
  end
  door.session = session()
  say("")
  door:submit(password)
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

-- ------------------------------------------------------------- the frame --

local BORDER = s(10)
local ROUND = s(25)
local PW, PH = s(760), s(640)
local PX, PY = math.floor((W - PW) / 2), math.floor((H - PH) / 2)
local GROW = { duration = 620, easing = "out_back" }
local function panel_open() return phase:get() == "in" end

local frame = ui.Sdf {
  anchors = { fill = true },
  ui.SdfShape {
    shape = "box", x = 0, y = 0, width = W, height = H,
    fill_color = function() return C.surface end,
  },
  ui.SdfShape {
    shape = "box", operation = "subtract", radius = ROUND,
    x = BORDER, y = BORDER, width = W - 2 * BORDER, height = H - 2 * BORDER,
  },
  ui.SdfShape {
    shape = "box", operation = "smooth_union", blend = s(28), radius = s(36),
    fill_color = function() return C.surfaceContainer end,
    x = function() return panel_open() and PX or math.floor(W / 2 - s(90)) end,
    y = function() return panel_open() and PY or H - BORDER end,
    width = function() return panel_open() and PW or s(180) end,
    height = function() return panel_open() and PH or BORDER end,
    behavior = { x = GROW, y = GROW, width = GROW, height = GROW },
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
    fill_color = function() return C.primary:alpha(0.035) end,
    loop = { rotation = { from = d[5], to = d[5] + (i % 2 == 0 and 360 or -360), duration = 90000 + i * 9000 } },
  }
end
local backdrop = ui.Item {
  anchors = { fill = true },
  ui.Rect { anchors = { fill = true }, color = function() return C.surfaceContainerLowest end },
  ui.Item(drift),
}

-- --------------------------------------------------------------- content --

local function shown(delay)
  return {
    opacity = function() return panel_open() and 1 or 0 end,
    behavior = { opacity = { duration = 320, delay = delay or 260 } },
  }
end
local function with(base, extra)
  for k, v in pairs(extra) do base[k] = v end
  return base
end

local function round_button(id, name, on_clicked, size, props)
  size = size or s(44)
  local area = ui.MouseArea {
    id = id, width = size, height = size, cursor = "pointer",
    on_clicked = on_clicked,
    ui.Rect {
      anchors = { fill = true }, radius = size / 2,
      color = function() return C.surfaceContainerHighest end,
    },
    icon(name, math.floor(size * 0.5), C.onSurface, { anchors = { center_in = true } }),
  }
  for k, v in pairs(props or {}) do area[k] = v end
  return area
end

local AV = s(132)
local cookie = shapes.path("cookie9", { segments = false })
local avatar = ui.Item {
  width = AV, height = AV,
  -- Only the cookie turns: a face or an initial stays upright on it.
  ui.Path {
    anchors = { fill = true }, view_box = { 0, 0, 100, 100 }, d = cookie,
    fill_color = function() return C.primaryContainer end,
    loop = { rotation = { to = 360, duration = 80000 } },
  },
  ui.Image {
    anchors = { fill = true }, fill_mode = "preserve_aspect_crop",
    source = function() return person().face or "" end,
    visible = function() return person().face ~= nil end,
    mask = ui.Path { anchors = { fill = true }, view_box = { 0, 0, 100, 100 }, d = cookie, fill_color = "#ffffff" },
  },
  text {
    anchors = { center_in = true }, font_size = s(56), font_weight = 600, color = C.onPrimaryContainer,
    text = function() return person().initial or "?" end,
    visible = function() return person().face == nil end,
  },
}

-- The account, with arrows either side when there is more than one.
local chooser = ui.Row {
  gap = s(18), align = "center",
  round_button("greet-person-previous", "chevron_left", function() step_person(-1) end, s(40),
    { visible = #people > 1 }),
  avatar,
  round_button("greet-person-next", "chevron_right", function() step_person(1) end, s(40),
    { visible = #people > 1 }),
}

local FIELD_W, FIELD_H = s(380), s(58)
local DOT = s(12)
local MAX_DOTS = 20
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
  translate_x = function() return (shake:get() % 2 == 1) and s(10) or 0 end,
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

local centre = ui.Column(with({
  anchors = { horizontal_center = true }, y = PY + s(44), gap = s(4), align = "center",
  text { id = "greet-clock", text = function() return clock:get() end, font_size = s(96), font_weight = 600, color = C.primary },
  text { text = function() return day:get() end, font_size = s(20), color = C.onSurfaceVariant },
  ui.Item { width = 1, height = s(24) },
  chooser,
  ui.Item { width = 1, height = s(10) },
  text { id = "greet-name", font_size = s(24), font_weight = 600, text = function() return person().label end },
  ui.Item { width = 1, height = s(20) },
  field,
  ui.Item { width = 1, height = s(14) },
  session_chip,
  ui.Item { width = 1, height = s(10) },
  text {
    id = "greet-message", height = s(22), font_size = s(15),
    text = function() return message:get() end,
    color = function() return bad:get() and C.error or C.onSurfaceVariant end,
  },
}, shown(240)))

-- Power, in the top-right corner of the frame.
local power_row = ui.Row(with({
  anchors = { right = true, right_margin = BORDER + s(24), top = true, top_margin = BORDER + s(22) },
  gap = s(10),
  round_button("greet-suspend", "bedtime", function() power("Suspend", "suspend") end),
  round_button("greet-reboot", "restart_alt", function() power("Reboot", "reboot") end),
  round_button("greet-poweroff", "power_settings_new", function() power("PowerOff", "power off") end),
}, shown(420)))

-- The machine's name, top-left.
local host = ""
do
  local ok, name = pcall(morf.fs.read, "/proc/sys/kernel/hostname")
  if ok and type(name) == "string" then host = name:match("^%s*(.-)%s*$") end
end
local host_label = text(with({
  x = BORDER + s(28), y = BORDER + s(30), font_size = s(18), font_weight = 600,
  color = C.onSurfaceVariant, text = host,
}, shown(420)))

-- ------------------------------------------------------------ the screen --

ui.Item {
  anchors = { fill = true },
  backdrop,
  frame,
  centre,
  power_row,
  host_label,
  ui.MouseArea {
    anchors = { fill = true }, z = -1,
    on_key_pressed = function(keysym, typed_text)
      local RETURN, KP_ENTER, BACKSPACE, ESCAPE = 0xff0d, 0xff8d, 0xff08, 0xff1b
      local LEFT, RIGHT, F2 = 0xff51, 0xff53, 0xffbf
      if phase:get() ~= "in" or busy:get() then return end
      if keysym == RETURN or keysym == KP_ENTER then
        submit()
      elseif keysym == BACKSPACE then
        password = password:sub(1, -2)
        typed:set(math.min(#password, MAX_DOTS))
      elseif keysym == ESCAPE then
        clear()
        say("")
      elseif keysym == LEFT and #password == 0 then
        step_person(-1)
      elseif keysym == RIGHT and #password == 0 then
        step_person(1)
      elseif keysym == F2 then
        step_session(1)
      elseif typed_text and typed_text ~= "" and typed_text:byte(1) >= 32 then
        password = password .. typed_text
        typed:set(math.min(#password, MAX_DOTS))
        if bad:get() then say("") end
      end
    end,
  },
}

morf.timer(30, function() phase:set("in") end, false)
