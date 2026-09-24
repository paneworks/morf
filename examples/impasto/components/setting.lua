-- The settings window's vocabulary: a group of rows on one card, a row with
-- its name and reading on the left and its control on the right, a block for
-- anything that is not a row, a row of preview tiles, a text field and a
-- slider.
--
-- Ports of SettingGroup, GroupHeading, SettingRow, SettingLabel,
-- SettingBlock, SettingTiles, SettingField, SettingSlider and
-- SettingDivider. Every piece takes its `width` as a number: the settings
-- window is laid out at one size, and a row is laid out before it can
-- measure its parent. Values a piece shows may be plain or functions
-- (bindings), as in `components/controls.lua`.
--
-- The page scrolls under the pointer, but the engine hands a wheel event to
-- the topmost area under it only, so every area drawn here forwards the
-- wheel to `setting.scroll`, which the window points at its page.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local controls = require("components.controls")

local C = theme.color
local val = controls.val
local signal = controls.signal
local fast = function() return theme.behave("fast") end

local M = {}

local function bind(v)
  if type(v) == "function" then return v end
  return function() return v end
end
M.bind = bind

-- ------------------------------------------------------------------ wheel --

--- Set by the window: `scroll(steps, pixels)` moves the page.
M.scroll = nil

--- An `on_wheel` for any area on the page, so the page scrolls wherever the
--- pointer is.
function M.wheel(_, _, _, pixel_y, _, steps_y)
  if M.scroll then M.scroll(steps_y or 0, pixel_y or 0) end
end

--- A MouseArea over its parent that lights `hovered`, answers `on_click`
--- and passes the wheel on. `enabled` (value or binding) greys it out.
function M.hit(values)
  local hovered, enabled = values.hovered, values.enabled
  return ui.MouseArea {
    anchors = values.anchors or { fill = true },
    z = values.z,
    cursor = function()
      if enabled ~= nil and not val(enabled) then return "default" end
      return values.cursor or "pointer"
    end,
    on_entered = function() if hovered then hovered:set(true) end end,
    on_exited = function() if hovered then hovered:set(false) end end,
    on_clicked = function(_, _, x, y, button)
      if enabled ~= nil and not val(enabled) then return end
      if values.on_click then values.on_click(x, y, button) end
    end,
    on_wheel = M.wheel,
  }
end

--- An area that only passes the wheel on, under a surface's contents.
function M.wheel_area()
  return ui.MouseArea { anchors = { fill = true }, z = -1, on_wheel = M.wheel }
end

-- -------------------------------------------------------------- headings --

