-- The calendar as paper.
--
-- Port of analogue/CalendarFace.qml. 2x2: one leaf, with rings, the month on
-- a ribbon, the day and the weekday. 4x2: that leaf, and the week beside it
-- as seven small leaves (today in the accent, a dot on days with tasks) with
-- a summary line under them. 4x4: the month ruled on one sheet with today's
-- tasks under it. Task dots come from `sources.tasks.days_with_tasks`.
--
-- Pressing a day with tasks (or the 2x2 leaf, which is today) shows that
-- day's sheet: the 4x4 rewrites its own ribbon, the smaller faces lay a sheet
-- over themselves. The face comes back by the arrow or when the pointer
-- leaves.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local instrument = require("desktop.faces.analogue.instrument")
local leaf = require("desktop.faces.analogue.leaf")
local day_tasks = require("desktop.faces.day_tasks")
local common = require("desktop.faces.common")
local S = require("desktop.sources")

local C = theme.color
local M = {}

local LETTERS = { "M", "T", "W", "T", "F", "S", "S" }

local function today() return S.clock.now() end
local function month_name() return S.clock.format("%B"):upper() end

-- The day `i` (1 = Monday) of this week, and whether it is today.
local function week_day(i)
  local now = today()
  local shift = morf.time.weekday(now.year, now.month, now.day) - 1
  local at = morf.time.add(morf.time.now(), { days = i - 1 - shift })
  return morf.time.date(at), (i - 1) == shift
end

local function marks_of(d)
  return S.tasks.days_with_tasks(d.year, d.month)[d.day]
end

-- The summary line under the week: today's count, else the next task due,
-- else the weekday.
local function agenda_line()
  local left = S.tasks.pending_on(S.tasks.today_key())
  if left > 0 then return left .. " to do today" end
  local next = S.tasks.next()
  if next then return next.text .. " · " .. S.tasks.due_label(next.due) end
  return S.clock.format("%A")
end

-- The notes' ink, for what is written on the paper.
local function paper_ink_of(ink)
  return {
    text = function() return C.paperInk end, muted = function() return C.paperInkMuted end,
    accent = ink.accent, accentText = ink.accentText,
    raised = function() return C.paperInk:alpha(0.08) end,
    red = function() return C.red() end,
    rule = function() return C.paperInkMuted:alpha(0.4) end,
    dim = function() return C.paperInkMuted:alpha(0.4) end,
    border = ink.border, ground = ink.ground,
  }
end

-- The picked day's name for the ribbon, kept through the fade-out.
local function ribbon_of(pick)
  local last = ""
  return function()
    local d = pick.day()
    if d ~= "" then
      local at = S.tasks.service() and S.tasks.service().date_of(d)
      if at then last = morf.time.format("%A %-d", at):upper() end
    end
    return last
  end
end

local function square(ctx, pick)
  return instrument.build(ctx, {
    object = function(w, h)
      local lw = math.min(w, h * 0.74)
      return leaf.build {
        x = (w - lw) / 2, y = 0, width = lw, height = h, ink = ctx.ink,
        month = month_name, day = function() return tostring(today().day) end,
        weekday = function() return S.clock.format("%A") end,
        common.area(ctx, {
          anchors = { fill = true }, cursor = "pointer",
          visible = function() return S.tasks.count_on(S.tasks.today_key()) > 0 end,
          on_clicked = function() pick.pick(S.tasks.today_key()) end,
        }),
      }
    end,
  })
end

