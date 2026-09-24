-- 2x2 faces: the module's mark, its main value and a caption.
--
-- Port of faces/Squares.qml. Every face shows something when there is no
-- data: a widget is always drawn, and says what is missing rather than going
-- blank.

local ui = require("morf.ui")
local theme = require("theme")
local common = require("desktop.faces.common")
local S = require("desktop.sources")
local weather = require("services.weather")
local stats = require("services.stats")
local claude = require("services.claude")

local face = common.widget_face
local glyph = common.glyph
local M = {}

local function ring_mark(ctx, progress, fill, inner)
  local ink = ctx.ink
  return common.ring {
    size = 40, thickness = 3, progress = progress,
    track_color = ink.dim, fill_color = fill or ink.text,
    inner,
  }
end

local function centred_glyph(ctx, g, size, color)
  return glyph { glyph = g, size = size or 16, color = color or ctx.ink.text, anchors = { center_in = true }, box = 40 }
end

function M.battery(ctx)
  return face(ctx, {
    label = "Battery",
    reading = function() return S.battery.available() and (S.battery.percent() .. "%") or "—" end,
    note = function() return S.battery.available() and S.battery.estimate() or "no battery" end,
    mark = glyph { glyph = S.battery.icon, size = 30, color = ctx.ink.text },
  })
end

function M.volume(ctx)
  local ink = ctx.ink
  return face(ctx, {
    label = "Volume",
    reading = function() return S.audio.muted() and "Muted" or (S.audio.volume() .. "%") end,
    tint = function() return S.audio.muted() and ink.muted() or ink.text() end,
    mark = ring_mark(ctx, function() return S.audio.muted() and 0 or S.audio.volume() / 100 end, nil,
      centred_glyph(ctx, S.audio.icon, 16, function() return S.audio.muted() and ink.muted() or ink.text() end)),
  })
end

function M.brightness(ctx)
  return face(ctx, {
    label = "Brightness",
    reading = function() return S.brightness.available() and (S.brightness.percent() .. "%") or "—" end,
    note = function() return S.brightness.available() and "" or "no backlight" end,
    mark = ring_mark(ctx, function() return S.brightness.percent() / 100 end, nil,
      centred_glyph(ctx, S.brightness.icon, 16)),
  })
end

function M.network(ctx)
  local ink = ctx.ink
  return face(ctx, {
    label = "Network", reading = S.network.name, note = S.network.state_line,
    mark = glyph { glyph = S.network.icon, size = 30,
      color = function() return S.network.online() and ink.text() or ink.muted() end },
  })
end

function M.bluetooth(ctx)
  local ink = ctx.ink
  return face(ctx, {
    label = "Bluetooth", reading = S.bluetooth.summary,
    mark = glyph { glyph = S.bluetooth.icon, size = 30,
      color = function() return S.bluetooth.enabled() and ink.text() or ink.muted() end },
  })
end

function M.updates_note()
  if S.updates.checking() then return "checking" end
  if not S.updates.available() then return "cannot check" end
  return S.updates.count() == 0 and "up to date" or "pending"
end

function M.updates(ctx)
  local ink = ctx.ink
  local quiet = function() return S.updates.count() == 0 end
  return face(ctx, {
    label = "Updates",
    reading = function()
      return (S.updates.available() or S.updates.checking()) and tostring(S.updates.count()) or "—"
    end,
    note = M.updates_note,
    tint = function() return quiet() and ink.muted() or ink.text() end,
    mark = glyph { glyph = "󰏖", size = 30, color = function() return quiet() and ink.muted() or ink.text() end },
  })
end

function M.weather(ctx)
  return face(ctx, {
    label = function() local p = weather.place() return p ~= "" and p or "Weather" end,
    reading = function() return weather.available() and (weather.temperature() .. "°") or "--°" end,
    note = function() return weather.available() and weather.description() or "no forecast" end,
    mark = glyph { glyph = function() return weather.available() and weather.glyph() or "󰅤" end,
      size = 34, color = ctx.ink.text },
  })
end

function M.stats(ctx)
  return face(ctx, {
    label = "System",
    reading = function() return string.format("%d%%", math.floor(stats.cpu() + 0.5)) end,
    note = function() return string.format("RAM %d%%", math.floor(stats.memory_fraction() * 100 + 0.5)) end,
    mark = glyph { glyph = "󰻠", size = 30, color = ctx.ink.text },
  })
end

function M.claude_reading()
  if not claude.available() then return "—" end
  if claude.session_measured() then return claude.percent(claude.session_fraction()) end
  return claude.compact(claude.block_tokens())