--- A group's name in small capitals, with an info glyph that opens the
--- note and the hint under it on a click (GroupHeading).
function M.heading(values)
  local width = values.width
  local told = table.concat((function()
    local parts = {}
    for _, text in ipairs { values.note or "", values.hint or "" } do
      if text ~= "" then parts[#parts + 1] = text end
    end
    return parts
  end)(), " ")
  local opened = signal("heading.open", false)
  local hovered = signal("heading.hover", false)
  local title = kit.text {
    text = function() return tostring(val(values.title) or ""):upper() end,
    size = theme.size.label, weight = 600, letter_spacing = 0.8, color = C.textMuted,
  }
  local info = ui.Item {
    width = 16, height = 14, visible = told ~= "",
    kit.glyph {
      x = 0, y = 0, width = 16, height = 14, glyph = "󰋼", size = 10,
      vertical_alignment = "center",
      color = function() return opened:get() and C.accent() or C.textMuted() end,
      opacity = function() return (opened:get() or hovered:get()) and 1 or 0.5 end,
      behavior = { color = fast(), opacity = fast() },
    },
    M.hit { anchors = { fill = true, margins = -5 }, hovered = hovered, cursor = "help",
      on_click = function() opened:set(not opened:get()) end },
  }
  local head = ui.Row { x = 4, gap = 7, align = "center", title, info }
  local note = kit.text {
    x = 4, text = told, wrap = true, width = width - 24, size = theme.size.label,
    color = C.textMuted,
  }
  return ui.Flex {
    direction = "column", gap = 4, align = "start", width = width,
    ui.Item { width = width, height = function() return head.layout_height or 0 end, head },
    ui.Item {
      width = width, visible = function() return opened:get() and told ~= "" end,
      height = function() return (note.layout_height or 0) + 2 end, note,
    },
  }
end

--- The rule between two rows.
function M.divider(width)
  return ui.Rect { x = 0, y = 0, width = width, height = 1, color = C.islandBorder }
end

-- ----------------------------------------------------------------- group --

--- A heading, then one card holding the rows (SettingGroup). `rows` is the
--- array part; a row may carry `visible`, and a rule is drawn above every
--- shown row that has a shown row above it. `bare` draws no card.
function M.group(values)
  local width = values.width
  local rows = {}
  for index, row in ipairs(values) do
    local above = {}
    for j = 1, index - 1 do above[j] = values[j] end
    rows[#rows + 1] = ui.Item {
      width = width,
      visible = function() return row.visible ~= false end,
      row,
      ui.Rect {
        x = 0, y = 0, width = width, height = 1, color = C.islandBorder,
        visible = function()
          if values.bare then return false end
          for _, other in ipairs(above) do if other.visible ~= false then return true end end
          return false
        end,
      },
    }
  end
  local list = ui.Flex {
    direction = "column", align = "start", width = width,
    gap = values.bare and 10 or 0,
    table.unpack(rows),
  }
  local card = ui.Rect {
    width = width,
    height = function() return list.layout_height or 0 end,
    radius = theme.radius_medium,
    color = values.bare and "#00000000" or C.islandSurface,
    border_width = values.bare and 0 or 1,
    border_color = C.islandBorder,
    M.wheel_area(),
    list,
  }
  local title = values.title
  local parts = { direction = "column", gap = 7, align = "start", width = width, visible = values.visible }
  if title ~= nil and title ~= "" then
    parts[#parts + 1] = M.heading { title = title, note = values.note, hint = values.hint, width = width }
  end
  parts[#parts + 1] = card
  return ui.Flex(parts)
end

-- ------------------------------------------------------------------- row --

--- The left half of a row: the name, and under it what the setting says or,
--- locked, a padlock and why (SettingLabel). `width` a number or binding.
function M.label(values)
  local width = bind(values.width)
  local locked = bind(values.locked or false)
  local reading = bind(values.reading or "")
  local reason = bind(values.reason or "")
  local said = function() return locked() and reason() or reading() end
  -- A Flex rather than a Column: it gives a hidden reading no room, so a
  -- label alone sits in the middle of its row.
  return ui.Flex {
    direction = "column", gap = 1, align = "start",
    kit.text {
      text = values.label, size = theme.size.small, weight = 500, elide = "right",
      width = function() return math.max(10, width()) end,
    },
    ui.Flex {
      direction = "row", gap = 5, align = "center",
      visible = function() return (said() or "") ~= "" end,
      kit.glyph { glyph = "󰌾", size = 9, color = C.textMuted, visible = locked },
      kit.text {
        text = said, size = theme.size.label, elide = "right",
        width = function() return math.max(10, width() - (locked() and 14 or 0)) end,
        color = function()
          if val(values.alarm) and not locked() then return C.red() end
          return C.textMuted()
        end,
      },
    },
  }
end

--- One setting on one line (SettingRow). `control` is the node on the
--- right; `locked` dims the row and stops the control, `reason` says why.
function M.row(values)
  local width = values.width
  local locked = bind(values.locked or false)
  local control = values.control or ui.Item {}
  local label = M.label {
    label = values.label, reading = values.reading, alarm = values.alarm,
    locked = locked, reason = values.reason,
    width = function() return width - 28 - 16 - (control.layout_width or 0) end,
  }
  return ui.Item {
    width = width,
    height = function() return math.max(values.height or 48, (label.layout_height or 0) + 16) end,
    visible = values.visible,
    opacity = function() return locked() and 0.55 or 1 end,
    behavior = { opacity = fast() },
    M.wheel_area(),
    ui.Item { x = 14, anchors = { vertical_center = true },
      width = function() return label.layout_width or 0 end,
      height = function() return label.layout_height or 0 end, label },
    ui.Item {
      anchors = { right = true, right_margin = 14, vertical_center = true },
      width = function() return control.layout_width or 0 end,
      height = function() return control.layout_height or 0 end,
      enabled = function() return not locked() end,
      control,
    },
  }
end

--- A switch row: `checked()` and `on_toggled(on)`.
function M.switch_row(values)
  local out = {}
  for k, v in pairs(values) do out[k] = v end
  out.control = controls.switch { checked = values.checked, on_toggled = values.on_toggled }
  return M.row(out)
end

--- Anything that is not a row, padded like one (SettingBlock). The array
--- part is laid out in a column `width - 2 * padding` wide.
function M.block(values)
  local width = values.width
  local padding = values.padding or 14
  local children = { direction = "column", gap = values.gap or 10, align = values.align or "start",
    width = width - 2 * padding }
  for _, child in ipairs(values) do children[#children + 1] = child end
  local body = ui.Flex(children)
  return ui.Item {
    width = width,
    height = function() return (body.layout_height or 0) + 2 * padding end,
    visible = values.visible,
    M.wheel_area(),
    ui.Item { x = padding, y = padding, width = width - 2 * padding,
      height = function() return body.layout_height or 0 end, body },
  }
end

-- ----------------------------------------------------------------- tiles --

--- An option drawn as the thing it changes, above its caption
--- (PreviewTile). `stage(hovered)` builds what the tile shows, centred in a
--- stage `stage_height` tall; `selected()`, `on_picked()`.
function M.tile(values)
  local width = values.width
  local stage_height = values.stage_height or 62
  local hovered = signal("tile.hover", false)
  local selected = bind(values.selected or false)
  local stage = values.stage and values.stage(hovered) or ui.Item {}
  return ui.Rect {
    width = width, height = stage_height + 34,
    radius = theme.radius_medium,
    color = function()
      if selected() then return C.islandSurfaceHover end
      return hovered:get() and C.islandSurface or "#00000000"
    end,
    border_width = 1,
    border_color = function()
      if selected() then return C.accent() end
      return hovered:get() and C.islandBorder or C.islandSurfaceHover
    end,
    behavior = { color = fast(), border_color = fast() },
    ui.ClipRect {
      x = 1, y = 1, width = width - 2, height = stage_height, color = "#00000000",
      stage,
    },
    kit.text {
      anchors = { horizontal_center = true, bottom = true, bottom_margin = 9 },
      text = values.caption, size = theme.size.label,
      weight = function() return selected() and 600 or 400 end,
      color = function() return selected() and C.accent() or C.textMuted() end,
    },
    M.hit { hovered = hovered, on_click = function() if values.on_picked then values.on_picked() end end },
  }
end

--- A setting whose options are shapes (SettingTiles): the name above, then
--- the tiles sharing the width. `tiles` is a list of `function(width)`
--- returning a tile.
function M.tiles(values)
  local width = values.width
  local locked = bind(values.locked or false)
  -- `columns` wraps the tiles into rows of that many; else one row.
  local count = math.max(1, values.columns or #values.tiles)
  local gap = 10
  local tile_w = math.floor((width - 28 - gap * (count - 1)) / count)
  local built = { gap = gap }
  for index, make in ipairs(values.tiles) do built[index] = make(tile_w) end
  local label = M.label {
    label = values.label, reading = values.reading, locked = locked, reason = values.reason,
    width = width - 28,
  }
  local row
  if values.columns then
    built.columns = count
    row = ui.Grid(built)
  else
    row = ui.Row(built)
  end
  return ui.Item {
    width = width,
    height = function() return (label.layout_height or 0) + 10 + (row.layout_height or 0) + 28 end,
    visible = values.visible,
    opacity = function() return locked() and 0.55 or 1 end,
    behavior = { opacity = fast() },
    M.wheel_area(),
    ui.Item { x = 14, y = 14, width = width - 28, height = function() return label.layout_height or 0 end, label },
    ui.Item {
      x = 14, y = function() return 14 + (label.layout_height or 0) + 10 end,
      width = width - 28, height = function() return row.layout_height or 0 end,
      enabled = function() return not locked() end,
      row,
    },
  }
end

-- ----------------------------------------------------------------- field --

--- A text box as wide as a slider and its figure (the field half of
--- SettingField). Set once from `value()`, not bound, so the caret stays
--- put while typing. `on_edited(text)`, `placeholder`, `field_width`.
function M.text_box(values)
  local field
  local focused = signal("field.focus", false)
  field = ui.TextInput {
    anchors = { fill = true, left_margin = 10, right_margin = 10 },
    vertical_alignment = "center",
    text = val(values.value) or "",
    placeholder = values.placeholder or "",
    placeholder_color = C.textMuted,
    font_family = function() return theme.font() end,
    font_size = theme.size.small,
    color = C.text, caret_color = C.accent,
    selection_color = C.accent, selected_text_color = C.accentText,
    max_length = values.max_length or 0,
    on_text_changed = function(text) if values.on_edited then values.on_edited(text) end end,
    on_accepted = values.on_accepted,
    on_escape = function()
      if values.on_escape then values.on_escape(field) return end
      field.text = ""
      if values.on_edited then values.on_edited("") end
    end,
    on_focus_changed = function(on)
      focused:set(on)
      if values.on_focus_changed then values.on_focus_changed(on) end
    end,
    focus = values.focus or false,
  }
  return ui.Rect {
    width = values.field_width or 300, height = values.field_height or 30,
    radius = theme.radius_small, color = C.island,
    border_width = 1,
    border_color = function() return focused:get() and C.accent() or C.islandBorder end,
    behavior = { border_color = fast() },
    field,
  }, field
end

--- A setting row whose control is a text field (SettingField).
function M.field(values)
  local box = M.text_box(values)
  return M.row {
    width = values.width, label = values.label, reading = values.reading, alarm = values.alarm,
    visible = values.visible, control = box,
  }
end

-- ---------------------------------------------------------------- slider --

--- A setting with a continuous value (SettingSlider): the name at the left,
--- the track and its figure at the right. The wheel steps over the track
--- only. `value()`, `from`, `to`, `step`, `decimals`, `unit`, `reading()`,
--- `locked()`, `reason`, `on_moved(value)`.
function M.slider(values)
  local width = values.width
  local value = bind(values.value or 0)
  local from, to = values.from or 0, values.to or 100
  local step = values.step or 1
  local locked = bind(values.locked or false)
  local track_w = 240
  local hovered = signal("slider.hover", false)
  local position = function()
    return math.max(0, math.min(1, ((tonumber(value()) or 0) - from) / math.max(1e-9, to - from)))
  end
  local reading = values.reading or function()
    return string.format("%." .. (values.decimals or 0) .. "f", tonumber(value()) or 0) .. (values.unit or "")
  end
  local function snap(v)
    local snapped = from + math.floor((v - from) / step + 0.5) * step
    snapped = math.max(from, math.min(to, snapped))
    if (values.decimals or 0) == 0 then snapped = math.floor(snapped + 0.5) end
    return snapped
  end
  local figure_w = values.figure_width or 64
  local control = ui.Row {
    gap = 12, align = "center",
    ui.Item {
      width = track_w, height = 16,
      ui.Rect {
        anchors = { left = true, right = true, vertical_center = true }, height = 4, radius = 2,
        color = C.islandSurfaceHover,
        ui.Rect { x = 0, y = 0, height = 4, radius = 2, color = C.accent,
          width = function() return track_w * position() end },
      },
      ui.Rect {
        anchors = { vertical_center = true },
        x = function() return track_w * position() - (hovered:get() and 8 or 6.5) end,
        width = function() return hovered:get() and 16 or 13 end,
        height = function() return hovered:get() and 16 or 13 end,
        radius = 8, color = C.accentText, border_width = 3, border_color = C.accent,
        behavior = { width = fast(), height = fast() },
      },
      ui.MouseArea {
        anchors = { fill = true },
        cursor = function() return locked() and "default" or "pointer" end,
        on_entered = function() hovered:set(true) end,
        on_exited = function() hovered:set(false) end,
        on_pressed = function(_, _, x)
          if locked() or not values.on_moved then return end
          values.on_moved(snap(from + math.max(0, math.min(1, x / track_w)) * (to - from)))
        end,
        on_dragged = function(_, _, _, _, x)
          if locked() or not values.on_moved then return end
          values.on_moved(snap(from + math.max(0, math.min(1, x / track_w)) * (to - from)))
        end,
        on_wheel = function(_, _, _, _, _, steps)
          if locked() or not steps or steps == 0 or not values.on_moved then return end
          values.on_moved(snap((tonumber(value()) or 0) - steps * step))
        end,
      },
    },
    M.figure {
      width = figure_w, reading = reading, value = value, from = from, to = to, step = step,
      decimals = values.decimals or 0, locked = locked, on_moved = values.on_moved,
    },
  }
  return M.row {
    width = width, label = values.label, locked = locked, reason = values.reason,
    visible = values.visible, control = control,
  }
end

local UP, DOWN = 0xff52, 0xff54

--- A slider's figure, which a click turns into a field for an exact number
--- (SettingSlider.qml's reading slot): Enter or leaving it commits, clamped
--- and snapped to the steps; Escape cancels; Up and Down step and show the
--- new value. Only a number can be typed: a sign, digits and one point (a
--- comma reads as one).
function M.figure(values)
  local width = values.width or 64
  local from, to, step = values.from, values.to, values.step
  local decimals = values.decimals or 0
  local locked = values.locked or function() return false end
  local editing = signal("figure.editing", false)
  local hovered = signal("figure.hover", false)
  local function format(v) return string.format("%." .. decimals .. "f", v) end
  local function clamp(v) return math.max(from, math.min(to, v)) end
  local function commit(entered)
    local parsed = tonumber((tostring(entered or ""):gsub(",", ".")))
    if not parsed or not values.on_moved then return end
    local wanted = clamp(parsed)
    if step > 0 then wanted = from + math.floor((wanted - from) / step + 0.5) * step end
    wanted = clamp(wanted)
    if decimals == 0 then wanted = math.floor(wanted + 0.5) end
    values.on_moved(wanted)
  end
  local valid = ""
  local editor
  local function nudge(direction)
    if not values.on_moved then return end
    local wanted = clamp((tonumber(values.value()) or 0) + direction * step)
    values.on_moved(wanted)
    valid = format(wanted)
    editor.text = valid
    editor.cursor_position = #valid
  end
  editor = ui.TextInput {
    anchors = { fill = true, left_margin = 6, right_margin = 6 },
    horizontal_alignment = "center", vertical_alignment = "center",
    font_family = function() return theme.font_mono() end, font_size = theme.size.small,
    color = C.text, caret_color = C.accent,
    selection_color = C.accent, selected_text_color = C.accentText,
    clip = true,
    on_text_changed = function(text)
      -- The validator: what is not a number being typed goes back.
      if text:match("^%-?%d*[.,]?%d*$") then valid = text return end
      local at = editor.cursor_position
      editor.text = valid
      editor.cursor_position = math.min(#valid, math.max(0, at - 1))
    end,
    on_accepted = function(text)
      if not editing:get() then return end
      editing:set(false)
      editor.focus = false
      commit(text)
    end,
    -- Escape clears `editing` first, so letting go does not commit.
    on_escape = function()
      editing:set(false)
      editor.focus = false
    end,
    on_key_pressed = function(keysym)
      if keysym == UP then nudge(1) return true end
      if keysym == DOWN then nudge(-1) return true end
    end,
    on_focus_changed = function(on)
      if on or not editing:get() then return end
      editing:set(false)
      commit(editor.text)
    end,
  }
  return ui.Item {
    width = width, height = 22,
    kit.text {
      anchors = { right = true, vertical_center = true },
      visible = function() return not editing:get() end,
      text = values.reading, mono = true, size = theme.size.small, width = width,
      horizontal_alignment = "right", elide = "left",
      color = function() return hovered:get() and C.text() or C.textMuted() end,
      behavior = { color = fast() },
    },
    ui.MouseArea {
      anchors = { fill = true },
      visible = function() return not editing:get() end,
      cursor = function() return locked() and "default" or "text" end,
      on_entered = function() hovered:set(true) end,
      on_exited = function() hovered:set(false) end,
      on_clicked = function()
        if locked() then return end
        valid = format(tonumber(values.value()) or 0)
        editing:set(true)
        editor.text = valid
        editor.focus = true
        editor:select_all()
      end,
    },
    ui.Rect {
      anchors = { right = true, vertical_center = true },
      width = 68, height = 22,
      visible = function() return editing:get() end,
      radius = theme.radius_small - 2, color = C.island,
      border_color = C.accent, border_width = 1,
      editor,
    },
  }
end

--- A node that runs `fn` now and whenever what it reads changes, for as
--- long as the node lives. `morf.effect` cannot be ended, so a page that
--- comes and goes watches through a binding of its own instead, which ends
--- with the page. `fn` should only schedule writes (a timer), not make them.
function M.watch(fn)
  return ui.Item {
    width = 1, height = 1, visible = false,
    opacity = function() fn() return 1 end,
  }
end

-- ------------------------------------------------------------------ page --

--- A settings page split into parts (the tab strip's): `parts` a list of
--- `{ id, build(width) }` where `build` returns a list of groups. Only the
--- shown part is built; the others are let go.
function M.parts(page, parts)
  local W = page.width
  local root = { direction = "column", gap = 20, align = "start", width = W }
  for _, part in ipairs(parts) do
    local shown = function() return page.tab() == part.id end
    root[#root + 1] = ui.Item {
      width = W, visible = shown,
      ui.Loader {
        active = shown,
        source = function()
          local children = { direction = "column", gap = 20, align = "start", width = W }
          for _, node in ipairs(part.build(W)) do children[#children + 1] = node end
          return ui.Flex(children)
        end,
      },
    }
  end
  return ui.Flex(root)
end

--- A page of one part: the groups `build(width)` returns, in a column.
function M.page(page, build)
  local children = { direction = "column", gap = 20, align = "start", width = page.width }
  for _, node in ipairs(build(page.width)) do children[#children + 1] = node end
  return ui.Flex(children)
end

-- ----------------------------------------------------------- small parts --

--- A pill button sized to fit (PillButton), passing the wheel on.
function M.pill(values)
  local node = controls.pill(values)
  return node
end

--- A plain word-and-glyph line for a block: `icon`, `text`, `note`.
function M.line(values)
  local parts = { gap = 10, align = "center" }
  if values.icon then
    parts[#parts + 1] = kit.glyph { glyph = values.icon, size = 12, color = values.icon_color or C.accent }
  end
  parts[#parts + 1] = kit.text { text = values.text, size = theme.size.small, color = values.color or C.text,
    width = values.text_width, elide = values.text_width and "right" or nil }
  if values.note then
    parts[#parts + 1] = kit.text { text = values.note, size = theme.size.label, color = C.textMuted }
  end
  return ui.Row(parts)
end

--- A small-caps caption inside a block ("FACES", "DELETED").
function M.caption(text)
  return kit.text { text = text, size = theme.size.label, weight = 600, letter_spacing = 0.8,
    color = C.textMuted }
end

return M
