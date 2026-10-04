-- A header bar (composite: Shell slot + Press): a window's title, its own
-- controls at the start and end, and the window controls -- minimise,
-- maximise, close -- for a window that decorates itself.
--
--     local node = composites.header_bar {
--       id = "header", width = function() return win.width end, window = win,
--       title = "Settings", subtitle = function() return page:get() end,
--       start = { toggle_button }, ["end"] = { menu_button },
--     }
--
-- A press on the bar's empty part moves the window (`start_system_move`),
-- a double press maximises or restores it. The controls are kit icon
-- presses with names a screen reader reads. `controls = false` leaves the
-- window controls out (a dialog, a phone). `flat = true` leaves out its
-- ground and rule: a kit Shell's skin draws them under its header region.
local ui = require("morf.ui")
local widgets = require("lib.kit.widgets")

local function get(v) if type(v) == "function" then return v() end return v end

return function(spec)
  spec = spec or {}
  local kit = require("kit")
  local id = spec.id or "header-bar"
  local W, H = spec.width or 600, spec.height or 46
  local win = spec.window
  local gap = 6
  -- The bar's ground and its rule: the default look's header tone, else
  -- the theme's raised surface.
  local P = kit.theme and kit.theme.P
  local function header_color()
    if P then return function() return P().header end end
    return function() local c = kit.signal and kit.signal("accent")() return c and c:alpha(0.06) or "#00000000" end
  end
  local function line_color()
    if P then return function() return P().border end end
    return function() local c = kit.ink and kit.ink("lo")() return c and c:alpha(0.2) or "#00000000" end
  end
  local function control(name, icon, label, action)
    return widgets.icon { id = id .. "-" .. name, width = 32, height = 32, icon_on = icon, icon_off = icon,
      on = function() return false end, accessible_name = label, on_clicked = action }
  end
  local ends = {}
  for _, node in ipairs(spec["end"] or {}) do ends[#ends + 1] = node end
  if win and spec.controls ~= false then
    ends[#ends + 1] = control("minimize", "minimize", "Minimise", function() win:minimized(true) end)
    ends[#ends + 1] = control("maximize", "crop_square", "Maximise", function() win:maximized(not win:maximized()) end)
    ends[#ends + 1] = control("close", "close", "Close", function() win:close() end)
  end
  local starts = spec.start or {}
  local start_row = ui.Row { anchors = { left = true, left_margin = gap, vertical_center = true }, gap = gap,
    table.unpack(starts) }
  local end_row = ui.Row { anchors = { right = true, right_margin = gap, vertical_center = true }, gap = gap,
    table.unpack(ends) }
  local title = kit.text { id = id .. "-title", anchors = { horizontal_center = true },
    y = spec.subtitle and 5 or (H - 22) / 2, text = spec.title or "", font_weight = 700, elide = "right",
    accessible_role = "heading" }
  local subtitle = spec.subtitle and kit.label { id = id .. "-subtitle", anchors = { horizontal_center = true },
    y = 25, text = spec.subtitle } or nil
  -- The empty part of the bar: it moves the window, a double press toggles maximised.
  local drag = ui.MouseArea { anchors = { fill = true },
    on_pressed = function() if win then win:start_system_move() end end,
    on_double_clicked = function() if win then win:maximized(not win:maximized()) end end }
  local node = ui.Item { id = id, x = spec.x, y = spec.y, width = W, height = H,
    accessible_role = "toolbar", accessible_name = spec.accessible_name or "Header bar",
    ui.Rect { anchors = { fill = true }, color = spec.color or header_color(), visible = not spec.flat },
    ui.Rect { anchors = { left = true, right = true, bottom = true }, height = 1, color = line_color(),
      visible = not spec.flat },
    drag, start_row, title, subtitle, end_row }
  return node, { node = node }
end
