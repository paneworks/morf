-- 4x2 faces: the square with its right half filled in.
--
-- Port of faces/Wides.qml. The mark, label, reading and caption stay where
-- they are on the square; whatever the square had no room for -- a bar, a
-- sparkline, the next hours, the transport, the week -- goes on the right.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local common = require("desktop.faces.common")
local squares = require("desktop.faces.squares")
local S = require("desktop.sources")
local weather = require("services.weather")
local stats = require("services.stats")
local claude = require("services.claude")

local face = common.widget_face
local glyph = common.glyph
local M = {}

local function ring_mark(ctx, progress, fill, inner)
  return common.ring {
    size = 40, thickness = 3, progress = progress,
    track_color = ctx.ink.dim, fill_color = fill or ctx.ink.text, inner,
  }
end

local function centred(ctx, g, size, color)
  return glyph { glyph = g, size = size or 16, color = color or ctx.ink.text, anchors = { center_in = true }, box = 40 }
end

-- A bar across the extra area, with an optional line under it.
local function bar_extra(ctx, progress, fill, caption)
  return function(w, h)
    local bar = common.bar { width = w, progress = progress, fill_color = fill or ctx.ink.text, track_color = ctx.ink.raised }
    local column = ui.Column {
      gap = 8, width = w,
      bar,
      caption and kit.text {
        width = w, text = caption, horizontal_alignment = "right", elide = "right",
        size = theme.size.small, color = ctx.ink.muted,
      } or nil,
    }
    return ui.Item { width = w, height = h, ui.Item { anchors = { left = true, right = true, vertical_center = true }, height = caption and 26 or 6, column } }
  end
end

function M.battery(ctx)
  return face(ctx, {
    label = "Battery",
    reading = function() return S.battery.available() and (S.battery.percent() .. "%") or "—" end,
    note = function() return S.battery.available() and S.battery.estimate() or "no battery" end,
    extra_share = 0.42,
    mark = glyph { glyph = S.battery.icon, size = 30, color = ctx.ink.text },
    extra = bar_extra(ctx, function() return S.battery.percent() / 100 end, S.battery.tint, function()
      if not S.battery.available() then return "" end
      local w = S.battery.watts()
      local e = S.battery.energy()
      if w > 0 and e > 0 then return string.format("%.1f W · %.1f Wh", w, e) end
      if w > 0 then return string.format("%.1f W", w) end
      return e > 0 and string.format("%.1f Wh", e) or ""
    end),
  })
end

function M.volume(ctx)
  local ink = ctx.ink
  return face(ctx, {
    label = "Volume",
    reading = function() return S.audio.muted() and "Muted" or (S.audio.volume() .. "%") end,
    note = function() return S.audio.muted() and "output silenced" or "output" end,
    tint = function() return S.audio.muted() and ink.muted() or ink.text() end,
    extra_share = 0.42,
    mark = ring_mark(ctx, function() return S.audio.muted() and 0 or S.audio.volume() / 100 end, nil,
      centred(ctx, S.audio.icon, 16)),
    extra = bar_extra(ctx, function() return S.audio.muted() and 0 or S.audio.volume() / 100 end),
  })
end

function M.brightness(ctx)
  return face(ctx, {
    label = "Brightness",
    reading = function() return S.brightness.available() and (S.brightness.percent() .. "%") or "—" end,
    note = function() return S.brightness.available() and "backlight" or "no backlight" end,
    extra_share = 0.42,
    mark = ring_mark(ctx, function() return S.brightness.percent() / 100 end, nil, centred(ctx, S.brightness.icon, 16)),
    extra = bar_extra(ctx, function() return S.brightness.percent() / 100 end),
  })
end

function M.claude(ctx)
  return face(ctx, {
    label = "Claude",
    reading = squares.claude_reading,
    note = function()
      if not claude.available() then return "no usage found" end
      if claude.session_measured() then return "of this block · " .. claude.compact(claude.block_tokens()) end
      return "this block · " .. claude.messages(claude.block_messages())
    end,
    extra_share = 0.42,
    mark = common.claude_mark { size = 32, x = 4, y = 4, color = claude.tint },
    extra = function(w, h)
      return ui.Item { width = w, height = h, visible = claude.available,
        bar_extra(ctx, claude.gauge, claude.tint, claude.resets_in)(w, h) }
    end,
  })
end

function M.stats(ctx)
  return face(ctx, {
    label = "System",
    reading = function() return string.format("%d%%", math.floor(stats.cpu() + 0.5)) end,
    note = function()
      return string.format("RAM %d%% · load %.2f", math.floor(stats.memory_fraction() * 100 + 0.5), stats.load())
    end,
    extra_share = 0.45,
    mark = glyph { glyph = "󰻠", size = 30, color = ctx.ink.text },
    extra = function(w, h)
      return ui.Item { width = w, height = h,
        common.sparkline { y = (h - 48) / 2, width = math.floor(w), height = 48,
          values = function() return stats.history("cpu") end, stroke = ctx.ink.accent } }
    end,
  })
end

