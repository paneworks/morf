-- A time picker (composite: Range spins + Popup).
--
--     local node, picker = composites.time_picker {
--       id = "alarm", value = function() return alarm:get() end,  -- "HH:MM" (24 h)
--       on_changed = function(time) alarm:set(time) end,
--       seconds = false, twelve_hour = false, minute_step = 5,
--     }
--
-- Each field -- hours, minutes, seconds -- is a spin: a kit Range that
-- wraps round (23 goes on to 00), the reading between a step up and a
-- step down. With focus the arrows step it, Page Up and Page Down by a
-- larger step, Home and End go to its ends, the wheel turns it, and
-- typing two digits sets it; Tab goes to the next field. `twelve_hour`
-- reads the hours 1 to 12 beside an AM/PM choice (a kit segmented
-- Selection); the value is always "HH:MM[:SS]", 24 h. The press shows the
-- time and opens the spins in a popover; `inline = true` gives the spins
-- alone. Ids: `<id>-field`, `<id>-popup`, `<id>-hours` (`-minutes`,
-- `-seconds`; each with `-up` and `-down`), `<id>-meridiem`.
local ui = require("morf.ui")
local control = require("lib.kit.control")
local widgets = require("lib.kit.widgets")

local function get(v) if type(v) == "function" then return v() end return v end

local function parse(text)
  if type(text) ~= "string" then return nil end
  local h, m, s = text:match("^%s*(%d%d?):(%d%d):?(%d?%d?)")
  h, m, s = tonumber(h), tonumber(m), tonumber(s) or 0
  if not h or h > 23 or m > 59 or s > 59 then return nil end
  return h, m, s
end

