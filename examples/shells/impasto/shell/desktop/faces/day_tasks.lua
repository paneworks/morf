-- A calendar widget's day view, and the task rows every face lists.
--
-- Port of faces/DayTasks.qml (and the TaskRows the tasks faces repeat): the
-- day, how much of it is left, and every task on it, scrolling if needed.
-- The face decides when to show it and puts the month back through
-- `on_back` or when the pointer leaves (`M.picker`).
--
--     local pick = day_tasks.picker(ctx)
--     day_tasks.view { ink = ctx.ink, region = pick.region, width = w, height = h,
--       day = pick.day, on_back = pick.clear }

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local common = require("desktop.faces.common")
local task_row = require("components.task_row")
local tasks = require("services.tasks")

local M = {}

local read = common.read

--- TaskRow's ink from a face's: the same colours under TaskRow's names, and
--- the palette's red (Theme.red) for a late day.
function M.ink_of(ink)
  return {
    text = ink.text, muted = ink.muted, accent = ink.accent,
    accent_text = ink.accentText or ink.accent_text, raised = ink.raised,
    red = function() return theme.color.red() end,
    rule = ink.dim or ink.rule,
  }
end

--- A task opened from a face: the board, on that task.
function M.open(key)
  if not key or key == "" then return end
  tasks.open(key)
  local ok, modules = pcall(require, "services.modules")
  if ok and modules.request_panel then modules.request_panel("board") end
end

--- `count` rows `width` wide; `list(n)` returns the tasks to show. Each row
--- ticks the task off, opens it on the board, strikes it through once done,
--- and writes its day (in red when late) when `options.dated`.
function M.rows(ctx, width, count, list, options)
  options = options or {}
  local region = common.region_of(ctx)
  local ink = M.ink_of(ctx.ink)
  local nodes = {}
  for i = 1, count do
    local function task() return list(count)[i] end
    local row = task_row.build {
      task = task, width = width, height = options.height or 24,
      dated = options.dated == true, ink = ink,
      on_hover = region.hover,
      on_open = function() local t = task() if t then M.open(t.key) end end,
    }
    nodes[i] = ui.Item {
      width = width, height = options.height or 24,
      visible = function() return task() ~= nil end,
      row,
    }
  end
  return ui.Column { width = width, gap = options.gap or 2, table.unpack(nodes) }
end

--- The state of a face that opens a day: `day()` is the picked day's key
--- ("" for the month), `pick(key)` opens one, `clear()` goes back; the
--- face's region closes it when the pointer leaves.
local picks = 0
function M.picker(ctx)
  picks = picks + 1
  local picked = morf.signal("impasto.desk.day." .. picks, "")
  local region = common.region_of(ctx)
  region.on_leave(function() picked:set("") end)
  local pick = {
    signal = picked,
    region = region,
    day = function() return picked:get() end,
    pick = function(key) picked:set(key or "") end,
    clear = function() picked:set("") end,
  }
  -- The latest face of a widget, for `desk day <key> <day>` on a bench
  -- without a pointer.
  if ctx.key and not ctx.card then M.by_widget[ctx.key] = pick end
  return pick
end

M.by_widget = setmetatable({}, { __mode = "v" })