local function week(ctx, pick)
  local w, h, ink = ctx.width, ctx.height, ctx.ink
  local days_w = w - 146 - 22
  local cell_w = (days_w - 36) / 7
  local cells = {}
  for i = 1, 7 do
    cells[i] = ui.Item {
      x = (i - 1) * (cell_w + 6), y = 0, width = cell_w, height = 66,
      leaf.build {
        x = 0, y = 0, width = cell_w, height = 66, ink = ink, rings = false, band = 16,
        today = function() local _, is_today = week_day(i) return is_today end,
        month = LETTERS[i], day = function() return tostring((week_day(i)).day) end,
      },
      ui.Rect { x = cell_w / 2 - 2, y = 66 - 9, width = 4, height = 4, radius = 2,
        visible = function() return marks_of((week_day(i))) ~= nil end,
        color = function() return marks_of((week_day(i))) == "pending" and ink.accent() or C.paperInkMuted end },
      common.area(ctx, {
        anchors = { fill = true }, cursor = "pointer",
        visible = function() return marks_of((week_day(i))) ~= nil end,
        on_clicked = function()
          local d = week_day(i)
          pick.pick(S.tasks.day_key(d.year, d.month, d.day))
        end,
      }),
    }
  end
  return ui.Item {
    width = w, height = h,
    leaf.build {
      x = 22, y = 22, width = 100, height = h - 44, ink = ink,
      month = month_name, day = function() return tostring(today().day) end,
      weekday = function() return S.clock.format("%A") end,
    },
    ui.Item { x = 146, y = 40, width = days_w, height = 66, table.unpack(cells) },
    kit.text { x = 146, y = 122, width = days_w, elide = "right", text = agenda_line,
      size = theme.size.regular, color = ink.muted },
  }
end

