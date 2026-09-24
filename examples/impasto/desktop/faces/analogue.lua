-- The Analogue theme's registry: one builder per module, each laying itself
-- out at any family.
--
-- Port of faces/analogue/Analogue.qml and the small faces it lists
-- (ClockFace, BatteryFace, StatsFace, ClaudeFace, MediaFace, TimerFace,
-- VolumeFace, BrightnessFace, TasksFace, UpdatesFace, NetworkFace,
-- BluetoothFace, CreatureFace, GamesFace, GithubFace). The larger ones --
-- the calendar, the weather and the photo -- have files of their own under
-- analogue/, beside the objects every face draws (dial, gauge, knob, fader,
-- record, hourglass, parcel, leaf...). Faces read the same sources as the
-- Modern ones and draw in the widget's ink; notes and the spectrum are the
-- same in both themes and are not here.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local common = require("desktop.faces.common")
local S = require("desktop.sources")
local stats = require("services.stats")
local claude = require("services.claude")
local github = require("services.github")
local instrument = require("desktop.faces.analogue.instrument")
local dial = require("desktop.faces.analogue.dial")
local gauge = require("desktop.faces.analogue.gauge")
local knob = require("desktop.faces.analogue.knob")
local fader = require("desktop.faces.analogue.fader")
local arcs = require("desktop.faces.analogue.arcs")
local battery_cell = require("desktop.faces.analogue.battery_cell")
local record = require("desktop.faces.analogue.record")
local hourglass = require("desktop.faces.analogue.hourglass")
local parcel = require("desktop.faces.analogue.parcel")
local joystick = require("desktop.faces.analogue.joystick")
local clipboard = require("desktop.faces.analogue.clipboard")
local svg = require("desktop.faces.analogue.svg")
local draw = require("pets.draw")

local M = {}
local n = svg.n

local function centred(w, h, size, node_of)
  return ui.Item { width = w, height = h, node_of((w - size) / 2, (h - size) / 2, size) }
end

local function bar_extra(ctx, progress, fill, caption)
  return function(w, h)
    local column = { gap = 4, width = w,
      common.bar { width = w, progress = progress, fill_color = fill or ctx.ink.accent, track_color = ctx.ink.raised } }
    if caption then
      column[#column + 1] = kit.text { width = w, elide = "right", text = caption,
        size = theme.size.label, color = ctx.ink.muted }
    end
    return ui.Item { width = w, height = h, ui.Column(column) }
  end
end

local function transport(ctx, gap, size)
  local ink = ctx.ink
  return ui.Row {
    gap = gap, align = "center",
    opacity = function() return S.media.available() and 1 or 0.4 end,
    common.icon_button { ink = ink, glyph = "󰒮", glyph_size = size, on_click = S.media.previous },
    common.icon_button { ink = ink, glyph = function() return S.media.playing() and "󰏤" or "󰐊" end,
      glyph_size = size + 2, on_click = S.media.toggle },
    common.icon_button { ink = ink, glyph = "󰒭", glyph_size = size, on_click = S.media.next },
  }
end

-- ------------------------------------------------------------------ clock --

-- An analogue dial at every size. 4x2 adds the weekday and the date beside
-- it; 4x4 adds numerals, minute marks and a date window.
function M.clock(ctx)
  if ctx.family == "4x4" then
    local size = math.min(ctx.width, ctx.height) - 44
    return ui.Item { width = ctx.width, height = ctx.height,
      dial.build { x = (ctx.width - size) / 2, y = (ctx.height - size) / 2, size = size, ink = ctx.ink,
        numerals = true, minute_ticks = true, date_window = true } }
  end
  return instrument.build(ctx, {
    reading = function() return S.clock.format("%A") end,
    note = function() return S.clock.format("%-d %B %Y") end,
    object = function(w, h)
      return centred(w, h, math.min(w, h), function(x, y, s) return dial.build { x = x, y = y, size = s, ink = ctx.ink } end)
    end,
  })
end

-- --------------------------------------------------------------- battery --

local function battery_word()
  if not S.battery.available() then return "no battery" end
  if S.battery.charging() then return "charging" end
  if S.battery.percent() >= 100 then return "full" end
  return "discharging"
end

