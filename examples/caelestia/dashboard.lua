-- The dashboard: a drawer at the top of the frame with four tabs --
-- Dashboard, Media, Performance, Weather. The Dashboard tab is here: the
-- weather, who is logged in and for how long, the time stacked on its side,
-- a month calendar, three resource rings and the media card; the others
-- are dashboard_media.lua, dashboard_performance.lua and
-- dashboard_weather.lua. Each tab has its own size: switching tabs slides
-- the pages sideways while the drawer eases to the new tab's size.
--
-- It opens over IPC, and when the pointer reaches the top edge of the frame
-- above it; one opened that way closes again when the pointer leaves it.
--
-- Geometry measured off the reference at 1920x1080 (panel coordinates):
-- 872 x 538 on the Dashboard tab (Media 1032 x 418, Performance 987 x 484,
-- Weather 870 x 660), 16 px padding, tabs 64 tall with a 3 px indicator and a hairline
-- under them, cards from y = 84 in two rows (132 and 295 tall, 12 apart).

local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local config = require("config")
local drawer = require("drawer")
local services = require("services")
local shapes = require("lib.m3shapes")

local C = theme.color
local M = {}

local PAD, GAP = 16, 12
-- Each tab's page (the panel less its padding and the tabs).
local PAGE = {
  { 840, 439 },
  { require("dashboard_media").WIDTH, require("dashboard_media").HEIGHT },
  { require("dashboard_performance").WIDTH, require("dashboard_performance").HEIGHT },
  { require("dashboard_weather").WIDTH, require("dashboard_weather").HEIGHT },
}
local TABS_H = 68            -- icons, labels, indicator and hairline
local ROW1, ROW2 = 132, 295

local state = require("dashboard_state")
M.tab = state.tab

--- The panel's size on tab `i`.
function M.size(i)
  local p = PAGE[i] or PAGE[1]
  return p[1] + 2 * PAD, TABS_H + PAD + p[2] + PAD - 1
end
local function width() return (M.size(M.tab:get())) end
local function height() local _, h = M.size(M.tab:get()) return h end

-- The drawer's and the pages' pace between tabs, fitted to films of the
-- reference: most of the way in the first hundred milliseconds, settled in
-- about a third of a second.
local SWITCH = { duration = theme.duration.normal, easing = theme.ease.emphasized_decel }
local opened = state.opened

local area = state.area

-- ------------------------------------------------------------------- tabs --

local TABS = {
  { name = "Dashboard", icon = "dashboard" },
  { name = "Media", icon = "queue_music" },
  { name = "Performance", icon = "speed" },
  { name = "Weather", icon = "cloud" },
}

