-- 4x4 faces, for modules with more to show.
--
-- Port of faces/Larges.qml. The same grid -- mark, label and reading stay in
-- place and the extra content goes in the middle -- except for media and
-- the calendar, whose content is the widget.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local common = require("desktop.faces.common")
local squares = require("desktop.faces.squares")
local wides = require("desktop.faces.wides")
local S = require("desktop.sources")
local weather = require("services.weather")
local stats = require("services.stats")
local claude = require("services.claude")

local face = common.widget_face
local glyph = common.glyph
local M = {}

function M.weather(ctx)
  return face(ctx, {
    label = function() local p = weather.place() return p ~= "" and p or "Weather" end,
    reading = function() return weather.available() and (weather.temperature() .. "°") or "--°" end,
    note = function()
      if not weather.available() then return "no forecast" end
      local d = weather.description()
      local rest = "feels " .. weather.feels_like() .. "° · " .. weather.high() .. "° / " .. weather.low() .. "°"
      return d ~= "" and (d .. " · " .. rest) or rest
    end,
    mark = glyph { glyph = function() return weather.available() and weather.glyph() or "󰅤" end, size = 34, color = ctx.ink.text },
    body = wides.hour_columns(ctx, 4, 26, true, 6),
  })
end

-- Three traces rather than four cards: at this size the recent history is
-- what is worth showing.
function M.stats(ctx)
  local ink = ctx.ink
  return face(ctx, {
    label = "System",
    reading = function() return string.format("%d%%", math.floor(stats.cpu() + 0.5)) end,
    -- The load and how far back the graphs go (StatsService.window).
    note = function()
      local minutes = math.floor((stats.HISTORY or 100) * (stats.POLL_MS or 3000) / 60000 + 0.5)
      return string.format("load %.2f · last %d min", (stats.load()[1] or 0), minutes)
    end,
    mark = glyph { glyph = "󰻠", size = 30, color = ink.text },
    body = function(w, h)
      local traces = {
        { "Processor", function() return string.format("%d%%", math.floor(stats.cpu() + 0.5)) end, "cpu" },
        { "Memory", function() return stats.bytes(stats.memory_used()) end, "memory" },
        { "Network", function() return stats.rate(stats.down_rate()) end, "rx" },
      }
      local th = math.floor((h - 20) / 3)
      local nodes = {}
      for i, t in ipairs(traces) do
        nodes[i] = ui.Item {
          width = w, height = th,
          kit.text { text = t[1], size = theme.size.label, color = ink.muted },
          kit.text { mono = true, anchors = { right = true }, text = t[2], size = theme.size.label, color = ink.text },
          common.sparkline { y = 14, width = math.floor(w), height = math.max(8, th - 16),
            values = function() return stats.history(t[3]) end, stroke = ink.accent },
        }
      end
      return ui.Column { gap = 10, table.unpack(nodes) }
    end,
  })
end

function M.claude(ctx)
  local ink = ctx.ink
  return face(ctx, {
    label = "Claude",
    reading = function() return claude.available() and claude.compact(claude.block_tokens()) or "—" end,
    note = function()
      if not claude.available() then return "no usage found" end
      return "this block · " .. claude.messages(claude.block_messages()) .. " · " .. claude.resets_in()
    end,
    mark = common.claude_mark { size = 34, x = 3, y = 3, color = claude.tint },
    body = function(w, h)
      local function gauge(caption, progress, fill)
        return ui.Column { gap = 7, width = w,
          kit.text { text = caption, size = theme.size.small, color = ink.muted },
          common.bar { width = w, progress = progress, fill_color = fill, track_color = ink.raised },
        }
      end
      return ui.Item { width = w, height = h, visible = claude.available,
        ui.Column {
          anchors = { left = true, right = true, vertical_center = true }, gap = 16,
          gauge(function()
            if claude.session_measured() then return "block · " .. claude.percent(claude.session_fraction()) end
            return "block · against the busiest on record"
          end, claude.gauge, claude.tint),
          gauge(function()
            if claude.weekly_measured() then return "week · " .. claude.percent(claude.weekly_fraction()) end
            return "week · " .. claude.compact(claude.week_tokens())
          end, function()
            if claude.weekly_measured() then return claude.weekly_fraction() end
            local peak = claude.peak_week_tokens()
            return peak > 0 and claude.week_tokens() / peak or 0
          end, ink.accent),
          kit.text { width = w, elide = "right", size = theme.size.small, color = ink.muted,
            text = function() return "busiest block · " .. claude.compact(claude.peak_block_tokens()) end },
        } }
    end,
  })
end

