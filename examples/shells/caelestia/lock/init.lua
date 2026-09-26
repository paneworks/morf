-- caelestia's lock screen: the frame round the screen closes in, and out of
-- its bottom edge a panel swells up -- one liquid surface, frame and panel
-- fused -- carrying the time, the one who is logged in and a place for the
-- password, with the weather beside it and whatever is playing. The right
-- password (or a finger, when the machine has a reader) and the panel sinks
-- back into the edge, the frame opens and the desk is there again.
--
--   morf -c caelestia/lock               the lock, held under ext-session-lock
--   morf -c caelestia/lock -- window     the same, in a window, holding nothing
--
-- Its own file: nothing here reaches into the shell's folder. What the lock
-- and the greeter share is in the library -- lib.auth (PAM and greetd),
-- lib.accounts, lib.lule and lib.material for the colours.

local morf = require("morf")
local ui = require("morf.ui")
local lule = require("lib.lule")
local material = require("lib.material")
local shapes = require("lib.m3shapes")
local accounts = require("lib.accounts")
local auth = require("lib.auth")

local HELD = morf.operands[1] ~= "window"

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
-- in: the panel out and the content on; "opening": all of it going away.
local phase = morf.signal("lock.phase", "closed")

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
  phase:set("opening")
  say("")
  morf.timer(520, function()
    if HELD then morf.surface.session_lock = false else morf.quit() end
  end, false)
end
door = auth.lock {
  user = me.name,
  on_busy = function(b) busy:set(b) end,
  on_info = function(words, wrong) if not busy:get() then say(words, wrong) end end,
  on_failed = function(why)
    say(why ~= "" and why or "Wrong password", true)
    clear()
    shake:set(shake:get() + 1)
  end,
  on_open = lift,
}

local function submit()
  if busy:get() or phase:get() == "opening" then return end
  if password == "" then
    say("Type your password", false)
    return
  end
  say("")
  door:submit(password)
end

-- --------------------------------------------------------------- the time --

local function now(format) return morf.time.format(format, morf.time.now()) end
local clock = morf.signal("lock.clock", now("%H:%M"))
local day = morf.signal("lock.day", now("%A, %-d %B"))
morf.timer(1000, function()
  clock:set(now("%H:%M"))
  day:set(now("%A, %-d %B"))
end, true)

-- ------------------------------------------------------------- the frame --

-- The frame and the panel are one distance field: the panel is a box that
-- rises out of the frame's bottom edge and melts into it where they meet.
local BORDER = s(10)
local ROUND = s(25)
local PW, PH = s(1180), s(600)
local PX, PY = math.floor((W - PW) / 2), math.floor((H - PH) / 2)
local GROW = { duration = 620, easing = "out_back" }

