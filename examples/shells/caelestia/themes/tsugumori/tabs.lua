local stroke = require("themes.tsugumori.strokes")
-- Numbered instrument tabs. Selection fills the header from the left; a
-- square accent rail (`<id>-tab-indicator`) slides under the chosen tab.
-- Labels are one rolling strip each (the pill's roll), fitted to their slot:
-- they shrink to a floor and then elide, so a narrow drawer never cuts a
-- glyph in half. Options as themes/KIT.md: `ids` ("name" | "key") and
-- `growing` (the row follows an easing drawer instead of fixed slots).
local morf = require("morf")
local ui = require("morf.ui")
return function(theme, kit, spec)
  local C, tabs, tab = theme.color, spec.tabs, spec.tab
  local growing = spec.growing == true
  local by_name = spec.ids == "name"
  local pad, height, gap = spec.pad or 11, spec.height or 64, 8
  local MENU = theme.typography.menu
  -- A growing row reads its own laid-out width (the drawer eases it); the
  -- effect at the end keeps this signal in step.
  local grown = growing and morf.signal("caelestia." .. spec.id .. ".tabs.span", 0) or nil
  local function span()
    if growing then return grown:get() end
    local w = type(spec.width) == "function" and spec.width() or spec.width
    return math.max(0, (w or 0) - 2 * pad)
  end
  local function slot() return math.max(1, (span() - gap * (#tabs - 1)) / #tabs) end
  local function left(i) return (i - 1) * (slot() + gap) end
  local buttons = {}
  for i, t in ipairs(tabs) do
    local name = spec.id .. "-tab-" .. (by_name and t.name:lower() or (t.key or t.name:lower()))
    local selected = function() return tab:get() == i end
    local function ink() return selected() and C.onPrimary or C.onSurface end
    local caption = t.name:upper()
    local has_icon = (t.icon_build or t.icon) and true or false
    local button
    local function width() return slot() end
    -- The face's own measure, not a guessed monospace advance.
    local measure = kit.menu_label { text = caption, font_size = MENU, height = 18, opacity = 0 }
    local function natural() return math.max(1, measure.layout_width or utf8.len(caption) * MENU * .62) end
    local function room() return math.max(0, width() - (has_icon and 66 or 42)) end
    local function size() return math.max(9, math.min(MENU, math.floor(MENU * (room() - 4) / natural() * 2) / 2)) end
    local function text_w() return math.min(room(), natural() * size() / MENU + 2) end
    local strip = ui.Column { gap = 0, y = -36,
      kit.menu_label { text = "/ / / / / /", height = 18, color = ink },
      kit.menu_label { text = "+ | + | + |", height = 18, color = ink },
      kit.menu_label { text = caption, font_size = size, height = 18, width = text_w, elide = "right",
        vertical_alignment = "center", color = ink },
    }
    local frame = ui.Rect { anchors = { fill = true }, color = function() return C.surfaceContainer end,
      border_width = 1, border_color = function() return selected() and stroke(C, "focus") or stroke(C, "quiet") end,
      behavior = { border_color = { duration = 180 } } }
    local props = {
      id = name, cursor = "pointer",
      on_clicked = function() tab:set(i) end,
      measure, frame,
      ui.Item { anchors = { fill = true, margins = 1 }, clip = true,
        ui.Rect { x = 0, y = 0, height = 38,
          width = function() return selected() and math.max(0, width() - 2) or 0 end,
          color = function() return C.primary end,
          behavior = { width = { duration = 220, easing = { x1 = 0.76, y1 = 0, x2 = 0.24, y2 = 1 } } } } },
      kit.section_label { text = ("%02d"):format(i), x = 9, y = 14, color = ink },
      ui.Rect { x = 27, y = 10, width = 1, height = 20, color = function() return ink():alpha(0.35) end },
      ui.Item { id = name .. "-label", x = 34, y = 12, height = 18, clip = true,
        width = text_w, visible = function() return text_w() > 0 end, strip },
    }
    -- Registration marks on the chamfer diagonal of the chosen tab.
    for k, corner in ipairs { { left = true, top = true }, { right = true, bottom = true } } do
      local s = k == 1 and -3 or 3
      props[#props + 1] = ui.Path { anchors = corner, width = 7, height = 7, z = 2,
        view_box = { 0, 0, 7, 7 }, d = "M0 0 H7 V1.5 H1.5 V7 H0 Z", rotation = k == 1 and 0 or 180,
        fill_color = function() return C.primary end,
        translate_x = function() return selected() and s or 0 end,
        translate_y = function() return selected() and s or 0 end,
        opacity = function() return selected() and 1 or 0 end,
        behavior = { translate_x = { duration = 340, easing = "out_cubic" },
          translate_y = { duration = 340, easing = "out_cubic" }, opacity = { duration = 220 } } }
    end
    if growing then
      props.width, props.height, props.layout = 10, 40, { grow = 1 }
    else
      props.x = function() return pad + left(i) end
      props.y, props.width, props.height = 8, slot, 40
    end
    button = kit.action(props)
    frame.border_color = function()
      return selected() and stroke(C, "focus") or button.hovered and stroke(C, "hover") or stroke(C, "quiet")
    end
    if has_icon then
      local icon_id = name .. "-icon"
      local icon = t.icon_build and t.icon_build(selected, icon_id, ink)
        or kit.icon(t.icon, 18, ink, { id = icon_id, fill = selected })
      icon.anchors = { right = true, right_margin = 9, vertical_center = true }
      ui.reparent(icon, button)
    end
    local was, running = false, nil
    morf.effect(spec.id .. ".tab-roll." .. i, function()
      local now = button.hovered
      if now == was then return end
      was = now
      if running then running:stop() running = nil end
      if now then
        running = morf.animation.play { { node = strip, property = "y", from = 0, to = -36,
          duration = 300, easing = "out_cubic" } }
      else strip.y = -36 end
    end, { owner = button })
    buttons[i] = button
  end
  -- The rail under the row: a quiet hairline and the accent segment that
  -- slides to the chosen tab.
  local rail = ui.Rect { id = spec.id .. "-tab-rail", y = height - 7, height = 1,
    color = function() return stroke(C, "quiet") end }
  local indicator = ui.Rect { id = spec.id .. "-tab-indicator", y = height - 8, height = 2,
    color = function() return C.primary end,
    x = function() return left(tab:get()) end, width = slot,
    behavior = { x = { duration = 260, easing = "out_cubic" } } }
  if growing then
    rail.anchors = { left = true, right = true }
    local row = ui.Flex { anchors = { left = true, right = true }, y = 8, height = 40, direction = "row", gap = gap,
      padding = 0, table.unpack(buttons) }
    local node = ui.Item { anchors = { left = true, right = true, left_margin = pad, right_margin = pad }, height = height,
      row, rail, indicator }
    morf.effect("caelestia." .. spec.id .. ".tabs.span", function()
      local w = row.layout_width or 0
      if math.abs(w - grown:get()) > .25 then grown:set(w) end
    end, { owner = node })
    return node
  end
  rail.x, rail.width = pad, span
  indicator.x = function() return pad + left(tab:get()) end
  local node = { width = spec.width, height = height, rail, indicator }
  for _, b in ipairs(buttons) do node[#node + 1] = b end
  return ui.Item(node)
end