-- A battery cell filled to the charge, with a bolt while charging; the fill
-- takes the fixed battery red when the charge is low.
function M.battery(ctx)
  local ink = ctx.ink
  local function low() return not S.battery.charging() and S.battery.percent() <= 20 end
  return instrument.build(ctx, {
    line = function()
      if not S.battery.available() then return "No battery" end
      local e = S.battery.estimate()
      return S.battery.percent() .. "%" .. (e ~= "" and (" · " .. e) or "")
    end,
    reading = function() return S.battery.available() and (S.battery.percent() .. "%") or "—" end,
    note = function()
      if not S.battery.available() then return "no battery" end
      local e = S.battery.estimate()
      return battery_word() .. (e ~= "" and (" · " .. e) or "")
    end,
    object = function(w, h)
      local size = math.floor(math.min(w, h / 0.48))
      return ui.Item { width = w, height = h,
        battery_cell.build { x = (w - size) / 2, y = (h - size * 0.48) / 2, size = size, ink = ink,
          fraction = function() return S.battery.percent() / 100 end,
          charging = function() return S.battery.charging() or (S.battery.available() and S.battery.percent() >= 100) end,
          fill = function() return low() and S.battery.tint() or ink.text() end } }
    end,
  })
end

-- ------------------------------------------------------------------- stats --

local function uptime()
  local up = stats.uptime()
  if not up then return "—" end
  local ok, text = pcall(morf.time.duration, up, "short")
  return ok and text or tostring(math.floor(up / 3600)) .. "h"
end