local function tabs()
  -- The tabs share the panel's width as it eases between tabs' sizes: a
  -- row that grows each of them, so they follow the drawer as it moves.
  local function slot() return (width() - 2 * PAD) / #TABS end
  local labels = {}
  local buttons = {}
  for i, t in ipairs(TABS) do
    local on = function() return M.tab:get() == i end
    local label = kit.text {
      text = t.name, font_size = theme.size.normal + 1,
      color = function() return on() and C.primary or C.onSurface end,
      behavior = { color = { duration = theme.duration.small } },
    }
    labels[i] = label
    local button
    button = ui.MouseArea {
      id = "dashboard-tab-" .. t.name:lower(),
      width = 10, height = TABS_H - 4, cursor = "pointer",
      layout = { grow = 1 },
      on_clicked = function() M.tab:set(i) end,
      ui.Column {
        anchors = { horizontal_center = true }, y = 8, gap = 4, align = "center",
        kit.icon(t.icon, 22, function() return on() and C.primary or C.onSurface end, { fill = on }),
        label,
      },
    }
    -- The hovered tab lifts on a rounded wash, as the reference's.
    local wash = ui.Rect {
      anchors = { fill = true, top_margin = 6, bottom_margin = 1 }, radius = 10, z = -1,
      color = function() return button.hovered and C.onSurface:alpha(0.06) or C.onSurface:alpha(0) end,
      behavior = { color = { duration = theme.duration.small } },
    }
    ui.reparent(wash, button)
    buttons[#buttons + 1] = button
  end
  -- The indicator: as wide as the chosen label, under it. It is a layer
  -- of a small distance field with the hairline beneath it, which it melts
  -- into at its foot; moving to another tab it stretches out ahead, the
  -- edge in front first and the one behind after (kit.elastic), and draws
  -- itself in at the new tab.
  local function span(i, panel_width)
    local l = labels[i]
    local w = (l and l.layout_width or 80) + 4
    local s = (panel_width - 2 * PAD) / #TABS
    local x = (i - 1) * s + (s - w) / 2
    return x, x + w
  end
  local indicator = ui.Item {
    id = "dashboard-tab-indicator",
    y = TABS_H - 4, height = 3,
    x = 0, width = 0,
  }
  local shown_tab, moved = 1, false
  morf.effect("caelestia.dashboard.indicator", function()
    local tab = M.tab:get()
    -- Until the first switch it follows the label's width as it is laid
    -- out; after, each switch runs from tab to tab.
    local l1, r1 = span(tab, width())
    if tab == shown_tab then
      if not moved then indicator.x, indicator.width = l1, r1 - l1 end
      return
    end
    local from = shown_tab
    shown_tab, moved = tab, true
    local l0, r0 = span(from, M.size(from))
    kit.elastic(indicator, "x", l0, r0, l1, r1, { duration = 520 })
  end)
  local field = ui.Sdf {
    id = "dashboard-tab-field",
    anchors = { left = true, right = true }, y = TABS_H - 8, height = 8,
    blend = 3,
    ui.SdfShape {
      shape = "box", operation = "union",
      anchors = { left = true, right = true }, y = 7, height = 1,
      fill_color = function() return C.outlineVariant end,
    },
    ui.SdfShape {
      id = "dashboard-tab-indicator-shape",
      shape = "box", operation = "smooth_union",
      top_left_radius = 1.5, top_right_radius = 1.5,
      track = indicator,
      fill_color = function() return C.primary end,
    },
  }
  return ui.Item {
    anchors = { left = true, right = true, left_margin = PAD, right_margin = PAD },
    height = TABS_H,
    ui.Flex {
      anchors = { fill = true, bottom_margin = 4 }, direction = "row", padding = 0,
      table.unpack(buttons),
    },
    field,
    indicator,
  }
end

-- ---------------------------------------------------------------- weather --

local function weather_card()
  local function now()
    if not opened:get() then return { available = false } end
    return services.weather()
  end
  return kit.card {
    id = "dashboard-weather",
    width = 275, height = ROW1,
    ui.Row {
      anchors = { center_in = true }, gap = 18, align = "center",
      kit.icon(function()
        local w = now()
        return w.available and services.weather_symbol(w.code, w.is_day) or "cloud"
      end, 60, function() return C.secondary end),
      ui.Column {
        gap = 2, align = "center",
        kit.text {
          id = "dashboard-temperature",
          font_size = theme.size.extra + 6, font_weight = 500,
          color = function() return C.primary end,
          text = function()
            local w = now()
            if not w.available then return "--" end
            return ("%d%s"):format(math.floor((w.temperature or 0) + 0.5), w.units and w.units.temperature or "°")
          end,
        },
        kit.text {
          font_size = theme.size.normal + 1,
          text = function()
            local w = now()
            return w.available and (w.condition or "") or "No weather"
          end,
        },
      },
    },
  }
end

-- ------------------------------------------------------------------- user --

local function uptime_text(seconds)
  seconds = math.floor(tonumber(seconds) or 0)
  local days, hours, minutes = seconds // 86400, (seconds % 86400) // 3600, (seconds % 3600) // 60
  local parts = {}
  local function add(n, unit) if n > 0 then parts[#parts + 1] = n .. " " .. unit .. (n == 1 and "" or "s") end end
  add(days, "day") add(hours, "hour") add(minutes, "minute")
  if #parts == 0 then return "up just now" end
  return "up " .. table.concat(parts, ", ")
end

local function wm_name()
  if services.hyprland.available() then return "Hyprland" end
  local desktop = morf.env and morf.env("XDG_CURRENT_DESKTOP")
  return (desktop and desktop ~= "") and desktop or "Wayland"
end

local function user_card()
  local sysinfo = require("lib.sysinfo")
  local face = morf.fs.home() .. "/.face"
  local has_face = morf.fs.exists(face)
  local AV = 100
  local avatar = ui.Item {
    x = 43, y = 16, width = AV, height = AV,
    ui.Rect {
      anchors = { fill = true }, radius = AV / 2,
      color = function() return C.surfaceContainerHighest end,
      kit.icon("person_add", 40, function() return C.onSurfaceVariant end, {
        anchors = { center_in = true }, visible = not has_face, fill = true,
      }),
    },
    has_face and ui.Image {
      anchors = { fill = true }, source = face, fill_mode = "preserve_aspect_crop",
      mask = ui.Rect { radius = AV / 2, color = "#ffffff" },
    } or nil,
  }
  -- The distribution's badge: its logo on a primary cookie, over the
  -- avatar's shoulder.
  local badge = ui.Item {
    x = 18, y = 14, width = 50, height = 50,
    ui.Path {
      anchors = { fill = true }, view_box = { 0, 0, 100, 100 },
      d = shapes.path("cookie9"),
      fill_color = function() return C.primaryContainer end,
    },
    ui.Text {
      anchors = { center_in = true }, text = "\u{f303}", font_family = theme.mono, font_size = 22,
      color = function() return C.onPrimaryContainer end,
    },
  }
  local chip_row = ui.Row {
    gap = 6, align = "center",
    kit.icon("select_window", 17, function() return C.onSecondaryContainer end),
    kit.text {
      text = function() return wm_name() end, font_size = theme.size.normal,
      color = function() return C.onSecondaryContainer end,
    },
  }
  local chip = ui.Rect {
    id = "dashboard-wm",
    x = 152, y = 20, height = 36, radius = 18,
    width = function() return (chip_row.layout_width or 0) + 28 end,
    color = function() return C.secondaryContainer end,
    ui.Item {
      x = 14, width = function() return chip_row.layout_width or 0 end, height = 36,
      ui.Item { anchors = { vertical_center = true }, width = function() return chip_row.layout_width or 0 end, height = 22, chip_row },
    },
  }
  -- A thought bubble's tail from the avatar up to the chip.
  local tail = ui.Item {
    ui.Rect { x = 149, y = 58, width = 10, height = 10, radius = 5, color = function() return C.secondaryContainer end },
    ui.Rect { x = 164, y = 51, width = 7, height = 7, radius = 3.5, color = function() return C.secondaryContainer end },
  }
  local uptime_badge = ui.Rect {
    x = 111, y = 86, width = 44, height = 30, radius = 12,
    color = function() return C.tertiary end,
    kit.icon("timer", 19, function() return C.onTertiary end, { anchors = { center_in = true } }),
  }
  local uptime = kit.text {
    id = "dashboard-uptime",
    x = 164, y = 90, width = 340 - 164 - 12, elide = "right",
    font_size = theme.size.normal,
    text = function()
      if not opened:get() then return "" end
      return uptime_text((sysinfo.system() or {}).uptime)
    end,
  }
  return kit.card {
    id = "dashboard-user",
    width = 340, height = ROW1,
    avatar, badge, tail, chip, uptime_badge, uptime,
  }
end

-- ------------------------------------------------------------------ clock --

local function clock_card()
  local function t(fmt) return function() morf.minute_clock:get() return morf.time.format(fmt) end end
  return kit.card {
    id = "dashboard-clock",
    width = 111, height = ROW2,
    ui.Column {
      anchors = { center_in = true }, gap = 0, align = "center",
      kit.text { text = t("%I"), font_size = 36, font_weight = 600, color = function() return C.secondary end },
      kit.text { text = "•••", font_size = 18, font_weight = 700, color = function() return C.primary end },
      kit.text { text = t("%M"), font_size = 36, font_weight = 600, color = function() return C.secondary end },
      kit.text { text = t("%p"), font_size = 22, font_weight = 600, color = function() return C.primary end },
    },
  }
end

-- --------------------------------------------------------------- calendar --

M.month_offset = morf.signal("caelestia.dashboard.month", 0)

--- The 42 days shown for the month `offset` months from now, Sunday first:
--- `{ day, current, today, weekend }` each, and the month's title.
function M.month(offset, today)
  today = today or morf.time.date()
  local year, month = today.year, today.month + (offset or 0)
  while month > 12 do month = month - 12 year = year + 1 end
  while month < 1 do month = month + 12 year = year - 1 end
  local weeks = morf.time.month(year, month, 7)
  local days = {}
  for _, week in ipairs(weeks) do
    for column, d in ipairs(week) do
      days[#days + 1] = {
        day = d.day,
        current = d.current,
        today = d.year == today.year and d.month == today.month and d.day == today.day,
        weekend = column == 1 or column == 7,
      }
    end
  end
  -- Six rows always, as the reference's grid: the next month runs on.
  local last = weeks[#weeks][7]
  local n = last.current and 0 or last.day
  while #days < 42 do
    n = n + 1
    local column = #days % 7 + 1
    days[#days + 1] = { day = n, current = false, today = false, weekend = column == 1 or column == 7 }
  end
  local title = morf.time.format("%B %Y", morf.time.time { year = year, month = month, day = 1, hour = 12 })
  return { title = title, days = days }
end

local function calendar_card()
  local CELL_W, CELL_H = 50, 31
  local function month()
    morf.minute_clock:get()
    return M.month(M.month_offset:get())
  end
  local cells = {}
  for i = 1, 42 do
    local function day() return month().days[i] end
    cells[#cells + 1] = ui.Item {
      width = CELL_W, height = CELL_H,
      ui.Path {
        anchors = { center_in = true }, width = 34, height = 34,
        view_box = { 0, 0, 100, 100 }, d = shapes.path("cookie9"),
        fill_color = function() return C.primary end,
        visible = function() return day().today end,
      },
      kit.text {
        anchors = { center_in = true },
        text = function() return tostring(day().day) end,
        font_size = theme.size.normal + 1,
        color = function()
          local d = day()
          if d.today then return C.onPrimary end
          if not d.current then return C.outline end
          if d.weekend then return C.tertiary end
          return C.onSurface
        end,
      },
    }
  end
  local names = {}
  for i, n in ipairs { "Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat" } do
    names[#names + 1] = kit.centred(CELL_W, 24, kit.text {
      text = n, font_size = theme.size.normal + 1, font_weight = 500,
      color = function() return (i == 1 or i == 7) and C.tertiary or C.onSurface end,
    })
  end
  local function arrow(icon, delta, id)
    return ui.MouseArea {
      id = id, width = 32, height = 32, cursor = "pointer",
      on_clicked = function() M.month_offset:set(M.month_offset:get() + delta) end,
      kit.icon(icon, 20, function() return C.onSurface end, { anchors = { center_in = true } }),
    }
  end
  return kit.card {
    id = "dashboard-calendar",
    width = 380, height = ROW2,
    ui.MouseArea {
      anchors = { fill = true }, z = -1,
      on_wheel = function(_, _, _, _, _, step_y)
        if step_y ~= 0 then M.month_offset:set(M.month_offset:get() + (step_y > 0 and 1 or -1)) end
      end,
    },
    ui.Item {
      x = 15, y = 20, width = 350, height = 32,
      arrow("chevron_left", -1, "calendar-previous"),
      kit.text {
        id = "calendar-title",
        anchors = { center_in = true },
        text = function() return month().title end,
        font_size = theme.size.larger + 1, font_weight = 500,
        color = function() return C.primary end,
      },
      ui.Item { anchors = { right = true }, width = 32, height = 32, arrow("chevron_right", 1, "calendar-next") },
    },
    ui.Row { x = 15, y = 62, gap = 0, table.unpack(names) },
    ui.Grid { x = 15, y = 92, columns = 7, gap = 0, table.unpack(cells) },
  }
end

-- -------------------------------------------------------------- resources --

local function ring(icon, value, color, id)
  local D, STROKE = 80, 5
  local r = D / 2 - STROKE / 2
  local circle = ("M%g %g A%g %g 0 1 1 %g %g"):format(D / 2, STROKE / 2, r, r, D / 2 - 0.01, STROKE / 2)
  return ui.Item {
    id = id, width = D, height = D,
    ui.Path {
      anchors = { fill = true }, view_box = { 0, 0, D, D }, d = circle,
      fill_color = "transparent", stroke_width = STROKE, stroke_cap = "round",
      stroke_color = function() return C.secondaryContainer end,
    },
    ui.Path {
      anchors = { fill = true }, view_box = { 0, 0, D, D }, d = circle,
      fill_color = "transparent", stroke_width = STROKE, stroke_cap = "round",
      stroke_color = color,
      trim_end = function() return math.max(0.001, math.min(1, value() / 100)) end,
      behavior = { trim_end = { duration = theme.duration.large, easing = theme.ease.emphasized_decel } },
    },
    kit.icon(icon, 26, color, { anchors = { center_in = true } }),
  }
end

local function resources_card()
  local sysinfo = require("lib.sysinfo")
  local function gated(read)
    return function()
      if not opened:get() then return 0 end
      local ok, v = pcall(read)
      return ok and tonumber(v) or 0
    end
  end
  local disk = function()
    local list = sysinfo.disks() or {}
    for _, d in ipairs(list) do if d.mount == "/" then return d.percent end end
    return list[1] and list[1].percent or 0
  end
  return kit.card {
    id = "dashboard-resources",
    width = 113, height = ROW2,
    ui.Column {
      anchors = { center_in = true }, gap = 12,
      ring("memory", gated(function() return sysinfo.cpu().usage end), function() return C.primary end, "ring-cpu"),
      ring("memory_alt", gated(function() return sysinfo.memory().percent end), function() return C.tertiary end, "ring-memory"),
      ring("hard_drive", gated(disk), function() return C.secondary end, "ring-storage"),
    },
  }
end

-- ------------------------------------------------------------------ media --

local function media_card()
  local media = services.media
  local active = services.player
  local function field(name, fallback)
    return function()
      local v = active()[name]
      if type(v) == "table" then v = table.concat(v, ", ") end
      return (v and v ~= "") and v or fallback
    end
  end
  local function button(icon, action, id, wide)
    return kit.hover(ui.MouseArea {
      id = id, width = wide and 72 or 44, height = 44, cursor = "pointer",
      on_clicked = function() if media then pcall(media[action]) end end,
      kit.icon(icon, 22, function() return C.onSurfaceVariant end, { anchors = { center_in = true } }),
    }, function(hovered) return hovered and C.surfaceContainerHighest or C.surfaceContainerHigh end, 22)
  end
  local D = 176
  local r = D / 2 - 4
  -- From a little below the left of centre, over the top, to the right.
  local a0 = math.rad(190)
  local sx, sy = D / 2 + r * math.cos(a0), D / 2 - r * math.sin(a0)
  local ex = D - sx
  local arc = ("M%g %g A%g %g 0 1 1 %g %g"):format(sx, sy, r, r, ex, sy)
  local ends = { sx, sy, ex }
  local cover = ui.Item {
    width = D, height = D,
    -- The track's progress: an arc over the top, a dot at either end.
    ui.Path {
      anchors = { fill = true }, view_box = { 0, 0, D, D }, d = arc,
      fill_color = "transparent", stroke_width = 3, stroke_cap = "round",
      stroke_color = function() return C.secondaryContainer end,
    },
    ui.Path {
      anchors = { fill = true }, view_box = { 0, 0, D, D }, d = arc,
      fill_color = "transparent", stroke_width = 3, stroke_cap = "round",
      stroke_color = function() return C.primary end,
      trim_end = function()
        local a = active()
        if not a.length or a.length <= 0 then return 0 end
        return math.min(1, (a.position or 0) / a.length)
      end,
    },
    ui.Rect { x = ends[1] - 3, y = ends[2] - 3, width = 6, height = 6, radius = 3, color = function() return C.primary end },
    ui.Rect { x = ends[3] - 3, y = ends[2] - 3, width = 6, height = 6, radius = 3, color = function() return C.primary end },
    ui.Path {
      anchors = { fill = true, margins = 20 }, view_box = { 0, 0, 100, 100 },
      d = shapes.path("cookie12"),
      fill_color = function() return C.surfaceContainerHighest end,
    },
    kit.icon("art_track", 64, function() return C.onSurfaceVariant end, {
      anchors = { center_in = true },
      visible = function() return (active().art_url or "") == "" end,
    }),
    ui.Image {
      id = "media-cover",
      anchors = { fill = true, margins = 20 }, fill_mode = "preserve_aspect_crop",
      source = function() return active().art_url or "" end,
      visible = function() return (active().art_url or "") ~= "" end,
      mask = ui.Path {
        view_box = { 0, 0, 100, 100 }, d = shapes.path("cookie12"), fill_color = "#ffffff",
      },
    },
  }
  return kit.card {
    id = "dashboard-media",
    width = 200, height = ROW1 + GAP + ROW2,
    ui.Column {
      anchors = { horizontal_center = true }, y = 12, gap = 6, align = "center",
      cover,
      kit.text {
        id = "media-title", width = 170, horizontal_alignment = "center", elide = "right",
        text = field("title", "No media"), font_size = theme.size.larger + 1, font_weight = 500,
        color = function() return C.primary end,
      },
      kit.text {
        width = 170, horizontal_alignment = "center", elide = "right",
        text = field("album", "No media"), color = function() return C.outline end,
      },
      kit.text {
        width = 170, horizontal_alignment = "center", elide = "right",
        text = field("artist", "No media"), color = function() return C.onSurfaceVariant end,
      },
      ui.Item { width = 1, height = 4 },
      ui.Row {
        gap = 6, align = "center",
        button("skip_previous", "previous", "media-previous"),
        button(function() return active().playing and "pause" or "play_arrow" end, "play_pause", "media-play", true),
        button("skip_next", "next", "media-next"),
      },
      -- TODO(phase 2): the visualiser under the controls (lib/spectrum.lua).
    },
  }
end

-- -------------------------------------------------------------- the panel --

local function dashboard_tab()
  return ui.Row {
    gap = GAP,
    ui.Column {
      gap = GAP,
      ui.Row { gap = GAP, weather_card(), user_card() },
      ui.Row { gap = GAP, clock_card(), calendar_card(), resources_card() },
    },
    media_card(),
  }
end

local pages = {
  dashboard_tab(),
  require("dashboard_media").page,
  require("dashboard_performance").page,
  require("dashboard_weather").page,
}

-- Where page `i` starts along the track: the pages side by side, a
-- padding's width apart on either side.
local function offset(i)
  local x = 0
  for k = 1, i - 1 do x = x + PAGE[k][1] + PAD * 2 end
  return x
end

-- The tabs slide sideways as the reference's do, while the drawer eases to
-- the new tab's size: the strip is the panel less its padding, so it
-- follows the drawer as it moves.
local strip = ui.Item {
  id = "dashboard-pages",
  anchors = { fill = true, left_margin = PAD, right_margin = PAD, top_margin = TABS_H + PAD, bottom_margin = PAD - 1 },
  clip = true,
}
local track = ui.Row {
  gap = PAD * 2,

  translate_x = function() return -offset(M.tab:get()) end,
  behavior = { translate_x = SWITCH },
  table.unpack(pages),
}
ui.reparent(track, strip)

-- Behind everything on the panel, so the panel is in the surface's input
-- region: the pointer is seen anywhere on it, and `contains_pointer` with it.
local background = ui.MouseArea { anchors = { fill = true }, z = -1 }

local content = ui.Item {
  anchors = { fill = true },
  background,
  tabs(),
  strip,
}

M.drawer = drawer.new {
  name = "dashboard",
  edge = "top",
  width = width,
  height = height,
  content = content,
  props = { behavior = { width = SWITCH, height = SWITCH } },
}

morf.effect("caelestia.dashboard.shown", function()
  opened:set(M.drawer.open:get())
end)

-- ------------------------------------------------------------- hover open --

local by_hover = false

-- The strip along the frame's top edge over the dashboard: reaching it
-- opens the dashboard.
local trigger = ui.MouseArea {
  id = "dashboard-trigger",
  anchors = { top = true, horizontal_center = true },
  width = width, height = theme.BORDER,
}

-- Anywhere on the panel, whatever is under the pointer there: a tab, a
-- calendar arrow, a media button.
local function panel_hovered()
  return M.drawer.panel.contains_pointer
end

--- The trigger, in a box as wide as the frame's opening so it centres
--- over the dashboard.
function M.edge_trigger()
  return ui.Item {
    anchors = { fill = true, left_margin = theme.BAR, right_margin = theme.BORDER },
    trigger,
  }
end

local closing
morf.effect("caelestia.dashboard.hover", function()
  if not config.get("dashboard.hover") then return end
  local over = trigger.hovered or panel_hovered()
  if over then
    if closing then closing:cancel() closing = nil end
    if not M.drawer.open:get() then
      by_hover = true
      M.drawer.set(true)
    end
  elseif by_hover and M.drawer.open:get() then
    -- A moment's grace (the reference shuts at once), so the pointer
    -- crossing from the edge onto the panel does not shut it.
    if closing then closing:cancel() end
    closing = morf.timer(50, function()
      closing = nil
      if not (trigger.hovered or panel_hovered()) then
        by_hover = false
        M.drawer.set(false)
      end
    end, false)
  end
end)

morf.effect("caelestia.dashboard.opened-otherwise", function()
  -- Opened over IPC: stays until asked to close.
  if not M.drawer.open:get() then by_hover = false end
end)

return M
