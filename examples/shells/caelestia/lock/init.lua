-- caelestia's lock screen, in two stages -- a phone's, and a desk's too.
--
-- At rest it is a thing to look at: the time, large, the date and the
-- weather under it, whatever is playing as a row with its controls, and a
-- small swell on the frame's bottom edge saying where the way in is. A key,
-- a click or a swipe up and that swell rises into the unlock sheet -- one
-- liquid surface with the frame -- carrying the account in its cookie, the
-- pill for the password and whatever PAM has to say (a face being looked
-- for, a finger). The first key typed is already the password's. Escape on
-- an empty field, or a while with nothing typed, and the sheet sinks back.
-- On a phone (a screen taller than wide, or no keyboard attached) the sheet
-- carries the on-screen keyboard. The right password and all of it sinks
-- into the edge, the frame opens, and the desk is there again.
--
--   morf -c caelestia/lock               the lock, held under ext-session-lock
--   morf -c caelestia/lock -- window     the same, in a window, holding nothing
--
-- Its own file: nothing here reaches into the shell's folder. What the lock
-- and the greeter share is in the library -- lib.auth (PAM and greetd),
-- lib.accounts, lib.lule and lib.material for the colours, lib.osk.

local morf = require("morf")
local ui = require("morf.ui")
local lule = require("lib.lule")
local material = require("lib.material")
local shapes = require("lib.m3shapes")
local accounts = require("lib.accounts")
local auth = require("lib.auth")
local osk = require("lib.osk")

local HELD = morf.operands[1] ~= "window"
-- `-- window preview`: never asks PAM -- any password but "wrong" opens it.
-- For pictures and tests: a lock that asked PAM and was killed mid-way
-- would count as a failed login.
local PREVIEW = not HELD and morf.operands[2] == "preview"

local screen = morf.screens[1]
local W = (screen and screen.width) or 1920
local H = (screen and screen.height) or 1080
-- Everything in proportion to a 1080p screen.
local S = math.max(0.75, math.min(2.4, math.min(W / 1920, H / 1080)))
local function s(n) return math.floor(n * S + 0.5) end

morf.surface.width = W
morf.surface.height = H
morf.surface.anchors = { top = true, left = true, right = true, bottom = true }
morf.surface.layer = "overlay"
morf.surface.keyboard_focus = "exclusive"
-- Colours mix as the shell's do: a translucent tint is as faint as it says.
morf.surface.blend = "srgb"
morf.surface.namespace = "caelestia-lock"
morf.surface.session_lock = HELD

-- ----------------------------------------------------------------- colour --

-- The desk's own scheme: Material from lule's accent, in lule's mode, as the
-- shell builds it; a calm blue when lule has not run.
local tool = lule.read()
local scheme = material.scheme(tool and tool.accent or "#9ccbfb",
  { variant = "tonal_spot", mode = tool and tool.theme or "dark" })
local C = setmetatable({}, { __index = function(_, role) return scheme[role] or morf.color("#888888") end })
local WALLPAPER = tool and tool.wallpaper or ""
if WALLPAPER ~= "" and not morf.fs.exists(WALLPAPER) then WALLPAPER = "" end

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

local me = accounts.me() or { name = "", label = "", initial = "?" }
-- The password is a plain local: never a signal, never kept. The screen
-- needs only how many characters it has.
local password = ""
local typed = morf.signal("lock.typed", 0)
local busy = morf.signal("lock.busy", false)
local message = morf.signal("lock.message", "")
local bad = morf.signal("lock.bad", false)
local shake = morf.signal("lock.shake", 0)
-- "pattern" or "password": what the sheet takes (a pattern where one is set).
local method = morf.signal("lock.method", "password")
local has_pattern -- below, with the pattern pad
-- "closed" (coming in), "rest", "sheet" (the way in is open), "opening"
-- (unlocked: all of it going away).
local stage = morf.signal("lock.stage", "closed")
-- How far a swipe has pulled the sheet up, 0..1, while it is being drawn.
local pull = morf.signal("lock.pull", 0)

local function say(words, wrong)
  message:set(words or "")
  bad:set(wrong == true)