local function panel_open() return phase:get() == "in" end
local frame = ui.Sdf {
  anchors = { fill = true },
  ui.SdfShape {
    shape = "box", x = 0, y = 0, width = W, height = H,
    fill_color = function() return C.surface end,
  },
  -- The opening in the frame: the desk shows through until the lock is in.
  ui.SdfShape {
    shape = "box", operation = "subtract", radius = ROUND,
    x = function() return phase:get() == "closed" and 0 or BORDER end,
    y = function() return phase:get() == "closed" and 0 or BORDER end,
    width = function() return phase:get() == "closed" and W or W - 2 * BORDER end,
    height = function() return phase:get() == "closed" and H or H - 2 * BORDER end,
    behavior = { x = GROW, y = GROW, width = GROW, height = GROW },
  },
  -- The panel, out of the bottom edge.
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

-- The desk under it: the wallpaper, blurred and dimmed, fading in as the
-- lock closes and out as it opens.
local backdrop = ui.Item {
  anchors = { fill = true },
  opacity = function() return phase:get() == "in" and 1 or 0 end,
  behavior = { opacity = { duration = 420, easing = "out_cubic" } },
  ui.Rect { anchors = { fill = true }, color = function() return C.surface end },
  WALLPAPER ~= "" and ui.Image {
    anchors = { fill = true }, fill_mode = "preserve_aspect_crop", source = WALLPAPER,
  } or ui.Item {},
  ui.Rect {
    anchors = { fill = true }, backdrop_blur = s(28),
    color = function() return C.surface:alpha(0.35) end,
  },
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

-- The account: its face cut into a cookie, or its initial on one.
local AV = s(124)
local cookie = shapes.path("cookie9", { segments = false })
local avatar = ui.Item {
  width = AV, height = AV,
  -- Only the cookie turns: a face or an initial stays upright on it.
  ui.Path {
    anchors = { fill = true }, view_box = { 0, 0, 100, 100 }, d = cookie,
    fill_color = function() return C.primaryContainer end,
    loop = { rotation = { to = 360, duration = 80000 } },
  },
  me.face and ui.Image {
    anchors = { fill = true }, fill_mode = "preserve_aspect_crop", source = me.face,
    mask = ui.Path { anchors = { fill = true }, view_box = { 0, 0, 100, 100 }, d = cookie, fill_color = "#ffffff" },
  } or text {
    anchors = { center_in = true }, text = me.initial or "?", font_size = s(52), font_weight = 600,
    color = C.onPrimaryContainer,
  },
}

-- The password: a pill, a dot for each character, each popping in.
local FIELD_W, FIELD_H = s(360), s(58)
local DOT = s(12)
local MAX_DOTS = 18
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
  translate_x = function() return (shake:get() % 2 == 1) and s(10) or 0 end,
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
  ui.Row {
    x = s(56), anchors = { vertical_center = true }, gap = s(7),
    table.unpack(dots),
  },
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

-- The weather, where it can be had.
local weather_card
do
  local ok, lib = pcall(require, "lib.weather")
  if ok then
    local w = lib.new { units = "metric" }
    local function now() return w:get() or {} end
    weather_card = ui.Rect(with({
      width = s(300), height = s(180), radius = s(26),
      color = function() return C.surfaceContainerHigh end,
      icon(function() local n = now() return lib.material_symbol(n.code, n.is_day) end, s(56), C.primary,
        { x = s(24), y = s(26) }),
      text {
        x = s(96), y = s(28), font_size = s(40), font_weight = 600,
        text = function()
          local n = now()
          return n.temperature and ("%d°"):format(math.floor(n.temperature + 0.5)) or "--"
        end,
      },
      text {
        x = s(24), y = s(104), width = s(252), elide = "right", font_size = s(17), font_weight = 500,
        text = function() return now().condition or "" end,
      },
      text {
        x = s(24), y = s(134), width = s(252), elide = "right", font_size = s(14),
        color = C.onSurfaceVariant,
        text = function() return now().place or "" end,
      },
    }, shown(320)))
  end
end

-- What is playing, and its controls: the lock does not stop the music.
local media_card
do
  local ok, media = pcall(function() return require("lib.mpris").connect() end)
  if ok and media then
    local function active() return media.state.active or {} end
    local function control(action) pcall(media[action]) end
    local function button(id, name, action, strong)
      return ui.MouseArea {
        id = id, width = s(44), height = s(44), cursor = "pointer",
        on_clicked = function() control(action) end,
        ui.Rect {
          anchors = { fill = true }, radius = s(22),
          color = function() return strong and C.primary or C.surfaceContainerHighest end,
        },
        icon(type(name) == "function" and name or name, s(22), strong and C.onPrimary or C.onSurface,
          { anchors = { center_in = true } }),
      }
    end
    local art = function()
      local url = active().art_url or ""
      return require("lib.remote").file(url)
    end
    media_card = ui.Rect(with({
      width = s(300), height = s(300), radius = s(26),
      color = function() return C.surfaceContainerHigh end,
      visible = function() return (active().title or "") ~= "" end,
      ui.Item {
        x = s(90), y = s(22), width = s(120), height = s(120),
        ui.Path {
          anchors = { fill = true }, view_box = { 0, 0, 100, 100 }, d = shapes.path("cookie12", { segments = false }),
          fill_color = function() return C.secondaryContainer end,
        },
        ui.Image {
          anchors = { fill = true }, fill_mode = "preserve_aspect_crop", source = art,
          visible = function() return art() ~= "" end,
          mask = ui.Path {
            anchors = { fill = true }, view_box = { 0, 0, 100, 100 },
            d = shapes.path("cookie12", { segments = false }), fill_color = "#ffffff",
          },
        },
        loop = function()
          if not active().playing then return nil end
          return { rotation = { to = 360, duration = 30000, hold = true } }
        end,
      },
      text {
        x = s(20), y = s(156), width = s(260), horizontal_alignment = "center", elide = "right",
        font_size = s(17), font_weight = 600, text = function() return active().title or "" end,
      },
      text {
        x = s(20), y = s(182), width = s(260), horizontal_alignment = "center", elide = "right",
        font_size = s(14), color = C.onSurfaceVariant, text = function() return active().artist or "" end,
      },
      ui.Row {
        anchors = { horizontal_center = true }, y = s(226), gap = s(10),
        button("lock-media-previous", "skip_previous", "previous"),
        button("lock-media-play", function() return active().playing and "pause" or "play_arrow" end, "play_pause", true),
        button("lock-media-next", "skip_next", "next"),
      },
    }, shown(380)))
  end
end

local centre = ui.Column(with({
  anchors = { horizontal_center = true }, y = PY + s(56), gap = s(4), align = "center",
  text {
    id = "lock-clock", text = function() return clock:get() end,
    font_size = s(112), font_weight = 600, color = C.primary,
  },
  text { text = function() return day:get() end, font_size = s(22), color = C.onSurfaceVariant },
  ui.Item { width = 1, height = s(26) },
  avatar,
  ui.Item { width = 1, height = s(10) },
  text { text = me.label or me.name, font_size = s(22), font_weight = 600 },
  ui.Item { width = 1, height = s(18) },
  field,
  ui.Item { width = 1, height = s(10) },
  text {
    id = "lock-message", height = s(22),
    text = function() return message:get() end, font_size = s(15),
    color = function() return bad:get() and C.error or C.onSurfaceVariant end,
  },
}, shown(240)))

-- A placeholder only where the card could not be made: one built up front
-- and replaced is left with no parent, and a lock surface holds exactly one
-- root -- the lock would not start.
weather_card = weather_card or ui.Item {}
media_card = media_card or ui.Item {}

local sides = ui.Item {
  anchors = { fill = true },
  ui.Item(with({ x = PX + s(36), y = PY + s(56), width = s(300), height = s(400), weather_card }, {})),
  ui.Item(with({ x = PX + PW - s(336), y = PY + s(56), width = s(300), height = s(400), media_card }, {})),
}

-- ------------------------------------------------------------ the screen --

-- Opaque from the first frame: a lock that let the desk show through for
-- a moment would not be a lock, and morf will not hold one that could.
ui.Rect {
  anchors = { fill = true },
  color = C.surface:alpha(1),
  backdrop,
  frame,
  sides,
  centre,
  -- Under everything that can be clicked: the keys, for the whole screen.
  ui.MouseArea {
    anchors = { fill = true }, z = -1,
    on_key_pressed = function(keysym, typed_text)
      local RETURN, KP_ENTER, BACKSPACE, ESCAPE = 0xff0d, 0xff8d, 0xff08, 0xff1b
      if phase:get() ~= "in" or busy:get() then return end
      if keysym == RETURN or keysym == KP_ENTER then
        submit()
      elseif keysym == BACKSPACE then
        password = password:sub(1, -2)
        typed:set(math.min(#password, MAX_DOTS))
      elseif keysym == ESCAPE then
        -- Looked at in a window, it holds nothing: Escape on an empty
        -- field is the way out, in case the door will not open.
        if not HELD and #password == 0 then morf.quit() return end
        clear()
        say("")
      elseif typed_text and typed_text ~= "" and typed_text:byte(1) >= 32 then
        password = password .. typed_text
        typed:set(math.min(#password, MAX_DOTS))
        if bad:get() then say("") end
      end
    end,
  },
}

-- In: the frame closes, the panel rises out of it.
morf.timer(30, function() phase:set("in") end, false)
