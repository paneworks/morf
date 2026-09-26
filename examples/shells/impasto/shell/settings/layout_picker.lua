-- Every keyboard layout xkb knows, and the order they are loaded in
-- (LayoutPicker.qml).
--
-- Hyprland passes the comma-separated list to xkb, which loads the layouts
-- as groups in order: the first is the one you start on and the switch key
-- cycles through the rest. The chosen layouts are chips; Edit opens a
-- searchable list of every layout in `evdev.lst` (`base.lst`, its older
-- name, where there is no evdev), where a click takes a layout or lets it
-- go. At least one stays loaded.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local pill = require("components.pill")
local controls = require("components.controls")
local setting = require("components.setting")
local tr = require("services.tr")

local C = theme.color
local fast = function() return theme.behave("fast") end

local M = {}

M.RULES = { "/usr/share/X11/xkb/rules/evdev.lst", "/usr/share/X11/xkb/rules/base.lst" }

--- The `! layout` section of an xkb rules list: `{ id, label }` rows in the
--- file's order.
function M.parse(text)
  local out, inside = {}, false
  for line in tostring(text or ""):gmatch("[^\n]*") do
    if line:match("^!") then
      inside = line:match("^!%s*layout%s*$") ~= nil
    elseif inside then
      local id, label = line:match("^%s+(%S+)%s+(.-)%s*$")
      if id and label and label ~= "" then out[#out + 1] = { id = id, label = label } end
    end
  end
  return out
end

local cached
--- Every layout, read once from the first rules list there is.
function M.layouts()
  if cached then return cached end
  for _, path in ipairs(M.RULES) do
    local text = morf.fs.read(path)
    local found = text and M.parse(text) or {}
    if #found > 0 then cached = found return cached end
  end
  cached = { { id = "us", label = "English (US)" } }
  return cached
end

--- A layout's name, or its id when the list does not know it.
function M.label(id)
  for _, entry in ipairs(M.layouts()) do if entry.id == id then return entry.label end end
  return id
end

--- The layouts `term` matches, by name or id, ignoring case.
function M.matching(term)
  term = tostring(term or ""):lower():match("^%s*(.-)%s*$")
  if term == "" then return M.layouts() end
  local out = {}
  for _, entry in ipairs(M.layouts()) do
    if entry.label:lower():find(term, 1, true) or entry.id:lower():find(term, 1, true) then
      out[#out + 1] = entry
    end
  end
  return out
end

--- `list` ("us,de") as ids, blanks dropped.
function M.split(list)
  local out = {}
  for entry in tostring(list or ""):gmatch("[^,]+") do
    local clean = entry:match("^%s*(.-)%s*$")
    if clean ~= "" then out[#out + 1] = clean end
  end
  return out
end

--- `list` with `id` taken or let go; the last one is never let go.
function M.toggled(list, id)
  local next_list, at = M.split(list), nil
  for i, entry in ipairs(next_list) do if entry == id then at = i end end
  if at then
    if #next_list == 1 then return table.concat(next_list, ",") end
    table.remove(next_list, at)
  else
    next_list[#next_list + 1] = id
  end
  return table.concat(next_list, ",")
end

--- `width`, `current()` (the stored list), `on_changed(list)`, `locked()`.
function M.build(values)
  local W = values.width
  local inner = W - 28
  local locked = values.locked or function() return false end
  local expanded = controls.signal("layouts.expanded", false)
  local filter = controls.signal("layouts.filter", "")
  local chosen_model = morf.list_model({})
  local match_model = morf.list_model({})
  local function toggle(id)
    if locked() then return end
    values.on_changed(M.toggled(values.current(), id))
  end
  local function chosen()
    local out = {}
    for i, id in ipairs(M.split(values.current())) do out[#out + 1] = { id = id, first = i == 1 } end
    return out
  end
  morf.effect("impasto.layouts.chosen", function()
    chosen_model:replace(chosen(), "id")
  end)
  morf.effect("impasto.layouts.matches", function()
    if not expanded:get() then match_model:replace({}, "id") return end
    local rows = {}
    for _, entry in ipairs(M.matching(filter:get())) do rows[#rows + 1] = { id = entry.id, label = entry.label } end
    match_model:replace(rows, "id")
  end)
  local function taken(id)
    for _, entry in ipairs(M.split(values.current())) do if entry == id then return true end end
    return false
  end

  local function chip(r)
    local hovered = controls.signal("layouts.chip", false)
    local row = ui.Row {
      gap = 7, align = "center",
      kit.text { text = M.label(r.id), size = theme.size.label,
        color = function() return r.first and C.accent() or C.text() end },
      kit.glyph { text = "󰅖", size = 9, color = C.textMuted },
    }
    return ui.Rect {
      id = "layout-chip-" .. r.id,
      width = function() return (row.layout_width or 0) + 20 end, height = 26, radius = 13,
      color = function() return hovered:get() and C.islandSurfaceHover or C.island end,
      border_width = 1,
      border_color = function() return r.first and C.accent() or C.islandBorder end,
      behavior = { color = fast() },
      ui.Item { x = 10, anchors = { vertical_center = true },
        width = function() return row.layout_width or 0 end, height = 16, row },
      setting.hit { hovered = hovered, enabled = function() return not locked() end,
        on_click = function() toggle(r.id) end },
    }
  end

  local function entry(r)
    local hovered = controls.signal("layouts.row", false)
    local on = function() return taken(r.id) end
    return ui.Rect {
      id = "layout-row-" .. r.id,
      width = inner, height = 28, radius = theme.radius_small - 2,
      color = function() return hovered:get() and C.islandSurfaceHover or morf.color("transparent") end,
      behavior = { color = fast() },
      ui.Row {
        x = 9, width = inner - 18, height = 28, gap = 10, align = "center",
        kit.glyph { text = function() return on() and "󰄬" or "󰝦" end, size = 11,
          color = function() return on() and C.accent() or C.islandBorder end },
        kit.text { text = r.label, size = theme.size.small, width = inner - 18 - 21 - 80, elide = "right",
          font_weight = function() return on() and 600 or 400 end,
          color = function() return on() and C.accent() or C.text() end },
        kit.text { text = r.id, mono = true, size = theme.size.label, color = C.textMuted,
          width = 60, horizontal_alignment = "right" },
      },
      setting.hit { hovered = hovered, on_click = function() toggle(r.id) end },
    }
  end

  local chips = ui.Repeater {
    as = "flex", direction = "row", wrap = true, gap = 6, width = inner,
    model = chosen_model, delegate = chip,
  }
  local search_box, search = setting.text_box {
    placeholder = tr("Search {} layouts"):gsub("{}", tostring(#M.layouts())),
    field_width = inner, field_height = 30,
    on_edited = function(text) filter:set(text) end,
    on_escape = function(field) field.text = "" filter:set("") end,
  }
  local list = ui.Repeater { as = "column", gap = 1, model = match_model, delegate = entry }
  local body = ui.Column {
    gap = 10, width = inner,
    ui.Item {
      width = inner, height = 26,
      ui.Row {
        anchors = { left = true, vertical_center = true }, gap = 8, align = "center",
        kit.text { text = tr("Layouts, in order"), size = theme.size.small, weight = 600 },
        kit.text { text = tr("· the first is the one you start on"), size = theme.size.label,
          color = C.textMuted, visible = function() return #M.split(values.current()) > 1 end },
      },
      pill.button {
        anchors = { right = true, vertical_center = true }, height = 26,
        text = function() return expanded:get() and tr("Done") or tr("Edit") end,
        icon = function() return expanded:get() and "󰄬" or "󰐕" end,
        active = function() return expanded:get() end,
        visible = function() return not locked() end,
        on_click = function()
          expanded:set(not expanded:get())
          if expanded:get() then search.focus = true end
        end,
      },
    },
    ui.Item { width = inner, height = function() return chips.layout_height or 26 end, chips },
    ui.Item { width = inner, height = 30, visible = function() return expanded:get() end, search_box },
    ui.ClipRect {
      width = inner, height = 220, color = "#00000000",
      visible = function() return expanded:get() end,
      ui.Flickable { anchors = { fill = true }, list },
      kit.text { anchors = { center_in = true }, text = tr("No layout by that name"),
        size = theme.size.small, color = C.textMuted,
        visible = function() return (list.layout_height or 0) < 1 end },
    },
  }
  return ui.Item {
    width = W,
    height = function() return (body.layout_height or 0) + 28 end,
    behavior = { height = fast() },
    opacity = function() return locked() and 0.55 or 1 end,
    ui.Item { x = 14, y = 14, width = inner, height = function() return body.layout_height or 0 end, body },
  }
end

return M