end
local function clear()
  password = ""
  typed:set(0)
end

local door
local function lift()
  stage:set("opening")
  say("")
  morf.timer(560, function()
    if HELD then morf.surface.session_lock = false else morf.quit() end
  end, false)
end
local handlers = {
  user = me.name,
  listen = false,
  on_busy = function(b) busy:set(b) end,
  on_info = function(words, wrong) if not busy:get() then say(words, wrong) end end,
  on_failed = function(why)
    say(why ~= "" and why or "Wrong password", true)
    clear()
    shake:set(1)
    morf.timer(70, function() shake:set(0) end, false)
  end,
  on_open = lift,
}
if PREVIEW then
  door = {
    submit = function(_, pw)
      handlers.on_busy(true)
      morf.timer(700, function()
        handlers.on_busy(false)
        if pw == "wrong" then handlers.on_failed("Wrong password") else lift() end
      end, false)
    end,
    listen = function() end, stop = function() end,
  }
else
  door = auth.lock(handlers)
end

-- Back to rest after a while with the sheet up and nothing typed.
local idle
local function poke()
  if idle then idle:cancel() end
  idle = morf.timer(15000, function()
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
  poke()
end

local function submit()
  if busy:get() or stage:get() ~= "sheet" then return end
  if password == "" then
    say("Type your password", false)
    return
  end
  say("")
  door:submit(password)
end

local MAX_DOTS = 18
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
  if #password == 0 then
    -- Looked at in a window, it holds nothing: Escape at rest is the way
    -- out, in case the door will not open.
    if stage:get() == "rest" and not HELD then morf.quit() return end
    stage:set("rest")
    say("")
  else
    clear()
    say("")
  end
end

-- --------------------------------------------------------------- the time --

local function now(format) return morf.time.format(format, morf.time.now()) end
local clock = morf.signal("lock.clock", now("%H:%M"))
local day = morf.signal("lock.day", now("%A, %-d %B"))
morf.timer(1000, function()
  clock:set(now("%H:%M"))
  day:set(now("%A, %-d %B"))
end, true)

-- The pattern, and the sources every screen's tree shares.
-- The pattern (tools/pattern): offered where the stack takes one and one
-- is set for this account. Anywhere else a drawn pattern would only be a
-- wrong password, and a failed login counted.
local PATTERN_STACK = (function()
  local ok, t = pcall(morf.fs.read, "/etc/pam.d/morf-lock")
  return ok and type(t) == "string" and t:find("morf-pattern-check", 1, true) ~= nil
end)()
function has_pattern()
  if PREVIEW then return true end
  local name = me.name
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
local weather_source, media_source

-- The screen the lock is used on; the others show the frame, the desk and
-- the time, and nothing to press. A laptop's own panel (eDP, LVDS; DSI on
-- a phone) whatever else is plugged in; with none -- a desk PC, or a
-- laptop shut -- the monitor in focus, where the compositor says (Hyprland
-- here), else the first.
local function built_in(name)
  name = tostring(name or "")
  return name:match("^eDP") ~= nil or name:match("^LVDS") ~= nil or name:match("^DSI") ~= nil
end
local main_output = morf.signal("lock.main", (function()
  for _, sc in ipairs(morf.screens or {}) do
    if built_in(sc.name) then return sc.name end
  end
  return (morf.screens and morf.screens[1] and morf.screens[1].name) or ""
end)())
if not built_in(main_output:get()) then
  pcall(function()
    require("lib.hyprland").json("monitors", function(list)
      for _, m in ipairs(type(list) == "table" and list or {}) do
        if m.focused and m.name then main_output:set(m.name) end
      end
    end)
  end)
end

-- --------------------------------------------------------------- a screen --