-- The month, Monday first, today in the accent and a dot under a day with
-- tasks; under it today's count and up to two of them.
function M.calendar(ctx)
  local ink = ctx.ink
  local w, h = ctx.width, ctx.height
  local day_tasks = require("desktop.faces.day_tasks")
  -- Pressing a day with tasks turns the widget into that day's list; the
  -- month comes back by the arrow or when the pointer leaves, so the widget
  -- never stays on a stale day.
  local pick = day_tasks.picker(ctx)
  local pad = 22
  local inner_w = w - 2 * pad
  local header_h = 22
  local agenda_h = S.tasks.available() and 60 or 18
  local grid_h = h - 2 * pad - header_h - 10 - 1 - 10 - agenda_h - 10
  local cell_w, cell_h = inner_w / 7, grid_h / 7

  local function cells()
    local now = S.clock.now()
    return morf.time.month(now.year, now.month), now
  end
  local nodes = {}
  for i, letter in ipairs { "M", "T", "W", "T", "F", "S", "S" } do
    nodes[#nodes + 1] = kit.text {
      x = (i - 1) * cell_w, y = 0, width = cell_w, height = cell_h,
      horizontal_alignment = "center", vertical_alignment = "center",
      text = letter, size = theme.size.label, weight = 600, color = ink.muted,
    }
  end
  for r = 1, 6 do
    for c = 1, 7 do
      local function cell()
        local weeks, now = cells()
        local row = weeks[r]
        local day = row and row[c]
        if not day or not day.current then return nil, now end
        return day, now
      end
      local function is_today()
        local day, now = cell()
        return day ~= nil and day.day == now.day
      end
      local function mark()
        local day, now = cell()
        if not day then return nil end
        return S.tasks.days_with_tasks(now.year, now.month)[day.day]
      end
      local function key()
        local day, now = cell()
        return day and S.tasks.day_key(now.year, now.month, day.day) or ""
      end
      local function picked() return not is_today() and key() ~= "" and pick.day() == key() end
      local d = math.min(cell_w, cell_h) - 6
      nodes[#nodes + 1] = ui.Item {
        x = (c - 1) * cell_w, y = r * cell_h, width = cell_w, height = cell_h,
        visible = function() return (cell()) ~= nil end,
        ui.Rect { x = (cell_w - d) / 2, y = (cell_h - d) / 2, width = d, height = d, radius = d / 2,
          visible = function() return is_today() or picked() end,
          color = function() return is_today() and ink.accent() or morf.color("transparent") end,
          border_color = ink.accent, border_width = function() return picked() and 1 or 0 end },
        kit.text { anchors = { fill = true }, horizontal_alignment = "center", vertical_alignment = "center",
          size = theme.size.small,
          weight = function() return is_today() and 600 or 400 end,
          color = function() return is_today() and ink.accentText() or ink.text() end,
          text = function() local day = cell() return day and tostring(day.day) or "" end },
        ui.Rect { x = cell_w / 2 - 1.5, y = cell_h - 7, width = 3, height = 3, radius = 1.5,
          visible = function() return mark() ~= nil end,
          color = function()
            if is_today() then return ink.accentText() end
            return mark() == "pending" and ink.accent() or ink.muted()
          end },
        common.area(ctx, { anchors = { fill = true }, cursor = "pointer",
          visible = function() return mark() ~= nil end,
          on_clicked = function() pick.pick(key()) end }),
      }
    end
  end
  local agenda = {
    kit.text { width = inner_w, elide = "right", size = theme.size.label, weight = 600, color = ink.muted,
      text = function()
        if not S.tasks.available() then return "Today · " .. S.clock.format("%A") end
        local due = S.tasks.on(S.tasks.today_key())
        local left = 0
        for _, t in ipairs(due) do if t.state ~= "done" and not t.done then left = left + 1 end end
        if #due == 0 then return "Today · nothing due" end
        return string.format("Today · %d of %d to do", left, #due)
      end },
  }
  if S.tasks.available() then
    agenda[#agenda + 1] = day_tasks.rows(ctx, inner_w, 2,
      function(n) local out = {} for i, t in ipairs(S.tasks.on(S.tasks.today_key())) do if i <= n then out[i] = t end end return out end)
  end
  return day_tasks.over(ctx, pick, ui.Item { width = w, height = h,
    kit.text { x = pad, y = pad, text = function() return S.clock.format("%B") end,
      size = theme.size.large, weight = 600, color = ink.text },
    kit.text { x = pad, y = pad + 4, width = inner_w, horizontal_alignment = "right",
      text = function() return S.clock.format("%Y") end, size = theme.size.small, color = ink.muted },
    ui.Item { x = pad, y = pad + header_h + 10, width = inner_w, height = grid_h, table.unpack(nodes) },
    ui.Rect { x = pad, y = pad + header_h + 10 + grid_h + 10, width = inner_w, height = 1, color = ink.dim },
    ui.Column { x = pad, y = pad + header_h + 10 + grid_h + 21, width = inner_w, gap = 3, table.unpack(agenda) },
  }, pad)
end

-- Album art at a useful size, with the transport under it.
function M.media(ctx)
  local ink = ctx.ink
  local w, h = ctx.width, ctx.height
  local info_h = 18 + 14 + 8 + 30
  local sleeve = math.min(w - 44, h - 22 - 14 - 18 - info_h)
  return ui.Item { width = w, height = h,
    common.picture { x = (w - sleeve) / 2, y = 22, size = sleeve, source = S.media.art, ink = ink, glyph_size = 54 },
    ui.Column { x = 22, y = 22 + sleeve + 14, width = w - 44, gap = 2,
      kit.text { width = w - 44, elide = "right", text = squares.media_reading,
        size = theme.size.medium, weight = 600, color = ink.text },
      kit.text { width = w - 44, elide = "right",
        text = function() return S.media.available() and S.media.artist() or "no player" end,
        size = theme.size.small, color = ink.muted },
    },
    ui.Item { x = (w - 126) / 2, y = h - 18 - 30, width = 126, height = 30, wides.transport(ctx, 18) },
  }
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
    body = function(w) return require("desktop.faces.day_tasks").rows(ctx, w, 6, function(n) return S.tasks.queue(n) end, { dated = true }) end,
  })
end

function M.notes(ctx) return require("desktop.faces.note").build(ctx) end
function M.photo(ctx) return require("desktop.faces.photo").build(ctx) end
function M.spectrum(ctx) return require("desktop.faces.spectrum").build(ctx) end

return M
