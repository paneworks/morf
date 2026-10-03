-- A date picker and a date range picker (composite: Press + Popup +
-- Selection day grid + Navigation months).
--
--     local node, picker = composites.date_picker {
--       id = "due", width = 200,
--       value = function() return due:get() end,       -- "YYYY-MM-DD" or ""
--       on_changed = function(date) due:set(date) end,
--     }
--     composites.date_picker { id = "trip", range = true,
--       from = "2026-10-03", to = "2026-10-09",
--       on_changed = function(from, to) end }
--
-- A press shows the date (`format`, strftime's, "%d %b %Y"; `placeholder`
-- while there is none) and opens a popover of the month around it -- a
-- kit calendar (lib.kit.composites.calendar): the arrows walk the days
-- and across months, Page Up and Page Down turn the month, Home and End
-- go to its ends, Return or a press picks and closes. A range picker
-- takes two picks, the first its start, and draws the run between.
-- `inline = true` gives the calendar alone, picking in place; `compact =
-- true` makes the press an icon (beside a field the date is also typed
-- in). Ids: `<id>-field` (the press), `<id>-popup`, and the calendar's
-- under `<id>` (`<id>-day-<date>`, `<id>-next`, ...).
local ui = require("morf.ui")
local widgets = require("lib.kit.widgets")
local calendar = require("lib.kit.composites.calendar")

local function get(v) if type(v) == "function" then return v() end return v end
local function date_or_empty(v) v = get(v) return calendar.parse(v) and v or "" end

return function(spec)
  spec = spec or {}
  local kit = require("kit")
  local id = spec.id
  local st = morf.state { value = date_or_empty(spec.value), from = date_or_empty(spec.from),
    to = date_or_empty(spec.to), cursor = "" }
  st.cursor = spec.range and (st.from ~= "" and st.from or calendar.today()) or (st.value ~= "" and st.value or calendar.today())
  local popup
  local function finish()
    if spec.range then
      if spec.on_changed then spec.on_changed(st.from, st.to) end
    elseif spec.on_changed then spec.on_changed(st.value) end
    if popup then popup.close("activated") end
  end
  local function picked(date)
    if not spec.range then
      st.value = date
      finish()
      return
    end
    if st.from == "" or st.to ~= "" then
      st.from, st.to = date, ""
      if spec.on_started then spec.on_started(date) end
    else
      local from, to = st.from, date
      if to < from then from, to = to, from end
      st.from, st.to = from, to
      finish()
    end
  end
  local W = spec.calendar_width or 280
  local function caption()
    if spec.range then
      if st.from == "" then return spec.placeholder or "Pick dates" end
      local from = calendar.format(st.from, spec.format)
      if st.to == "" then return from .. " – …" end
      return from .. " – " .. calendar.format(st.to, spec.format)
    end
    return st.value ~= "" and calendar.format(st.value, spec.format) or (spec.placeholder or "Pick a date")
  end
  local cal, panel
  -- The calendar is built when first wanted: until a popover holds it, it
  -- would hang from nothing.
  local function build()
    if panel then return end
    local cal_node
    cal_node, cal = calendar.make {
      id = id, width = W, cell_height = spec.cell_height, first_weekday = spec.first_weekday,
      press_activates = true, marked = spec.marked,
      value = function() return st.cursor end,
      on_changed = function(date) st.cursor = date end,
      on_picked = picked,
      band = spec.range and function()
        return st.from, st.to ~= "" and st.to or (st.from ~= "" and st.cursor or "")
      end or nil,
    }
    -- Under the days: what is chosen, and a way back to today.
    local foot = ui.Item { y = function() return cal.height_now() + 8 end, width = W, height = 32,
      kit.subtitle { anchors = { left = true, vertical_center = true }, width = W - 96, elide = "right",
        text = function()
          if spec.range then
            if st.from ~= "" and st.to == "" then return "Now the last day" end
            return st.from == "" and "Pick the first day" or caption()
          end
          return calendar.format(st.cursor, "%A")
        end },
      widgets.push { id = id and (id .. "-today"), label = "Today", width = 88, height = 30,
        anchors = { right = true, vertical_center = true },
        on_clicked = function() cal.select(calendar.today()) cal.focus() end },
    }
    -- (Over the popover's ground, which its skin lays after its content.)
    panel = ui.Item { z = 1, width = W, height = function() return cal.height_now() + 40 end, cal_node, foot }
  end
  local handle = {}
  function handle.value() return st.value end
  function handle.range() return st.from, st.to end
  if spec.inline then
    build()
    for _, k in ipairs { "x", "y", "anchors" } do if spec[k] ~= nil then panel[k] = spec[k] end end
    handle.calendar = cal
    return panel, handle
  end

  local field
  local function open()
    build()
    if not popup then
      popup = widgets.popover { id = id and (id .. "-popup"), content = panel, padding = 12,
        placement = spec.placement or "bottom-start", root = spec.root,
        on_opened = function() cal.focus() end, on_closed = spec.on_closed }
      handle.popup, handle.calendar = popup, cal
    end
    if spec.range then st.cursor = st.from ~= "" and st.from or (st.cursor ~= "" and st.cursor or calendar.today())
    else st.cursor = st.value ~= "" and st.value or calendar.today() end
    local y, m = calendar.parse(st.cursor)
    if y then cal.show(calendar.month_index(y, m)) end
    popup.toggle(field)
    cal.focus()
  end
  if spec.compact then
    field = widgets.icon { id = id and (id .. "-field"), accessible_name = spec.accessible_name or "Choose a date",
      width = spec.width or 36, height = spec.height or 36, size = 20, icon_off = spec.icon or "calendar_month",
      x = spec.x, y = spec.y, anchors = spec.anchors, on_clicked = open }
  else
    field = widgets.push { id = id and (id .. "-field"), label = caption, icon = spec.icon or "calendar_month",
      accessible_name = spec.accessible_name or (spec.range and "Dates" or "Date"),
      width = spec.width or 200, height = spec.height or 36, x = spec.x, y = spec.y, anchors = spec.anchors,
      on_clicked = open }
  end
  -- The configuration's dates, when it keeps them.
  local function follow(name, source)
    if type(source) ~= "function" then return end
    morf.effect("kit.date_picker." .. name .. "." .. tostring(field), function()
      local v = date_or_empty(source())
      if v ~= st[name] then st[name] = v end
    end, { owner = field })
  end
  follow("value", spec.value) follow("from", spec.from) follow("to", spec.to)
  function handle.open() if not (popup and popup.is_open()) then open() end end
  function handle.close() if popup then popup.close("closed") end end
  function handle.is_open() return popup ~= nil and popup.is_open() end
  return field, handle
end