-- The next hours, one column each: the hour, a glyph, the temperature.
function M.hour_columns(ctx, count, glyph_size, full_hour, gap)
  return function(w, h)
    local cells = {}
    for i = 1, count do
      local function block() return weather.hours_ahead(count)[i] end
      cells[#cells + 1] = ui.Item {
        width = w / count, height = h,
        visible = function() return block() ~= nil end,
        ui.Column {
          anchors = { center_in = true }, gap = gap or 2, align = "center",
          kit.text { mono = true, size = theme.size.label, color = ctx.ink.muted,
            text = function()
              local b = block()
              if not b then return "" end
              local hour = string.format("%02d", b.hour)
              if full_hour then return hour .. ":00" .. (b.tomorrow and "⁺" or "") end
              return b.tomorrow and (hour .. "⁺") or (hour .. "h")
            end },
          kit.glyph { size = glyph_size, color = ctx.ink.text,
            glyph = function() local b = block() return b and b.glyph or "" end },
          kit.text { mono = true, size = theme.size.small, color = ctx.ink.text,
            text = function() local b = block() return b and (b.temperature .. "°") or "" end },
        },
      }
    end
    return ui.Row { width = w, height = h, table.unpack(cells) }
  end
end

function M.weather(ctx)
  return face(ctx, {
    label = function() local p = weather.place() return p ~= "" and p or "Weather" end,
    reading = function() return weather.available() and (weather.temperature() .. "°") or "--°" end,
    note = function()
      if not weather.available() then return "no forecast" end
      local d = weather.description()
      local range = weather.high() .. "° / " .. weather.low() .. "°"
      return d ~= "" and (d .. " · " .. range) or range
    end,
    extra_share = 0.45,
    mark = glyph { glyph = function() return weather.available() and weather.glyph() or "󰅤" end, size = 34, color = ctx.ink.text },
    extra = M.hour_columns(ctx, 3, 20, false),
  })
end

function M.updates(ctx)
  local ink = ctx.ink
  local quiet = function() return S.updates.count() == 0 end
  return face(ctx, {
    label = "Updates",
    reading = function() return (S.updates.available() or S.updates.checking()) and tostring(S.updates.count()) or "—" end,
    note = squares.updates_note,
    tint = function() return quiet() and ink.muted() or ink.text() end,
    extra_share = 0.45,
    mark = glyph { glyph = "󰏖", size = 30, color = function() return quiet() and ink.muted() or ink.text() end },
    extra = function(w, h)
      local lines = {}
      for i = 1, 4 do
        lines[i] = kit.text { mono = true, width = w, horizontal_alignment = "right", elide = "left",
          size = theme.size.label, color = ink.muted,
          text = function() return S.updates.packages(4)[i] or "" end }
      end
      return ui.Item { width = w, height = h, ui.Column { anchors = { left = true, right = true, vertical_center = true }, gap = 2, table.unpack(lines) } }
    end,
  })
end

function M.bluetooth(ctx)
  local ink = ctx.ink
  return face(ctx, {
    label = "Bluetooth", reading = S.bluetooth.summary,
    note = function()
      if not S.bluetooth.available() then return "no adapter" end
      return S.bluetooth.enabled() and "adapter on" or "adapter off"
    end,
    extra_share = 0.42,
    mark = glyph { glyph = S.bluetooth.icon, size = 30, color = function() return S.bluetooth.enabled() and ink.text() or ink.muted() end },
    extra = function(w, h)
      local lines = {}
      for i = 1, 3 do
        lines[i] = kit.text { width = w, horizontal_alignment = "right", elide = "right",
          size = theme.size.label, color = ink.muted,
          text = function() local d = S.bluetooth.devices()[i] return d and d.name or "" end }
      end
      return ui.Item { width = w, height = h, ui.Column { anchors = { left = true, right = true, vertical_center = true }, gap = 3, table.unpack(lines) } }
    end,
  })
end

-- The name and the state are the whole content, so they take the full width.
function M.network(ctx) return squares.network(ctx) end

function M.timer(ctx)
  local ink = ctx.ink
  return face(ctx, {
    label = "Timer",
    reading = function() return S.timer.running() and S.timer.display() or "—" end,
    note = squares.timer_note,
    tint = function() return S.timer.running() and ink.text() or ink.muted() end,
    extra_share = 0.4,
    mark = ring_mark(ctx, S.timer.progress, function() return theme.color.indicatorTimer end),
    extra = function(w, h)
      return ui.Item { width = w, height = h,
        ui.Row {
          anchors = { right = true, vertical_center = true }, gap = 10, align = "center",
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
          common.icon_button { ink = ink, glyph = "󰅖", glyph_size = 13,
            opacity = function() return S.timer.running() and 1 or 0.35 end,
            on_click = function() if S.timer.running() then S.timer.cancel() end end },
        } }
    end,
  })
end

function M.transport(ctx, gap, sizes)
  local ink = ctx.ink
  return ui.Row {
    gap = gap or 16, align = "center",
    opacity = function() return S.media.available() and 1 or 0.4 end,
    common.icon_button { ink = ink, glyph = "󰒮", glyph_size = sizes or 16, on_click = S.media.previous },
    common.icon_button { ink = ink, glyph = function() return S.media.playing() and "󰏤" or "󰐊" end,
      glyph_size = (sizes or 16) + 2, on_click = S.media.toggle },
    common.icon_button { ink = ink, glyph = "󰒭", glyph_size = sizes or 16, on_click = S.media.next },
  }