local function month(ctx, pick)
  local w, h, ink = ctx.width, ctx.height, ctx.ink
  local margin, band = 22, 34
  local sw, sh = w - 2 * margin, h - 2 * margin
  local inner_w = sw - 28
  local agenda_rows = S.tasks.available() and 2 or 0
  local agenda_h = 15 + agenda_rows * 26
  local top = band + 10
  local grid_h = sh - top - 14 - agenda_h - 8 - 9
  local cell_w = inner_w / 7

  local function weeks()
    local now = today()
    return morf.time.month(now.year, now.month), now
  end
  local function rows_count() return #(weeks()) end
  local function cell_h() return grid_h / (1 + rows_count()) end

  local nodes = {}
  for i, letter in ipairs(LETTERS) do
    nodes[#nodes + 1] = kit.text {
      x = (i - 1) * cell_w, y = 0, width = cell_w, height = function() return cell_h() end,
      horizontal_alignment = "center", vertical_alignment = "center",
      text = letter, size = theme.size.label, weight = 600, color = C.paperInkMuted,
    }
  end
  for r = 1, 6 do
    for c = 1, 7 do
      local function cell()
        local list, now = weeks()
        local row = list[r]
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
      nodes[#nodes + 1] = ui.Item {
        x = (c - 1) * cell_w,
        y = function() return r * cell_h() end,
        width = cell_w, height = function() return cell_h() end,
        visible = function() return (cell()) ~= nil end,
        ui.Rect {
          anchors = { center_in = true },
          width = function() return math.min(cell_w, cell_h()) - 6 end,
          height = function() return math.min(cell_w, cell_h()) - 6 end,
          radius = function() return (math.min(cell_w, cell_h()) - 6) / 2 end,
          visible = is_today, color = ink.accent,
        },
        kit.text { anchors = { fill = true }, horizontal_alignment = "center", vertical_alignment = "center",
          size = theme.size.small,
          weight = function() return is_today() and 600 or 400 end,
          color = function() return is_today() and ink.accentText() or C.paperInk end,
          text = function() local day = cell() return day and tostring(day.day) or "" end },
        ui.Rect { x = cell_w / 2 - 1.5, y = function() return cell_h() - 6 end, width = 3, height = 3, radius = 1.5,
          visible = function() return mark() ~= nil end,
          color = function()
            if is_today() then return ink.accentText() end
            return mark() == "pending" and ink.accent() or C.paperInkMuted
          end },
        common.area(ctx, {
          anchors = { fill = true }, cursor = "pointer",
          visible = function() return mark() ~= nil end,
          on_clicked = function()
            local day, now = cell()
            if day then pick.pick(S.tasks.day_key(now.year, now.month, day.day)) end
          end,
        }),
      }
    end
  end

  -- What is due today, ruled under the month, in the notes' ink.
  local paper_ink = paper_ink_of(ink)
  local agenda = {
    kit.text { width = inner_w, elide = "right", size = theme.size.label, weight = 600, color = C.paperInkMuted,
      text = function()
        if not S.tasks.available() then return "Today · " .. S.clock.format("%A %-d") end
        local due = S.tasks.on(S.tasks.today_key())
        if #due == 0 then return "Today · nothing due" end
        local left = 0
        for _, t in ipairs(due) do if t.state ~= "done" and not t.done then left = left + 1 end end
        return string.format("Today · %d of %d to do", left, #due)
      end },
  }
  if agenda_rows > 0 then
    agenda[#agenda + 1] = day_tasks.rows({ ink = paper_ink, region = ctx.region }, inner_w, agenda_rows, function(count)
      local out = {}
      for i, t in ipairs(S.tasks.on(S.tasks.today_key())) do if i <= count then out[i] = t end end
      return out
    end)
  end

  local open = function() return pick.day() ~= "" end
  local ribbon = ribbon_of(pick)
  local node = ui.Item { width = w, height = h }
  local month_shown = day_tasks.linger(function() return not open() end, node)
  local day_shown = day_tasks.linger(open, node)
  local fade = theme.behave("fast")
  ui.reparent(leaf.build {
    x = margin, y = margin, width = sw, height = sh, ink = ink, band = band,
    month = function()
      if open() then return ribbon() end
      return S.clock.format("%B %Y"):upper()
    end,
    ui.Item {
      width = sw, height = sh,
      opacity = function() return open() and 0 or 1 end, visible = month_shown,
      behavior = { opacity = fade },
      ui.Item { x = 14, y = top, width = inner_w, height = grid_h, table.unpack(nodes) },
      ui.Rect { x = 14, y = top + grid_h + 8, width = inner_w, height = 1, color = C.paperInkMuted, opacity = 0.4 },
      ui.Column { x = 14, y = top + grid_h + 17, width = inner_w, gap = 3, table.unpack(agenda) },
    },
    -- The day, written where the month was; the ribbon names it.
    ui.Item {
      x = 14, y = top, width = inner_w, height = sh - top - 14,
      opacity = function() return open() and 1 or 0 end, visible = day_shown,
      behavior = { opacity = fade },
      day_tasks.view {
        ink = paper_ink, region = pick.region, titled = false,
        width = inner_w, height = sh - top - 14,
        day = pick.day, on_back = pick.clear,
      },
    },
  }, node)
  return node
end

-- The day sheet for the 2x2 and 4x2, laid over the face.
local function day_sheet(ctx, pick, face)
  local w, h = ctx.width, ctx.height
  local inset = ctx.family == "2x2" and 14 or 16
  local band = 26
  local sw, sh = w - 2 * inset, h - 2 * inset
  local open = function() return pick.day() ~= "" end
  local fade = theme.behave("fast")
  local node = ui.Item { width = w, height = h }
  local face_shown = day_tasks.linger(function() return not open() end, node)
  local sheet_shown = day_tasks.linger(open, node)
  ui.reparent(ui.Item {
    width = w, height = h,
    opacity = function() return open() and 0 or 1 end, visible = face_shown,
    behavior = { opacity = fade },
    face,
  }, node)
  ui.reparent(ui.Item {
    width = w, height = h,
    opacity = function() return open() and 1 or 0 end, visible = sheet_shown,
    behavior = { opacity = fade },
    common.area(ctx, { anchors = { fill = true }, visible = open }),
    leaf.build {
      x = inset, y = inset, width = sw, height = sh, ink = ctx.ink, band = band,
      month = ribbon_of(pick),
      ui.Item {
        x = 10, y = band + 8, width = sw - 20, height = sh - band - 18,
        day_tasks.view {
          ink = paper_ink_of(ctx.ink), region = pick.region, titled = false,
          width = sw - 20, height = sh - band - 18,
          day = pick.day, on_back = pick.clear,
        },
      },
    },
  }, node)
  return node
end

function M.build(ctx)
  local pick = day_tasks.picker(ctx)
  if ctx.family == "2x2" then return day_sheet(ctx, pick, square(ctx, pick)) end
  if ctx.family == "4x4" then return month(ctx, pick) end
  return day_sheet(ctx, pick, week(ctx, pick))
end

return M