end

function M.claude(ctx)
  return face(ctx, {
    label = "Claude",
    reading = M.claude_reading,
    note = function()
      if not claude.available() then return "no usage found" end
      return claude.session_measured() and "of this block" or "this block"
    end,
    mark = common.claude_mark { size = 32, x = 4, y = 4, color = ctx.ink.text },
  })
end

function M.timer_note()
  if not S.timer.running() then return "nothing running" end
  local label = S.timer.label()
  return label ~= "" and label or "counting down"
end

function M.timer(ctx)
  local ink = ctx.ink
  return face(ctx, {
    label = "Timer",
    reading = function() return S.timer.running() and S.timer.display() or "—" end,
    note = M.timer_note,
    tint = function() return S.timer.running() and ink.text() or ink.muted() end,
    mark = ring_mark(ctx, S.timer.progress, function() return theme.color.indicatorTimer end),
  })
end

function M.games_last()
  local id = S.games.last_played and S.games.last_played() or nil
  if type(id) == "table" then return id end
  return id and S.games.entry(id) or nil
end

function M.games(ctx)
  return face(ctx, {
    label = "Games",
    reading = function() local last = M.games_last() return last and tostring(S.games.best_of(last.id)) or "—" end,
    note = function() local last = M.games_last() return last and (last.name .. " · best") or "Nothing played yet" end,
    mark = ring_mark(ctx, 0, nil, centred_glyph(ctx, "󰊗", 18)),
  })
end

function M.pet_mark(size)
  local pet_face = require("pets.face")
  return pet_face.new { size = size, lively = true }
end

function M.pet(ctx)
  local pets = S.pets
  return face(ctx, {
    label = function() local n = pets.name() return n ~= "" and n or "Pet" end,
    reading = function() return pets.hatched() and ("Lv " .. pets.level()) or "Egg" end,
    note = pets.mood,
    mark = ring_mark(ctx, pets.progress, nil,
      ui.Item { x = 7, y = 7, width = 26, height = 26, M.pet_mark(26) }),
  })
end

function M.media_label()
  local artist = S.media.artist()
  return artist ~= "" and artist or "Media"
end
function M.media_reading()
  local title = S.media.title()
  if title ~= "" then return title end
  return S.media.available() and S.media.identity() or "Nothing playing"
end
function M.media_note()
  if not S.media.available() then return "no player" end
  return S.media.playing() and "playing" or "paused"
end

function M.media(ctx)
  return face(ctx, {
    label = M.media_label, reading = M.media_reading, note = M.media_note,
    mark = common.picture { size = 40, source = S.media.art, ink = ctx.ink },
  })
end

function M.clock(ctx)
  return face(ctx, {
    label = function() return S.clock.format("%a") end,
    reading = function() return S.clock.format(S.clock.pattern()) end,
    note = function() return S.clock.format("%-d %B") end,
    mark = glyph { glyph = "󰥔", size = 30, color = ctx.ink.text },
  })
end

-- The date; the month is the 4x4 face. The square is today, so pressing it
-- with tasks shows today's list, as a pressed day does on the larger faces;
-- the square comes back by the arrow or when the pointer leaves.
function M.calendar(ctx)
  local day_tasks = require("desktop.faces.day_tasks")
  local pick = day_tasks.picker(ctx)
  local has = function() return S.tasks.count_on(S.tasks.today_key()) > 0 end
  local square = face(ctx, {
    label = function() return S.clock.format("%B") end,
    reading = function() return tostring(S.clock.now().day) end,
    note = function()
      local weekday = S.clock.format("%A")
      local left = S.tasks.pending_on(S.tasks.today_key())
      return left > 0 and (weekday .. " · " .. left .. " to do") or weekday
    end,
    mark = glyph { glyph = "󰃭", size = 30, color = ctx.ink.text },
  })
  local node = ui.Item {
    width = ctx.width, height = ctx.height,
    square,
    common.area(ctx, {
      anchors = { fill = true }, cursor = "pointer", visible = has,
      on_clicked = function() pick.pick(S.tasks.today_key()) end,
    }),
  }
  return day_tasks.over(ctx, pick, node, 16)
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
    mark = glyph { glyph = "󰄲", size = 30, color = ink.text },
  })
end

function M.notes(ctx) return require("desktop.faces.note").build(ctx) end
function M.photo(ctx) return require("desktop.faces.photo").build(ctx) end
function M.github(ctx) return require("desktop.faces.github").build(ctx) end

return M