end

function M.media(ctx)
  return face(ctx, {
    label = squares.media_label, reading = squares.media_reading, note = squares.media_note,
    extra_share = 0.4,
    mark = common.picture { size = 40, source = S.media.art, ink = ctx.ink },
    extra = function(w, h)
      return ui.Item { width = w, height = h,
        ui.Item { anchors = { right = true, vertical_center = true }, width = 122, height = 30, M.transport(ctx, 16) } }
    end,
  })
end

function M.pet(ctx)
  local pets = S.pets
  return face(ctx, {
    label = function() local n = pets.name() return n ~= "" and n or "Pet" end,
    reading = function() return pets.hatched() and ("Lv " .. pets.level()) or "Egg" end,
    note = pets.mood_line,
    extra_share = 0.4,
    mark = ring_mark(ctx, pets.progress),
    extra = function(w, h)
      local pet_face = require("pets.face")
      return ui.Item { width = w, height = h,
        ui.Item { x = (w - 56) / 2, y = (h - 56) / 2, width = 56, height = 56,
          pet_face.lively(pet_face.new { size = 56, lively = true }, 3, 10) } }
    end,
  })
end

function M.games(ctx)
  local node = squares.games(ctx)
  return node
end

function M.clock(ctx)
  return face(ctx, {
    label = function() return S.clock.format("%A") end,
    reading = function() return S.clock.format(S.clock.pattern()) end,
    note = function() return S.clock.format("%-d %B %Y") end,
    mark = glyph { glyph = "󰥔", size = 30, color = ctx.ink.text },
  })
end

-- The week around today, Monday first; a dot under a day with tasks.
function M.calendar(ctx)
  local ink = ctx.ink
  return face(ctx, {
    label = function() return S.clock.format("%B") end,
    reading = function() return tostring(S.clock.now().day) end,
    note = function()
      local left = S.tasks.pending_on(S.tasks.today_key())
      if left > 0 then return left .. " to do today" end
      return S.clock.format("%A")
    end,
    extra_share = 0.5,
    mark = glyph { glyph = "󰃭", size = 30, color = ink.text },
    extra = function(w, h)
      local days = {}
      for i = 1, 7 do
        local function date()
          local now = S.clock.now()
          local shift = morf.time.weekday(now.year, now.month, now.day) - 1
          local at = morf.time.add(morf.time.now(), { days = i - 1 - shift })
          return morf.time.date(at), (i - 1) == shift
        end
        local function marks()
          local d = date()
          return S.tasks.days_with_tasks(d.year, d.month)[d.day]
        end
        days[i] = ui.Item {
          width = w / 7, height = h,
          ui.Column {
            anchors = { center_in = true }, gap = 4, align = "center",
            kit.text { size = theme.size.label, color = ink.muted,
              text = ({ "M", "T", "W", "T", "F", "S", "S" })[i] },
            ui.Rect {
              width = 24, height = 24, radius = 12,
              color = function() local _, today = date() return today and ink.accent() or morf.color("transparent") end,
              kit.text { anchors = { center_in = true }, size = theme.size.small,
                weight = function() local _, today = date() return today and 600 or 400 end,
                color = function() local _, today = date() return today and ink.accentText() or ink.text() end,
                text = function() return tostring((date()).day) end },
              ui.Rect { anchors = { bottom = true, bottom_margin = 2 }, x = 10.5, width = 3, height = 3, radius = 1.5,
                visible = function() return marks() ~= nil end,
                color = function()
                  local _, today = date()
                  if today then return ink.accentText() end
                  return marks() == "pending" and ink.accent() or ink.muted()
                end },
            },
          },
        }
      end
      return ui.Row { width = w, height = h, table.unpack(days) }
    end,
  })
end

function M.tasks(ctx)
  local ink = ctx.ink
  return face(ctx, {
    label = "Tasks",
    reading = function() return tostring(S.tasks.pending()) end,
    note = function()
      if not S.tasks.available() then return "no board yet" end
      if S.tasks.pending() == 0 then return S.tasks.count() == 0 and "nothing yet" or "all done" end
      return S.tasks.summary()
    end,
    tint = function() return S.tasks.overdue() > 0 and ink.red() or ink.text() end,
    extra_share = 0.55,
    mark = glyph { glyph = "󰄲", size = 30, color = ink.text },
    extra = function(w, h) return require("desktop.faces.day_tasks").rows(ctx, w, 3, function(n) return S.tasks.queue(n) end) end,
  })
end

function M.notes(ctx) return require("desktop.faces.note").build(ctx) end
function M.photo(ctx) return require("desktop.faces.photo").build(ctx) end
function M.github(ctx) return require("desktop.faces.github").build(ctx) end
function M.spectrum(ctx) return require("desktop.faces.spectrum").build(ctx) end

return M