--- The day view, `width` by `height`. `day()` is the day to show ("" while
--- hidden: the last one stays drawn through the fade). `titled = false`
--- where the face already shows the day (the analogue ribbon): the header is
--- then the arrow and the count.
function M.view(values)
  local ink = M.ink_of(values.ink)
  local region = values.region or common.region()
  local w, h = values.width, values.height
  local titled = values.titled ~= false
  local narrow = titled and w < 240

  -- The day last shown, kept through the fade-out: `day` is cleared at once,
  -- and an emptied list would flash "0 of 0".
  local last = ""
  local function shown()
    local d = values.day()
    if d ~= "" then last = d end
    return last
  end
  local function due() local d = shown() return d ~= "" and tasks.on(d) or {} end
  local function count()
    local list = due()
    local pending = 0
    for _, t in ipairs(list) do if t.state ~= "done" then pending = pending + 1 end end
    return pending .. " of " .. #list .. " to do"
  end

  local arrow = ui.Item {
    width = 20, height = 22,
    kit.glyph { anchors = { center_in = true }, glyph = "󰅁", size = theme.size.medium, color = ink.muted },
    common.area({ region = region }, {
      anchors = { fill = true }, cursor = "pointer",
      on_clicked = function() if values.on_back then values.on_back() end end,
    }),
  }
  local header = { arrow }
  if titled then
    header[#header + 1] = kit.text {
      width = narrow and (w - 26) or math.max(0, w - 26 - 110),
      elide = "right", size = theme.size.large, weight = 600, color = ink.text,
      text = function()
        local d = shown()
        local at = d ~= "" and tasks.date_of(d) or nil
        return at and morf.time.format("%A %-d", at) or ""
      end,
    }
  end
  if not narrow then
    header[#header + 1] = kit.text {
      width = titled and 104 or (w - 26), elide = "right",
      horizontal_alignment = titled and "right" or "left",
      size = theme.size.small, weight = titled and 400 or 600, color = ink.muted,
      text = count,
    }
  end

  local column = {
    ui.Row { gap = 6, align = "center", width = w, table.unpack(header) },
  }
  if narrow then
    column[#column + 1] = kit.text { width = w, elide = "right", size = theme.size.small,
      color = ink.muted, text = count }
  end
  column[#column + 1] = ui.Rect { width = w, height = 1, color = ink.rule }
  local head_h = 22 + (narrow and (6 + 14) or 0) + (narrow and 6 or 8) * (narrow and 2 or 1) + 1
  local list_h = math.max(24, h - head_h - (narrow and 6 or 8))

  local model = morf.list_model({})
  local list = ui.Flickable {
    width = w, height = list_h, clip = true,
    ui.Repeater {
      as = "column", gap = 4, width = w,
      model = model,
      delegate = function(row)
        local key = row.key
        return task_row.build {
          task = function() return tasks.entry(key) end,
          width = w, dated = false, ink = ink,
          on_hover = region.hover,
          on_open = function() M.open(key) end,
        }, function() end
      end,
    },
  }
  column[#column + 1] = list

  local node = ui.Column { width = w, height = h, gap = narrow and 6 or 8, table.unpack(column) }
  morf.effect("impasto.desk.day_tasks", function()
    local rows = {}
    for _, t in ipairs(due()) do rows[#rows + 1] = { key = t.key } end
    model:replace(rows, "key")
  end, { owner = node })
  return node
end

--- `when()`, kept true for a fade's length after it goes false, so what
--- fades out is still drawn while it does (QML's `visible: opacity > 0`).
local lingers = 0
function M.linger(when, owner)
  lingers = lingers + 1
  local shown = morf.signal("impasto.desk.linger." .. lingers, when())
  local serial = 0
  morf.effect("impasto.desk.linger", function()
    serial = serial + 1
    if when() then shown:set(true) return end
    local mine = serial
    morf.timer(theme.duration_fast() + 40, function()
      if mine == serial then shown:set(false) end
    end, false)
  end, owner and { owner = owner } or nil)
  return function() return shown:get() end
end

--- The day view over a face: the face fades out and the list in while a day
--- is picked. `face` is the month (or week, or square) node; `inset` the
--- margin the list keeps from the widget's edge.
function M.over(ctx, pick, face, inset, values)
  values = values or {}
  local w, h = ctx.width, ctx.height
  local open = function() return pick.day() ~= "" end
  local fade = theme.behave("fast")
  local view = M.view {
    ink = ctx.ink, region = pick.region,
    width = w - 2 * inset, height = h - 2 * inset,
    day = pick.day, on_back = pick.clear, titled = values.titled,
  }
  local node = ui.Item { width = w, height = h }
  local face_shown = M.linger(function() return not open() end, node)
  local view_shown = M.linger(open, node)
  ui.reparent(ui.Item {
    width = w, height = h,
    opacity = function() return open() and 0 or 1 end,
    visible = face_shown,
    behavior = { opacity = fade },
    face,
  }, node)
  ui.reparent(ui.Item {
    width = w, height = h,
    opacity = function() return open() and 1 or 0 end,
    visible = view_shown,
    behavior = { opacity = fade },
    -- Under the list, over the month: a press between the rows is the
    -- list's, not a day's underneath.
    common.area({ region = pick.region }, {
      anchors = { fill = true }, visible = open,
    }),
    ui.Item { x = inset, y = inset, width = w - 2 * inset, height = h - 2 * inset, view },
  }, node)
  return node
end

return M
