-- Shared greeter composition. Skins supply surfaces and motion only; no
-- password or greetd connection is stored here.
local morf = require("morf")
local ui = require("morf.ui")
local osk = require("lib.osk")
local shapes = require("lib.m3shapes")
return function(ctx)
local ui = ctx.ui or ui
local skin = ctx.auth_skin or {}
local message, bad = ctx.message, ctx.bad
local W = ctx.W
local H = ctx.H
local s = ctx.s
local C = ctx.C
local FONT = ctx.FONT
local ICONS = ctx.ICONS
local text = ctx.text
local icon = ctx.icon
local stage = ctx.stage
local pull = ctx.pull
local busy = ctx.busy
local say = ctx.say
local submit = ctx.submit
local method = ctx.method
local has_pattern = ctx.has_pattern
local type_text = ctx.type_text
local backspace = ctx.backspace
local escape = ctx.escape
local clock = ctx.clock
local day = ctx.day
local people = ctx.people
local person = ctx.person
local session = ctx.session
local list = ctx.list
local who = ctx.who
local which = ctx.which
local typed = ctx.typed
local shake = ctx.shake
local MAX_DOTS = ctx.MAX_DOTS
local open_sheet = ctx.open_sheet
local power = ctx.power
local step_person = ctx.step_person
local step_session = ctx.step_session
local choose = ctx.choose
local clear = ctx.clear
local pattern = ctx.pattern
local key = ctx.key
-- -------------------------------------------------------------- geometry --

local BORDER = s(10)
local ROUND = s(25)
local PORTRAIT = H > W
-- The on-screen keyboard: on a phone, or wherever no keyboard is attached.
local ONSCREEN = PORTRAIT or not ctx.keyboard_attached()

local SW = math.min(s(600), W - 2 * s(20))
local AV = s(96)
local FIELD_W, FIELD_H = math.min(s(380), SW - s(48)), s(58)

-- The pattern (tools/pattern): offered where the stack takes one and one
-- is set for this account. Anywhere else a drawn pattern would only be a
-- wrong password, and a failed login counted.
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
    font = FONT, icons = ICONS, radius = skin.key_radius,
  }
end
local PAD_W = math.min(s(300), SW - s(48))
local pad = osk.new {
  prefix = "greet.pattern", width = PAD_W, mode = "pattern", look = skin.keyboard_look and skin.keyboard_look(look()) or look(),
  active = function() return stage:get() == "sheet" and method:get() == "pattern" and not busy:get() end,
  on_pattern = function(dots)
    pattern(dots)
  end,
}
local function entry_h() return method:get() == "pattern" and pad.height() or FIELD_H end
local function chip_h() return has_pattern() and s(44) or 0 end
local kb
if ONSCREEN then
  kb = osk.new {
    active = function() return stage:get() == "sheet" and method:get() == "password" and not busy:get() end,
    action = skin.action,
      prefix = "greet.osk", width = SW - s(24), mode = "full", numbers = true,
    look = (skin.keyboard_look or function(v) return v end) {
      panel = function() return C.surfaceContainer end,
      key = function() return C.surfaceContainerHighest end,
      key_dim = function() return C.surfaceContainerHigh end,
      accent = function() return C.primary end,
      on_accent = function() return C.onPrimary end,
      text = function() return C.onSurface end,
      dim = function() return C.onSurfaceVariant end,
      press = function() return C.secondaryContainer end,
      font = FONT, icons = ICONS, radius = skin.key_radius,
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
local GROW = skin.grow or { duration = 420, easing = "out_back" }
local SETTLE = skin.settle or { duration = 340, easing = "out_cubic" }
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
      shape = "box", operation = "smooth_union", blend = skin.blend or s(26),
      radius = function() return up() > 0.5 and s(38) or s(12) end,
      fill_color = function() return up() > 0.5 and (skin.sheet_color and skin.sheet_color() or C.surfaceContainer) or C.surface end,
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
local backdrop
if skin.backdrop then
  backdrop = skin.backdrop(W, H, s)
else
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
  backdrop = ui.Item {
    anchors = { fill = true },
    ui.Rect { anchors = { fill = true }, color = function() return C.surfaceContainerLowest end },
    ui.Item(drift),
  }

end

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
      id = id .. "-surface", anchors = { fill = true }, radius = size / 2,
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
  if skin.avatar then return skin.avatar(p, size, function() return true end, "greet-face-" .. p.name) end
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
      (function()
        if skin.avatar then return skin.avatar(p, PEOPLE_AV, chosen, "greet-avatar-" .. index) end
        return ui.Item { anchors = { fill = true },
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
        }
      end)(),
    },
    (ctx.heading or text) {
      id = "greet-person-name-" .. index, active=function() return stage:get()=="rest" end,
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
  (skin.clock or text) { id = "greet-clock", text = function() return clock:get() end,
    font_size = PORTRAIT and s(132) or s(160), font_weight = skin.clock_weight or 600, color = C.primary },
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
local host = ctx.hostname
local host_label = ui.Row {
  x = BORDER + s(28), y = BORDER + s(30), gap = s(10), align = "center",
  opacity = function() return showing() and 1 or 0 end,
  behavior = { opacity = { duration = 320, delay = 200 } },
  icon("computer", s(22), C.onSurfaceVariant),
  (ctx.heading or text) { id="greet-host", active=showing, font_size = s(18), font_weight = 600, color = C.onSurfaceVariant, text = host },
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
    id = "greet-field-surface", anchors = { fill = true }, radius = FIELD_H / 2,
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
      id = "greet-submit-surface", anchors = { fill = true }, radius = (FIELD_H - s(14)) / 2,
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
  ui.Rect { id = "greet-session-surface", anchors = { fill = true }, radius = s(20), color = function() return C.secondaryContainer end },
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
  (ctx.heading or text) { id = "greet-name", active=function() return stage:get()=="sheet" end, font_size = s(20), font_weight = 600, height = s(30),
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
          id = "greet-method-surface", anchors = { fill = true }, radius = s(18),
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
  behavior = skin.sheet_motion or { opacity = { duration = 260, delay = 120 }, translate_y = GROW },
  visible = function() return stage:get() == "sheet" or stage:get() == "leaving" end,
  ui.Column(sheet_nodes),
}

if skin.sheet then skin.sheet(sheet, {role="greet", width=SW, scale=s,
  active=function() return stage:get()=="sheet" end}) end

-- ------------------------------------------------------------ the screen --

ui.Item {
  anchors = { fill = true },
  backdrop,
  skin.chrome and skin.chrome(W, H, s, "greet") or ui.Item {},
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
    on_key_pressed = key,
  },
}


end