-- One tree per screen, at that screen's size: a held lock covers every
-- output, and a laptop's panel and a 4K monitor each get their own layout.
-- What they share -- the password, the stage, the door -- is above.
local function build(W, H, NAME)
  -- In a window there is one screen, and it is the one.
  local function main() return not HELD or main_output:get() == NAME end
  local S = math.max(0.75, math.min(2.4, math.min(W / 1920, H / 1080)))
  local function s(n) return math.floor(n * S + 0.5) end
  local base_text = text
  local function text(props)
    props.font_size = props.font_size or s(15)
    return base_text(props)
  end

  -- -------------------------------------------------------------- geometry --

  local BORDER = s(10)
  local ROUND = s(25)
  local PORTRAIT = H > W
  -- The on-screen keyboard: on a phone, or wherever no keyboard is attached.
  local ONSCREEN = PORTRAIT or not select(2, pcall(function() return require("lib.keyboards").attached() end))

  local SW = math.min(s(600), W - 2 * s(20))
  local AV = s(96)
  local FIELD_W, FIELD_H = math.min(s(380), SW - s(48)), s(58)

  local PAD_W = math.min(s(300), SW - s(48))
  local pad = osk.new {
    prefix = "lock.pattern." .. NAME, width = PAD_W, mode = "pattern", look = look(),
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
      prefix = "lock.osk." .. NAME, width = SW - s(24), mode = "full", numbers = true,
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
  -- The sheet: the account, the pill, a line for what PAM says; the keyboard
  -- under them on a phone.
  local function sheet_h()
    return s(28) + AV + s(12) + s(30) + s(20) + entry_h() + s(10) + s(24) + chip_h() + s(24) + kb_h()
  end

  -- The swell's height as it stands: a bud at rest, the sheet up, a swipe
  -- in between.
  local BUD_W, BUD_H = s(132), s(16)
  local function up()
    if not main() then return 0 end
    local st = stage:get()
    if st == "sheet" then return 1 end
    if st == "rest" then return pull:get() end
    return 0
  end
  local function swell_h()
    local st = stage:get()
    if st == "closed" or st == "opening" or not main() then return BORDER end
    return BORDER + BUD_H + (sheet_h() - BUD_H) * up()
  end
  local function swell_w()
    local st = stage:get()
    if st == "closed" or st == "opening" then return BUD_W end
    return BUD_W + (SW - BUD_W) * up()
  end
  local GROW = { duration = 560, easing = "out_back" }
  local SETTLE = { duration = 420, easing = "out_cubic" }
  -- Follows a finger at once, and eases the rest of the way.
  local function swell_motion() return pull:get() > 0 and stage:get() == "rest" and { duration = 60 } or GROW end

  -- ------------------------------------------------------------- the frame --

  -- The frame and the swell are one distance field: the swell is a box that
  -- rises out of the frame's bottom edge and melts into it where they meet.
  local frame = ui.Sdf {
    anchors = { fill = true },
    ui.SdfShape {
      shape = "box", x = 0, y = 0, width = W, height = H,
      fill_color = function() return C.surface end,
    },
    -- The opening in the frame: shut while the lock comes in and goes.
    ui.SdfShape {
      shape = "box", operation = "subtract", radius = ROUND,
      x = function() return stage:get() == "closed" and 0 or BORDER end,
      y = function() return stage:get() == "closed" and 0 or BORDER end,
      width = function() return stage:get() == "closed" and W or W - 2 * BORDER end,
      height = function() return stage:get() == "closed" and H or H - 2 * BORDER end,
      behavior = { x = SETTLE, y = SETTLE, width = SETTLE, height = SETTLE },
    },
    ui.SdfShape {
      id = "lock-swell",
      shape = "box", operation = "smooth_union", blend = s(26),
      radius = function() return up() > 0.5 and s(38) or s(12) end,
      fill_color = function() return up() > 0.5 and C.surfaceContainer or C.surface end,
      x = function() return math.floor((W - swell_w()) / 2) end,
      y = function() return H - swell_h() end,
      width = swell_w,
      height = function() return swell_h() + s(40) end,
      behavior = { x = GROW, y = GROW, width = GROW, height = GROW, radius = SETTLE,
        fill_color = { duration = 300 } },
    },
  }

  -- The desk under it: the wallpaper, blurred and dimmed.
  local backdrop = ui.Item {
    anchors = { fill = true },
    opacity = function() return (stage:get() == "rest" or stage:get() == "sheet") and 1 or 0 end,
    behavior = { opacity = { duration = 420, easing = "out_cubic" } },
    ui.Rect { anchors = { fill = true }, color = function() return C.surface end },
    WALLPAPER ~= "" and ui.Image {
      anchors = { fill = true }, fill_mode = "preserve_aspect_crop", source = WALLPAPER,
    } or ui.Item {},
    ui.Rect {
      anchors = { fill = true }, backdrop_blur = s(28),
      color = function() return stage:get() == "sheet" and C.surface:alpha(0.55) or C.surface:alpha(0.3) end,
      behavior = { color = { duration = 420 } },
    },
  }

  -- ------------------------------------------------------------ at rest --

  local function with(base, extra)
    for k, v in pairs(extra) do base[k] = v end
    return base
  end
  local function resting() return stage:get() == "rest" or stage:get() == "sheet" end

  -- The weather, beside the date, where it can be had.
  local weather = {}
  do
    local ok, lib = pcall(require, "lib.weather")
    if ok then
      weather_source = weather_source or lib.new { units = "metric" }
      local w = weather_source
      local function now_w() return w:get() or {} end
      weather = {
        icon(function() local n = now_w() return lib.material_symbol(n.code, n.is_day) end, s(26),
          function() return C.onSurfaceVariant end,
          { visible = function() return now_w().temperature ~= nil end }),
        text {
          font_size = s(22), color = function() return C.onSurfaceVariant end,
          visible = function() return now_w().temperature ~= nil end,
          text = function()
            local n = now_w()
            return n.temperature and ("%d°"):format(math.floor(n.temperature + 0.5)) or ""
          end,
        },
      }
    end
  end

  -- What is playing, as a row: art in a turning cookie, the title, controls.
  local media_row
  do
    local ok, media = pcall(function() media_source = media_source or require("lib.mpris").connect() return media_source end)
    if ok and media then
      local function active() return media.state.active or {} end
      local function control(action) pcall(media[action]) end
      local function button(id, name, action, strong)
        local area
        area = ui.MouseArea {
          id = id, width = s(44), height = s(44), cursor = "pointer",
          on_clicked = function() control(action) end,
          scale = function() return (area and area.pressed) and 0.9 or 1 end,
          behavior = { scale = ui.spring { stiffness = 700, damping = 18 } },
          ui.Rect {
            anchors = { fill = true }, radius = s(22),
            color = function() return strong and C.primary or C.surfaceContainerHighest end,
          },
          icon(name, s(22), strong and C.onPrimary or C.onSurface, { anchors = { center_in = true } }),
        }
        return area
      end
      local art = function() return require("lib.remote").file(active().art_url or "") end
      local RW, RH = math.min(s(560), W - 2 * s(40)), s(84)
      local cookie12 = shapes.path("cookie12", { segments = false })
      media_row = ui.Rect {
        id = "lock-media", width = RW, height = RH, radius = RH / 2,
        color = function() return C.surfaceContainer:alpha(0.82) end,
        visible = function() return main() and (active().title or "") ~= "" end,
        ui.Item {
          x = s(12), anchors = { vertical_center = true }, width = s(60), height = s(60),
          ui.Path {
            anchors = { fill = true }, view_box = { 0, 0, 100, 100 }, d = cookie12,
            fill_color = function() return C.secondaryContainer end,
          },
          ui.Image {
            anchors = { fill = true }, fill_mode = "preserve_aspect_crop", source = art,
            visible = function() return art() ~= "" end,
            mask = ui.Path { anchors = { fill = true }, view_box = { 0, 0, 100, 100 }, d = cookie12, fill_color = "#ffffff" },
          },
          loop = function()
            if not active().playing then return nil end
            return { rotation = { to = 360, duration = 30000, hold = true } }
          end,
        },
        ui.Column {
          x = s(86), anchors = { vertical_center = true }, gap = s(2),
          text { width = RW - s(86) - s(170), elide = "right", font_size = s(16), font_weight = 600,
            text = function() return active().title or "" end },
          text { width = RW - s(86) - s(170), elide = "right", font_size = s(14),
            color = function() return C.onSurfaceVariant end,
            text = function() return active().artist or "" end },
        },
        ui.Row {
          anchors = { right = true, right_margin = s(14), vertical_center = true }, gap = s(8),
          button("lock-media-previous", "skip_previous", "previous"),
          button("lock-media-play", function() return active().playing and "pause" or "play_arrow" end, "play_pause", true),
          button("lock-media-next", "skip_next", "next"),
        },
      }
    end
  end

  -- The time at rest sits a third of the way down; with the sheet up it
  -- steps aside above it, smaller.
  local CLOCK_Y = PORTRAIT and math.floor(H * 0.12) or math.floor(H * 0.2)
  local glance = ui.Column {
    id = "lock-glance",
    anchors = { horizontal_center = true }, gap = s(6), align = "center",
    y = function()
      if stage:get() == "sheet" and main() then
        return math.max(s(40), math.floor((H - BORDER - sheet_h()) / 2 - s(150)))
      end
      return CLOCK_Y
    end,
    scale = function() return (stage:get() == "sheet" and main()) and 0.72 or 1 end,
    opacity = function() return resting() and 1 or 0 end,
    behavior = { y = GROW, scale = GROW, opacity = { duration = 320 } },
    text {
      id = "lock-clock", text = function() return clock:get() end,
      font_size = PORTRAIT and s(132) or s(168), font_weight = 600, color = C.primary,
    },
    ui.Row((function()
      local row = { gap = s(10), align = "center",
        text { text = function() return day:get() end, font_size = s(22), color = C.onSurfaceVariant } }
      for _, node in ipairs(weather) do row[#row + 1] = node end
      return row
    end)()),
    ui.Item { width = 1, height = s(34) },
    media_row or ui.Item { width = 1, height = 1 },
  }

  -- Where the way in is: a chevron bobbing over the bud, and what to do.
  local hint = ui.Column {
    anchors = { horizontal_center = true },
    y = H - BORDER - BUD_H - s(74), gap = s(2), align = "center",
    opacity = function() return (main() and stage:get() == "rest" and pull:get() < 0.1) and 1 or 0 end,
    behavior = { opacity = { duration = 260 } },
    icon("keyboard_arrow_up", s(30), function() return C.onSurfaceVariant end, {
      loop = { translate_y = { from = 0, to = -s(6), duration = 900, alternate = true, easing = "in_out_sine" } },
    }),
    text {
      text = ONSCREEN and "Swipe up to unlock" or "Type or click to unlock",
      font_size = s(14), color = function() return C.onSurfaceVariant end,
    },
  }

  -- -------------------------------------------------------------- the sheet --

  local cookie = shapes.path("cookie9", { segments = false })
  local avatar = ui.Item {
    width = AV, height = AV,
    -- Only the cookie turns: a face or an initial stays upright on it.
    ui.Path {
      anchors = { fill = true }, view_box = { 0, 0, 100, 100 }, d = cookie,
      fill_color = function() return C.primaryContainer end,
      loop = function()
        return { rotation = { to = 360, duration = busy:get() and 2400 or 60000, hold = true } }
      end,
    },
    me.face and ui.Image {
      anchors = { fill = true }, fill_mode = "preserve_aspect_crop", source = me.face,
      mask = ui.Path { anchors = { fill = true }, view_box = { 0, 0, 100, 100 }, d = cookie, fill_color = "#ffffff" },
    } or text {
      anchors = { center_in = true }, text = me.initial or "?", font_size = s(42), font_weight = 600,
      color = C.onPrimaryContainer,
    },
  }

  -- The password: a pill, a dot for each character, each popping in.
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
    id = "lock-field",
    width = FIELD_W, height = FIELD_H,
    translate_x = function() return shake:get() == 1 and s(12) or 0 end,
    behavior = { translate_x = ui.spring { stiffness = 900, damping = 9 } },
    ui.Rect {
      anchors = { fill = true }, radius = FIELD_H / 2,
      color = function() return C.surfaceContainerHighest end,
      border_width = function() return bad:get() and s(2) or 0 end,
      border_color = function() return C.error end,
    },
    icon(function() return busy:get() and "hourglass" or "lock" end, s(22), C.onSurfaceVariant,
      { x = s(20), anchors = { vertical_center = true } }),
    text {
      anchors = { vertical_center = true }, x = s(56),
      text = "Password", color = C.onSurfaceVariant, font_size = s(16),
      visible = function() return typed:get() == 0 end,
    },
    ui.Row { x = s(56), anchors = { vertical_center = true }, gap = s(7), table.unpack(dots) },
    ui.MouseArea {
      id = "lock-submit",
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

  local sheet_nodes = {
    x = s(12), y = s(28), width = SW - s(24), gap = 0, align = "center",
    avatar,
    ui.Item { width = 1, height = s(12) },
    text { text = me.label ~= "" and me.label or me.name, font_size = s(20), font_weight = 600, height = s(30) },
    ui.Item { width = 1, height = s(20) },
    ui.Item {
      width = SW - s(24), height = entry_h,
      ui.Item { anchors = { horizontal_center = true }, width = FIELD_W, height = FIELD_H,
        visible = function() return method:get() == "password" end, field },
      ui.Item { anchors = { horizontal_center = true }, width = PAD_W, height = pad.height,
        visible = function() return method:get() == "pattern" end, pad.node },
    },
    ui.Item { width = 1, height = s(10) },
    text {
      id = "lock-message", height = s(24), width = SW - s(48), horizontal_alignment = "center", elide = "right",
      text = function() return message:get() end, font_size = s(15),
      color = function() return bad:get() and C.error or C.onSurfaceVariant end,
    },
    ui.Item {
      width = SW - s(24), height = chip_h, visible = has_pattern,
      (function()
        local area
        area = ui.MouseArea {
          id = "lock-method", anchors = { horizontal_center = true, bottom = true },
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
    id = "lock-sheet",
    x = math.floor((W - SW) / 2), width = SW,
    y = function() return H - BORDER - sheet_h() end,
    height = sheet_h,
    opacity = function() return stage:get() == "sheet" and 1 or 0 end,
    translate_y = function() return stage:get() == "sheet" and 0 or s(60) end,
    behavior = { opacity = { duration = 260, delay = 120 },
      translate_y = GROW },
    visible = function() return main() and (stage:get() == "sheet" or stage:get() == "opening") end,
    ui.Column(sheet_nodes),
  }

  -- ------------------------------------------------------------ the screen --

  -- Opaque from the first frame: a lock that let the desk show through for
  -- a moment would not be a lock, and morf will not hold one that could.
  return ui.Rect {
    anchors = { fill = true },
    color = C.surface:alpha(1),
    backdrop,
    frame,
    glance,
    hint,
    sheet,
    -- Under everything that can be clicked: a click or a swipe up opens the
    -- sheet, and the keys go where they belong.
    ui.MouseArea {
      id = "lock-open",
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
        local st = stage:get()
        if st ~= "rest" and st ~= "sheet" then return end
        if busy:get() then return end
        if keysym == ESCAPE then escape() return end
        -- Any other key at rest opens the way in, and a character is the
        -- password's first.
        if st == "rest" then open_sheet() end
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

end

if HELD and morf.lock_surface then
  -- Called for each output, and again for one plugged in while locked.
  morf.lock_surface(function(output)
    return build(tonumber(output.width) or W, tonumber(output.height) or H, tostring(output.name or ""))
  end)
else
  build(W, H, screen and screen.name or "")
end

-- The reader (a finger, a face) is listened to while the way in is open.
morf.effect("lock.listen", function()
  if stage:get() == "sheet" then door:listen() else door:stop() end
end)

-- In a window, `morf ipc call stage sheet` puts it where a test wants it;
-- a held lock answers no such thing.
if not HELD then
  morf.ipc.stage = function(to)
    if to == "sheet" then stage:set("rest") open_sheet() elseif to == "rest" then stage:set("rest") end
    return stage:get()
  end
end

-- In: the frame closes and the bud comes up out of it.
morf.timer(30, function() stage:set("rest") end, false)