return function(spec)
  spec = spec or {}
  local kit = require("kit")
  local id = spec.id
  local twelve = spec.twelve_hour == true
  local h0, m0, s0 = parse(get(spec.value))
  local st = morf.state { h = h0 or 0, m = m0 or 0, s = s0 or 0 }
  local function text()
    if spec.seconds then return ("%02d:%02d:%02d"):format(st.h, st.m, st.s) end
    return ("%02d:%02d"):format(st.h, st.m)
  end
  local function caption()
    if not twelve then return text() end
    local h = st.h % 12
    local out = ("%d:%02d"):format(h == 0 and 12 or h, st.m)
    if spec.seconds then out = out .. (":%02d"):format(st.s) end
    return out .. (st.h >= 12 and " PM" or " AM")
  end
  local function changed() if spec.on_changed then spec.on_changed(text()) end end

  local CELL_W, READ_H, STEP_H = spec.cell_width or 64, 56, 30
  local spins = {}
  -- One spin: `from`..`to` wrapping, shown by `show`, kept by `read`/`write`.
  local function spin(name, from, to, step, page, read, write, show)
    local area, range
    local typed, typed_at = "", -1
    local clock = morf.elapsed_timer()
    area = ui.MouseArea { id = id and (id .. "-" .. name), width = CELL_W, height = READ_H, cursor = "ns_resize",
      accessible_role = "spin_button", accessible_name = name:gsub("^%l", string.upper),
      on_key_pressed = function(_, chars, modifiers, _, key)
        if chars and chars:match("^%d$") then
          local now = clock:elapsed_ms()
          typed = (now - typed_at < 1200 and #typed < 2) and (typed .. chars) or chars
          typed_at = now
          local v = tonumber(typed)
          if v and v >= from and v < to then range.send("set", v) write(v) changed() end
          return true
        end
        return range.key(key or "", modifiers or "", chars or "")
      end,
      on_wheel = function(_, _, _, _, step_x, step_y) range.send("wheel", step_x or 0, step_y or 0) end,
      kit.centred(CELL_W, READ_H, kit.readout { value = show, size = spec.size or 32 }) }
    range = control.headless("Range", { from = from, to = to, step = step, snap = "always", wrap = true,
      page_step = page, value = read, owner = area,
      on_moved = function(v) write(math.floor(v + 0.5)) changed() end })
    kit.focusable(area)
    -- A press puts the keys here, as Tab does.
    area.focus_policy = "strong"
    local function stepper(dir)
      return widgets.icon { id = id and (id .. "-" .. name .. (dir > 0 and "-up" or "-down")),
        accessible_name = (dir > 0 and "More " or "Fewer ") .. name, width = CELL_W, height = STEP_H, size = 20,
        icon_off = dir > 0 and "expand_less" or "expand_more", auto_repeat = true, focus_policy = "none",
        on_clicked = function() range.send(dir > 0 and "increase" or "decrease") end }
    end
    local column = ui.Column { gap = 2, stepper(1), area, stepper(-1) }
    spins[name] = { area = area, range = range }
    return column
  end
  local function colon()
    return kit.centred(14, 2 * STEP_H + READ_H + 4, kit.readout { value = ":", size = spec.size or 32,
      color = kit.ink("lo") })
  end

  -- The spins, built when first wanted (a popover's, when it first opens).
  local panel
  local function build()
  if panel then return panel end
  local row = { gap = 4 }
  if twelve then
    row[#row + 1] = spin("hours", 1, 13, 1, 3,
      function() local h = st.h % 12 return h == 0 and 12 or h end,
      function(v) st.h = (v % 12) + (st.h >= 12 and 12 or 0) end,
      function() local h = st.h % 12 return tostring(h == 0 and 12 or h) end)
  else
    row[#row + 1] = spin("hours", 0, 24, 1, 6, function() return st.h end, function(v) st.h = v end,
      function() return ("%02d"):format(st.h) end)
  end
  row[#row + 1] = colon()
  row[#row + 1] = spin("minutes", 0, 60, spec.minute_step or 1, 10, function() return st.m end,
    function(v) st.m = v end, function() return ("%02d"):format(st.m) end)
  if spec.seconds then
    row[#row + 1] = colon()
    row[#row + 1] = spin("seconds", 0, 60, 1, 10, function() return st.s end, function(v) st.s = v end,
      function() return ("%02d"):format(st.s) end)
  end
  if twelve then
    row[#row + 1] = ui.Item { width = 8, height = 1 }
    row[#row + 1] = ui.Item { width = 56, height = 2 * STEP_H + READ_H + 4,
      widgets.segmented { id = id and (id .. "-meridiem"), accessible_name = "Before or after noon",
        orientation = "vertical", items = { "AM", "PM" }, item_width = 56, item_height = 40, gap = 4,
        anchors = { vertical_center = true },
        current = function() return st.h >= 12 and 2 or 1 end,
        on_current_changed = function(i)
          local pm = i == 2
          if pm ~= (st.h >= 12) then st.h = (st.h + 12) % 24 changed() end
        end } }
  end
  panel = ui.Row(row)
  return panel
  end
  local function later_build(node)
    -- The configuration's time, when it keeps it.
    if type(spec.value) == "function" then
      morf.effect("kit.time_picker.value." .. tostring(node), function()
        local h, m, s = parse(spec.value())
        if h and (h ~= st.h or m ~= st.m or s ~= st.s) then st.h, st.m, st.s = h, m, s end
      end, { owner = node })
    end
  end
  local handle = {}
  function handle.value() return text() end
  function handle.set(time) local h, m, s = parse(time) if h then st.h, st.m, st.s = h, m, s changed() end end
  function handle.focus() if spins.hours then morf.focus.set(spins.hours.area, true) end end
  if spec.inline then
    build()
    for _, k in ipairs { "x", "y", "anchors" } do if spec[k] ~= nil then panel[k] = spec[k] end end
    later_build(panel)
    return panel, handle
  end
  local popup
  local field
  local function toggle()
    if not popup then
      build()
      -- (Over the popover's ground, which its skin lays after its content.)
      panel.z = 1
      popup = widgets.popover { id = id and (id .. "-popup"), content = panel, padding = 12,
        placement = spec.placement or "bottom-start", root = spec.root,
        on_opened = function() handle.focus() end, on_closed = spec.on_closed }
      handle.popup = popup
    end
    popup.toggle(field)
    handle.focus()
  end
  field = widgets.push { id = id and (id .. "-field"), label = caption, icon = spec.icon or "schedule",
    accessible_name = spec.accessible_name or "Time", width = spec.width or 140, height = spec.height or 36,
    x = spec.x, y = spec.y, anchors = spec.anchors,
    on_clicked = toggle }
  later_build(field)
  function handle.open() if not (popup and popup.is_open()) then toggle() end end
  function handle.close() if popup then popup.close("closed") end end
  function handle.is_open() return popup ~= nil and popup.is_open() end
  return field, handle
end