-- System load as gauges: the processor on a 2x2; the processor and memory
-- side by side with the load and uptime on a 4x2; at 4x4 the two gauges
-- above three traces of recent history.
function M.stats(ctx)
  local ink = ctx.ink
  local cpu_value = function() return string.format("%d%%", math.floor(stats.cpu() + 0.5)) end
  if ctx.family == "2x2" then
    return instrument.build(ctx, {
      line = function() return string.format("ram %d%%", math.floor(stats.memory_fraction() * 100 + 0.5)) end,
      object = function(w, h)
        return centred(w, h, math.min(w, h), function(x, y, s)
          return gauge.build { x = x, y = y, size = s, ink = ink, label = "cpu", value = cpu_value,
            fraction = function() return stats.cpu() / 100 end }
        end)
      end,
    })
  end
  local w, h = ctx.width, ctx.height
  local large = ctx.family == "4x4"
  local upper = large and h / 2 or h
  local g = 128
  local figures = {}
  for _, f in ipairs {
    { function() return string.format("%.2f", (stats.load()[1] or 0)) end, "load" },
    { uptime, "up" },
  } do
    figures[#figures + 1] = ui.Column { gap = 1, width = w - (22 + 2 * g + 28) - 22,
      kit.text { width = w - (22 + 2 * g + 28) - 22, elide = "right", text = f[1],
        size = theme.size.large, weight = 600, color = ink.text },
      kit.text { text = f[2], size = theme.size.label, letter_spacing = 1, color = ink.muted },
    }
  end
  local children = {
    gauge.build { x = 22, y = (upper - g) / 2, size = g, ink = ink, label = "cpu", value = cpu_value,
      fraction = function() return stats.cpu() / 100 end },
    gauge.build { x = 22 + g + 8, y = (upper - g) / 2, size = g, ink = ink, label = "memory",
      value = function() return stats.bytes(stats.memory_used()) end, fraction = stats.memory_fraction },
    ui.Column { x = 22 + 2 * g + 28, y = upper / 2 - 44, gap = 10, table.unpack(figures) },
  }
  if large then
    local traces = {
      { "Processor", cpu_value, "cpu" },
      { "Memory", function() return stats.bytes(stats.memory_used()) end, "memory" },
      { "Network", function() return stats.rate(stats.down_rate()) end, "rx" },
    }
    local tw = w - 44
    local th = math.floor((upper - 22 - 20) / 3)
    local nodes = {}
    for i, t in ipairs(traces) do
      nodes[i] = ui.Item { width = tw, height = th,
        kit.text { text = t[1], size = theme.size.label, color = ink.muted },
        kit.text { mono = true, anchors = { right = true }, text = t[2], size = theme.size.label, color = ink.text },
        common.sparkline { y = 14, width = math.floor(tw), height = math.max(8, th - 16),
          values = function() return stats.history(t[3]) end, stroke = ink.accent },
      }
    end
    children[#children + 1] = ui.Column { x = 22, y = upper, width = tw, gap = 10, table.unpack(nodes) }
  end
  return ui.Item { width = w, height = h, table.unpack(children) }
end

-- ------------------------------------------------------------------ claude --

-- Claude usage as a fuel gauge: F with the block untouched, E when it is
-- spent, red at the empty end.
function M.claude(ctx)
  local ink = ctx.ink
  local function figure()
    if not claude.available() then return "—" end
    if claude.session_measured() then return claude.percent(claude.session_fraction()) end
    return claude.compact(claude.block_tokens())
  end
  return instrument.build(ctx, {
    line = function()
      if not claude.available() then return "No usage found" end
      return figure() .. (claude.session_measured() and " of this block" or " this block")
    end,
    reading = figure,
    note = function()
      if not claude.available() then return "no usage found" end
      local resets = claude.resets_in()
      return (claude.session_measured() and "of this block" or "this block") .. (resets ~= "" and (" · " .. resets) or "")
    end,
    filled = true,
    object = function(w, h)
      local s = math.min(w, h)
      return centred(w, h, s, function(x, y, size)
        return gauge.build { x = x, y = y, size = size, ink = ink, low_is_bad = true, ends = { "E", "F" },
          fraction = function() return 1 - claude.gauge() end,
          hub = common.claude_mark { x = (size - 22) / 2, y = size * 0.28, size = 22, color = ink.text } }
      end)
    end,
    extra = bar_extra(ctx, claude.weekly_fraction, ink.accent, function()
      return claude.weekly_measured() and (claude.percent(claude.weekly_fraction()) .. " of the week") or "this week"
    end),
  })
end

-- ------------------------------------------------------------------- media --

local function media_title()
  local t = S.media.title()
  if t ~= "" then return t end
  return S.media.available() and S.media.identity() or "Nothing playing"
end

-- A record on a turntable, with the title under it on a 2x2 and beside it on
-- a 4x2, and the transport and progress under the text. At 4x4 the
-- transport moves into the body.
function M.media(ctx)
  local ink = ctx.ink
  local large = ctx.family == "4x4"
  return instrument.build(ctx, {
    line = media_title,
    reading = media_title,
    note = function()
      if not S.media.available() then return "no player on the bus" end
      local a = S.media.artist()
      return (a ~= "" and (a .. " · ") or "") .. (S.media.playing() and "playing" or "paused")
    end,
    filled = true,
    object = function(w, h)
      return centred(w, h, math.min(w, h), function(x, y, s)
        return record.build { x = x, y = y, size = s, ink = ink, playing = S.media.playing, art = S.media.art,
          shown = common.shown(ctx) }
      end)
    end,
    extra = not large and function(w, h)
      return ui.Item { width = w, height = h,
        ui.Column { width = w, gap = 8,
          common.bar { width = w, progress = S.media.progress, fill_color = ink.accent, track_color = ink.raised },
          transport(ctx, 16, 16) } }
    end or nil,
    body = large and function(w, h)
      return ui.Item { width = w, height = h,
        ui.Column { x = 0, y = (h - 100) / 2, width = w, gap = 14, align = "center",
          kit.text { width = w, elide = "right", horizontal_alignment = "center",
            text = function() return S.media.identity() end, size = theme.size.regular, color = ink.muted },
          common.bar { width = w, progress = S.media.progress, fill_color = ink.accent, track_color = ink.raised },
          transport(ctx, 28, 22) } }
    end or nil,
  })
end

-- ------------------------------------------------------------------- timer --

local function timer_extra(ctx)
  return function(w, h)
    return ui.Item { width = w, height = h,
      ui.Row { gap = 10, align = "center",
        common.pill {
          height = 30,
          text = function()
            if not S.timer.running() then return "5 min" end
            return S.timer.paused() and "Resume" or "Hold"
          end,
          on_click = function()
            if not S.timer.running() then S.timer.start(5 * 60 * 1000, "") else S.timer.toggle() end
          end,
        },
        common.icon_button { ink = ctx.ink, glyph = "󰅖", glyph_size = 13,
          opacity = function() return S.timer.running() and 1 or 0.35 end,
          on_click = function() if S.timer.running() then S.timer.cancel() end end },
      } }
  end
end

-- The timer as an hourglass: the sand above is the time left, and the
-- thread runs while it counts and stops when paused.
function M.timer(ctx)
  local ink = ctx.ink
  return instrument.build(ctx, {
    line = function()
      if not S.timer.running() then return "Nothing running" end
      local l = S.timer.label()
      return S.timer.display() .. (l ~= "" and (" · " .. l) or "")
    end,
    reading = function() return S.timer.running() and S.timer.display() or "—" end,
    note = function()
      if not S.timer.running() then return "nothing running" end
      local l = S.timer.label()
      return l ~= "" and l or "counting down"
    end,
    tint = function() return S.timer.running() and ink.text() or ink.muted() end,
    filled = true,
    object = function(w, h)
      local size = math.floor(math.min(h, w / 0.72))
      return ui.Item { width = w, height = h,
        hourglass.build { x = (w - size * 0.72) / 2, y = (h - size) / 2, size = size, ink = ink,
          fraction = function() return S.timer.running() and S.timer.progress() or 0 end,
          running = function() return S.timer.running() and not S.timer.paused() end } }
    end,
    extra = timer_extra(ctx),
  })
end

-- ------------------------------------------------------ volume, brightness --

-- A knob that turns the output's level; muted, a bar crosses it.
function M.volume(ctx)
  local ink = ctx.ink
  return instrument.build(ctx, {
    line = function() return S.audio.muted() and "Muted" or (S.audio.volume() .. "%") end,
    reading = function() return S.audio.muted() and "Muted" or (S.audio.volume() .. "%") end,
    note = function() return S.audio.muted() and (S.audio.volume() .. "% · muted") or "output" end,
    tint = function() return S.audio.muted() and ink.muted() or ink.text() end,
    filled = true,
    object = function(w, h)
      return centred(w, h, math.min(w, h), function(x, y, s)
        return knob.build { x = x, y = y, size = s, ink = ink,
          fraction = function() return S.audio.volume() / 100 end, muted = S.audio.muted,
          set = function(f) S.audio.set_volume(math.floor(f * 100 + 0.5)) end,
          toggle = S.audio.toggle_mute }
      end)
    end,
    extra = bar_extra(ctx, function() return S.audio.muted() and 0 or S.audio.volume() / 100 end),
  })
end

-- A fader for the backlight.
function M.brightness(ctx)
  local ink = ctx.ink
  return instrument.build(ctx, {
    line = function() return S.brightness.available() and (S.brightness.percent() .. "%") or "No backlight" end,
    reading = function() return S.brightness.available() and (S.brightness.percent() .. "%") or "—" end,
    note = function() return S.brightness.available() and "backlight" or "no backlight" end,
    filled = true,
    object = function(w, h)
      local size = math.floor(math.min(h, w * 2))
      return ui.Item { width = w, height = h,
        fader.build { x = (w - fader.WIDTH) / 2, y = (h - size) / 2, size = size, ink = ink,
          fraction = function() return S.brightness.percent() / 100 end,
          set = function(f) S.brightness.set(math.floor(f * 100 + 0.5)) end } }
    end,
    extra = bar_extra(ctx, function() return S.brightness.percent() / 100 end),
  })
end

-- ------------------------------------------------------------------- tasks --

local function tasks_count()
  if not S.tasks.available() then return "No board yet" end
  if S.tasks.pending() == 0 then return S.tasks.count() == 0 and "Nothing yet" or "All done" end
  local late = S.tasks.overdue()
  return S.tasks.pending() .. " to do" .. (late > 0 and (" · " .. late .. " late") or "")
end

local function open_board()
  local ok, island = pcall(require, "bar.island")
  if ok then pcall(island.toggle, "board") end
end

-- The tasks on a clipboard: the next three on a 2x2, the same three beside
-- the count on a 4x2, and nine lines at 4x4.
function M.tasks(ctx)
  local ink = ctx.ink
  if ctx.family == "4x4" then
    return ui.Item { width = ctx.width, height = ctx.height,
      clipboard.build { x = 22, y = 30, width = ctx.width - 44, height = ctx.height - 52, ctx = ctx, count = 9,
        title = function() return "Tasks · " .. tasks_count():lower() end } }
  end
  return instrument.build(ctx, {
    line = tasks_count,
    reading = function() return tostring(S.tasks.pending()) end,
    note = function()
      if not S.tasks.available() then return "no board yet" end
      if S.tasks.pending() == 0 then return S.tasks.count() == 0 and "nothing yet" or "all done" end
      return S.tasks.summary()
    end,
    tint = function() return S.tasks.overdue() > 0 and ink.red() or ink.text() end,
    filled = true,
    object = function(w, h)
      return clipboard.build { x = 0, y = 8, width = w, height = h - 8, ctx = ctx, count = 3 }
    end,
    extra = function(w, h)
      return ui.Item { width = w, height = h, common.pill { height = 28, text = "Open", on_click = open_board } }
    end,
  })
end

-- ----------------------------------------------------------------- updates --

local function updates_line()
  if S.updates.checking() then return "Checking" end
  if not S.updates.available() then return "Cannot check" end
  return S.updates.count() == 0 and "Up to date" or (S.updates.count() .. " pending")
end

-- A parcel with the count on its label.
function M.updates(ctx)
  local ink = ctx.ink
  local none = function() return S.updates.count() == 0 end
  return instrument.build(ctx, {
    line = updates_line,
    reading = function() return (S.updates.available() or S.updates.checking()) and tostring(S.updates.count()) or "—" end,
    note = function()
      if S.updates.checking() then return "checking" end
      if not S.updates.available() then return "cannot check" end
      return none() and "up to date" or "pending"
    end,
    tint = function() return none() and ink.muted() or ink.text() end,
    filled = true,
    object = function(w, h)
      return centred(w, h, math.min(w, h), function(x, y, s)
        return parcel.build { x = x, y = y, size = s, ink = ink,
          count = function() return S.updates.available() and tostring(S.updates.count()) or "?" end }
      end)
    end,
    extra = function(w, h)
      local lines = {}
      for i = 1, 2 do
        lines[i] = kit.text { mono = true, width = w, elide = "right", size = theme.size.label, color = ink.muted,
          text = function() return S.updates.packages(2)[i] or "" end }
      end
      return ui.Item { width = w, height = h, ui.Column { gap = 1, width = w, table.unpack(lines) } }
    end,
  })
end

-- ------------------------------------------------------ network, bluetooth --

-- Wi-Fi as arcs lit by signal strength; a plug when wired.
function M.network(ctx)
  local ink = ctx.ink
  return instrument.build(ctx, {
    line = S.network.name, reading = S.network.name, note = S.network.state_line,
    object = function(w, h)
      return centred(w, h, math.min(w, h), function(x, y, s)
        return arcs.build { x = x, y = y, size = s, ink = ink,
          strength = function() return (S.network.strength() or 0) / 100 end,
          connected = S.network.wifi,
          wired = function() return S.network.online() and not S.network.wifi() end }
      end)
    end,
  })
end

-- The Bluetooth rune, with two rings that take the accent while a device is
-- connected.
function M.bluetooth(ctx)
  local ink = ctx.ink
  local function connected() return #S.bluetooth.devices() end
  return instrument.build(ctx, {
    line = S.bluetooth.summary, reading = S.bluetooth.summary,
    note = function()
      if not S.bluetooth.available() then return "no adapter" end
      if not S.bluetooth.enabled() then return "off" end
      local c = connected()
      return c > 0 and (c .. " connected") or "nothing connected"
    end,
    object = function(w, h)
      local side = math.min(w, h)
      local children = {}
      for i, f in ipairs { 0.62, 0.9 } do
        local d = side * f
        children[#children + 1] = ui.Rect {
          x = (w - d) / 2, y = (h - d) / 2, width = d, height = d, radius = d / 2,
          color = morf.color("transparent"),
          border_color = function() return connected() > 0 and ink.accent() or ink.dim() end,
          border_width = i == 1 and 2 or 1, opacity = i == 1 and 1 or 0.6,
        }
      end
      children[#children + 1] = kit.glyph {
        x = 0, y = 0, width = w, height = h, vertical_alignment = "center",
        glyph = "󰂯", size = math.floor(side * 0.4 + 0.5),
        color = function() return S.bluetooth.enabled() and ink.text() or ink.muted() end,
      }
      return ui.Item { width = w, height = h, table.unpack(children) }
    end,
  })
end

-- --------------------------------------------------------------------- pet --

-- The pet, larger, standing on a line with its name under it; 4x2 adds its
-- mood and a bar of progress to the next level.
function M.pet(ctx)
  local ink = ctx.ink
  local pets = S.pets
  local function name() local nm = pets.name() return nm ~= "" and nm or "Pet" end
  return instrument.build(ctx, {
    line = function() return pets.hatched() and (name() .. " · Lv " .. pets.level()) or "An egg" end,
    reading = name,
    note = pets.mood_line,
    filled = true,
    object = function(w, h)
      local pet_face = require("pets.face")
      local size = math.floor(math.min(w, h) * 0.5)
      return ui.Item { width = w, height = h,
        ui.Item { x = (w - size) / 2, y = (h - size) / 2 - 8, width = size, height = size,
          pet_face.lively(pet_face.new { size = size, lively = true }, 3, 10) },
        ui.Rect { x = w * 0.15, y = h - h * 0.14 - 2, width = w * 0.7, height = 2, radius = 1, color = ink.dim },
      }
    end,
    extra = bar_extra(ctx, pets.progress, ink.accent, function()
      return pets.hatched() and ("Lv " .. pets.level()) or "not hatched yet"
    end),
  })
end

-- ------------------------------------------------------------------- games --

local function last_game()
  local id = S.games.last_played()
  return id and S.games.entry(id) or nil
end

local function open_games()
  local ok, island = pcall(require, "bar.island")
  if ok then pcall(island.toggle, "games") end
end

-- The arcade's mark, an arcade stick, with the last game's best.
function M.games(ctx)
  local ink = ctx.ink
  return instrument.build(ctx, {
    line = function()
      local last = last_game()
      return last and (last.name .. " · best " .. S.games.best_of(last.id)) or "Nothing played yet"
    end,
    reading = function() local last = last_game() return last and tostring(S.games.best_of(last.id)) or "—" end,
    note = function() local last = last_game() return last and (last.name .. " · best") or "nothing played yet" end,
    filled = true,
    object = function(w, h)
      return centred(w, h, math.min(w, h), function(x, y, s) return joystick.build { x = x, y = y, size = s, ink = ink } end)
    end,
    extra = function(w, h)
      return ui.Item { width = w, height = h, common.pill { height = 28, text = "Play", on_click = open_games } }
    end,
  })
end

-- ------------------------------------------------------------------ github --

-- The contribution wall with embossed tiles instead of flat ones: each tile
-- lit along its top and shaded along its bottom, from its own colour.
local function wall_doc(weeks, w, h)
  local spacing, radius = 3, 2.5
  local cell = math.min(24, (h - 6 * spacing) / 7)
  local step = cell + spacing
  local columns = math.max(1, math.floor((w + spacing) / step))
  local shown = math.min(columns, #weeks)
  local left = (w - (shown * step - spacing)) / 2
  local top = (h - (7 * step - spacing)) / 2
  local parts = {}
  for i = 1, shown do
    local week = weeks[#weeks - shown + i] or {}
    for d = 1, 7 do
      local level = week[d]
      if type(level) == "table" then level = level.level end
      if type(level) == "number" and level >= 0 then
        local colour = theme.github_levels[math.min(5, math.floor(level) + 1)]
        local x, y = left + (i - 1) * step, top + (d - 1) * step
        parts[#parts + 1] = string.format(
          '<rect x="%s" y="%s" width="%s" height="%s" rx="%s" fill="%s"/>' ..
          '<rect x="%s" y="%s" width="%s" height="%s" rx="%s" fill="%s"/>' ..
          '<rect x="%s" y="%s" width="%s" height="%s" rx="%s" fill="%s"/>',
          n(x), n(y), n(cell), n(cell), n(radius), draw.darker(colour, 1.35),
          n(x), n(y), n(cell), n(cell - 1.5), n(radius), draw.lighter(colour, 1.3),
          n(x + 1), n(y + 1.2), n(cell - 2), n(cell - 2.7), n(radius * 0.8), colour)
      end
    end
  end
  return svg.doc(w, h, table.concat(parts))
end

function M.github(ctx)
  local w, h = ctx.width, ctx.height
  return ui.Item {
    width = w, height = h,
    ui.Image { x = 12, y = 12, width = w - 24, height = h - 24, visible = github.available,
      source = function() return wall_doc(github.weeks(), w - 24, h - 24) end },
    kit.text { anchors = { center_in = true }, width = w - 24, wrap = true, horizontal_alignment = "center",
      visible = function() return not github.available() end,
      text = github.reason, size = theme.size.small, color = ctx.ink.muted },
  }
end

-- --------------------------------------------------- the ones in own files --

function M.calendar(ctx) return require("desktop.faces.analogue.calendar").build(ctx) end
function M.weather(ctx) return require("desktop.faces.analogue.weather").build(ctx) end
function M.photo(ctx) return require("desktop.faces.analogue.photo").build(ctx) end

return M
