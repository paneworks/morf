-- Preference rows (a row frame with a Press, Range, TextField or
-- Disclosure in it), the boxed group they stand in, and the scrolled page
-- of groups.
--
--     local rows = require("lib.kit.composites.rows")
--     rows.preferences_page { width = 480, height = 400, groups = {
--       { title = "Display", description = "How it looks", rows = {
--         { kind = "switch_row", title = "Dark mode", active = dark, on_toggled = set_dark },
--         { kind = "combo_row", title = "Scale", items = { "100%", "125%" }, current = 1, on_changed = set_scale },
--         { kind = "spin_row", title = "Font size", value = 11, from = 6, to = 48, on_changed = set_size },
--       } },
--     } }
--     rows.make { kind = "action_row", title = "About", on_activated = about }
--
-- Every row takes `id`, `width` (360), `height`, `title`, `subtitle`,
-- `icon` (leading) and returns its node and a handle. The kinds:
--
-- | row | what is in it |
-- |---|---|
-- | action_row | `on_activated` makes the row a press (a chevron when `chevron ~= false`); `suffix`, a node at its end |
-- | switch_row | a switch: `active` (a value or fn), `on_toggled(on)`; a press on the row toggles it too |
-- | check_row | a checkbox before the title: `active`, `on_toggled(on)` |
-- | combo_row | a combo box at its end: `items`, `current`, `on_changed(index, item)` |
-- | entry_row | the title over a text field: `text`, `placeholder`, `on_changed(text)`, `on_accepted(text)` |
-- | spin_row | − value +: `value`, `from`, `to`, `step`, `digits`, `on_changed(value)`; arrows step it |
-- | expander_row | a disclosure: `rows` (nodes or row specs) shown while expanded, `expanded` |
-- | button_row | a centred press: `on_activated` |
-- | property_row | a title over a read-only value: `value` (a value or fn) |
--
-- `preferences_group { title, description, rows, width }` boxes rows (nodes
-- or specs with `kind`) on the theme's card with separators between;
-- `preferences_page { groups, width, height, gap }` scrolls groups (nodes
-- or group specs). `make(spec)` builds `spec.kind` (a row, a group or a
-- page).
local ui = require("morf.ui")
local control = require("lib.kit.control")
local widgets = require("lib.kit.widgets")
local disclosure = require("lib.kit.disclosure")
local scroll = require("lib.kit.scroll")
local text_field = require("lib.kit.text_field")

local M = {}

local function get(v) if type(v) == "function" then return v() end return v end
local function K() local ok, kit = pcall(require, "kit") return ok and type(kit) == "table" and kit or {} end

local PAD = 14
local serial = 0
local function key(spec, what)
  serial = serial + 1
  return ("kit.composites.rows.%s.%s.%d"):format(what, tostring(spec.id or "anon"), serial)
end

local function height_of(spec) return spec.height or ((spec.subtitle and spec.subtitle ~= "") and 64 or 52) end

-- The title and subtitle, in `width` from `x`.
local function titles(spec, x, width, H)
  local kit = K()
  local col = { x = x, width = width, gap = 2, anchors = { vertical_center = true } }
  if kit.text then
    col[#col + 1] = kit.text { text = spec.title or "", width = width, elide = "right",
      color = kit.ink and kit.ink("hi") }
    if spec.subtitle and spec.subtitle ~= "" then
      col[#col + 1] = (kit.label or kit.text) { text = spec.subtitle, width = width, elide = "right",
        color = kit.ink and kit.ink("lo") }
    end
  end
  return ui.Column(col)
end

-- A row: its leading icon or control, its titles, and `suffix` (a node of
-- width `suffix_w`) at its end. A press when `on_clicked` is given (the
-- menu row's wash and the focus ring); a plain item otherwise.
local function frame(spec, parts)
  local kit = K()
  local W, H = spec.width or 360, height_of(spec)
  local children = {}
  local x = PAD
  local lead = parts.lead
  if not lead and spec.icon and kit.icon then lead = kit.icon(spec.icon, 22, kit.ink and kit.ink("lo")) end
  if lead then
    lead.x = PAD
    lead.anchors = { vertical_center = true }
    children[#children + 1] = lead
    x = PAD + (parts.lead_w or 22) + 12
  end
  local sw = parts.suffix_w or 0
  local tw = math.max(1, W - x - PAD - (sw > 0 and (sw + 12) or 0))
  if not parts.no_titles then children[#children + 1] = titles(spec, x, tw, H) end
  if parts.suffix then
    parts.suffix.anchors = { right = true, right_margin = PAD, vertical_center = true }
    children[#children + 1] = parts.suffix
  end
  for _, extra in ipairs(parts.extra or {}) do children[#children + 1] = extra end
  if parts.on_clicked then
    return (control.make("Press", "menu_item", { widget = "menu_item", id = spec.id, width = W, height = H,
      cursor = "pointer", on_clicked = parts.on_clicked, accessible_name = spec.accessible_name or spec.title,
      checkable = parts.checkable, checked = parts.checked },
      { children = children }))
  end
  local props = { id = spec.id, width = W, height = H, accessible_role = "group",
    accessible_name = spec.title }
  for i, child in ipairs(children) do props[i] = child end
  return ui.Item(props)
end

-- A boolean that is the caller's (a function) or the row's own.
local function flag(spec, name)
  local own = morf.signal(key(spec, name), get(spec.active) == true)
  local function value()
    if type(spec.active) == "function" then return spec.active() == true end
    return own:get()
  end
  local function set(on)
    own:set(on)
    if spec.on_toggled then spec.on_toggled(on) end
  end
  return value, set
end

function M.action_row(spec)
  local kit = K()
  local suffix, sw = spec.suffix, spec.suffix_width or 0
  if not suffix and spec.on_activated and spec.chevron ~= false and kit.icon then
    suffix, sw = kit.icon("chevron_right", 22, kit.ink and kit.ink("lo")), 22
  elseif suffix and sw == 0 then
    sw = (type(suffix.width) == "number" and suffix.width) or 120
  end
  local node = frame(spec, { suffix = suffix, suffix_w = sw,
    on_clicked = spec.on_activated and function() spec.on_activated() end or nil })
  return node, { node = node }
end

function M.switch_row(spec)
  local value, set = flag(spec, "switch")
  local switch = widgets.switch { id = spec.id and (spec.id .. "-switch") or nil, checked = value,
    accessible_name = spec.title, on_toggled = function(on) set(on) end }
  local node = frame(spec, { suffix = switch, suffix_w = 52, on_clicked = function() set(not value()) end })
  return node, { node = node, active = value, set = set }
end

function M.check_row(spec)
  local value, set = flag(spec, "check")
  local box = widgets.checkbox { id = spec.id and (spec.id .. "-check") or nil, checked = value,
    accessible_name = spec.title, on_toggled = function(on) set(on) end }
  local node = frame(spec, { lead = box, lead_w = 24, on_clicked = function() set(not value()) end,
    suffix = spec.suffix, suffix_w = spec.suffix and (spec.suffix_width or 120) or 0 })
  return node, { node = node, active = value, set = set }
end

function M.combo_row(spec)
  local CW = spec.combo_width or math.min(200, math.floor((spec.width or 360) * 0.45))
  local combo_node, combo = require("lib.kit.composites.combo_box").make {
    id = spec.id and (spec.id .. "-combo") or nil, width = CW, height = 40, variant = spec.variant or "select",
    items = spec.items, current = spec.current, on_changed = spec.on_changed, search = spec.search,
    accessible_name = spec.title, item_id = spec.item_id, placeholder = spec.placeholder,
  }
  local node = frame(spec, { suffix = combo_node, suffix_w = CW })
  return node, { node = node, combo = combo }
end

function M.entry_row(spec)
  local kit = K()
  local W = spec.width or 360
  local H = spec.height or 64
  local props = require("lib.kit.composites.input_group").style {
    id = spec.id and (spec.id .. "-entry") or nil, x = PAD, y = 26, width = W - 2 * PAD, height = 30,
    text = spec.text, placeholder = spec.placeholder, validator = spec.validator,
    accessible_name = spec.title,
    on_edited = function(text) if spec.on_changed then spec.on_changed(text) end end,
    on_accepted = function(text) if spec.on_accepted then spec.on_accepted(text) end end,
  }
  local field, input = text_field.make(spec.widget or "entry", props)
  local label = (kit.label or kit.text) and (kit.label or kit.text) { x = PAD, y = 8, text = spec.title or "",
    width = W - 2 * PAD, elide = "right", color = kit.ink and kit.ink("lo") } or nil
  local node = ui.Item { id = spec.id, width = W, height = H, label, field }
  return node, { node = node, input = input }
end

function M.spin_row(spec)
  local from, to = spec.from or 0, spec.to or 100
  local step = spec.step or 1
  local digits = spec.digits or 0
  local value = morf.signal(key(spec, "spin"), tonumber(get(spec.value)) or from)
  local function shown() return ("%." .. digits .. "f"):format(value:get()) end
  local input
  local range
  -- (What the range says while it is being made is its settings landing.)
  local ready = false
  range = control.headless("Range", { from = from, to = to, step = step, value = value:get(),
    on_value_changed = function(v)
      value:set(v)
      if input then input.text = shown() end
      if ready and spec.on_changed then spec.on_changed(v) end
    end })
  ready = true
  local SW = spec.spin_width or 150
  local group = require("lib.kit.composites.input_group")
  local holder
  holder, input = group.make {
    id = spec.id and (spec.id .. "-value") or nil, width = SW, height = 40, widget = "numeric_entry", variant = "select",
    text = shown(), horizontal_alignment = "center", accessible_name = spec.title,
    prefix = { icon = "remove", id = spec.id and (spec.id .. "-down"), accessible_name = "Less", auto_repeat = true,
      on_clicked = function() range.send("decrease") end },
    suffix = { icon = "add", id = spec.id and (spec.id .. "-up"), accessible_name = "More", auto_repeat = true,
      on_clicked = function() range.send("increase") end },
    on_accepted = function(text)
      local v = tonumber(text)
      if v then range.send("set", v) end
      input.text = shown()
    end,
    on_key_pressed = function(_, _, modifiers, _, name)
      if name == "Up" or name == "Down" or name == "Page_Up" or name == "Page_Down" then
        return range.key(name, modifiers or "", "")
      end
      return false
    end,
  }
  local node = frame(spec, { suffix = holder, suffix_w = SW })
  return node, { node = node, value = function() return value:get() end,
    set = function(v) range.send("set", v) end, input = input }
end

function M.expander_row(spec)
  local W = spec.width or 360
  local H = height_of(spec)
  local list = { gap = 0, width = W }
  for _, row in ipairs(spec.rows or {}) do
    if type(row) == "table" and row.kind then
      local s = {}
      for k, v in pairs(row) do s[k] = v end
      s.width = s.width or W
      row = (M[row.kind](s))
    end
    list[#list + 1] = row
  end
  local node, t = disclosure.make("expander_row", { id = spec.id, width = W, header_height = H,
    title = spec.title, expanded = spec.expanded, accessible_name = spec.title,
    on_expanded = spec.on_expanded, on_collapsed = spec.on_collapsed, content = ui.Column(list) })
  return node, { node = node, t = t }
end

function M.button_row(spec)
  local kit = K()
  local W, H = spec.width or 360, spec.height or 52
  local label = kit.text and kit.text { anchors = { center_in = true }, text = spec.title or "",
    color = kit.ink and kit.ink("accent") } or nil
  local s = {}
  for k, v in pairs(spec) do s[k] = v end
  s.height = H
  local node = frame(s, { no_titles = true, extra = { label },
    on_clicked = function() if spec.on_activated then spec.on_activated() end end })
  return node, { node = node }
end

function M.property_row(spec)
  local kit = K()
  local W, H = spec.width or 360, spec.height or 60
  local col = { x = PAD, width = W - 2 * PAD, gap = 2, anchors = { vertical_center = true } }
  if kit.text then
    col[#col + 1] = (kit.label or kit.text) { text = spec.title or "", width = W - 2 * PAD, elide = "right",
      color = kit.ink and kit.ink("lo") }
    col[#col + 1] = kit.text { text = function() return tostring(get(spec.value) or "") end, width = W - 2 * PAD,
      elide = "right", color = kit.ink and kit.ink("hi") }
  end
  local node = ui.Item { id = spec.id, width = W, height = H, accessible_role = "group",
    accessible_name = spec.title, ui.Column(col) }
  return node, { node = node }
end

-- ------------------------------------------------------------ groups --

local function build_row(row, W)
  if type(row) ~= "table" or not row.kind then return row end
  local s = {}
  for k, v in pairs(row) do s[k] = v end
  s.width = s.width or W
  return (M[row.kind](s))
end

function M.preferences_group(spec)
  local kit = K()
  local W = spec.width or 360
  local col = { id = spec.id, width = W, gap = 6 }
  if spec.title and kit.text then
    col[#col + 1] = (kit.section_label or kit.text) { text = spec.title, width = W, elide = "right",
      x = 4, color = kit.ink and kit.ink("accent") }
  end
  if spec.description and kit.label then
    col[#col + 1] = kit.label { text = spec.description, width = W - 8, x = 4, elide = "right" }
  end
  local list = { width = W, gap = 0 }
  for i, row in ipairs(spec.rows or {}) do
    if i > 1 and kit.separator then list[#list + 1] = kit.separator { width = W - 2 * PAD, x = PAD } end
    list[#list + 1] = build_row(row, W)
  end
  local column = ui.Column(list)
  local box = ui.Item { width = W, height = function() return column.layout_height or 0 end,
    kit.card and kit.card { anchors = { fill = true } } or nil, column }
  col[#col + 1] = box
  local node = ui.Column(col)
  node.accessible_role, node.accessible_name = "group", spec.title
  return node, { node = node }
end

function M.preferences_page(spec)
  local W, H = spec.width or 400, spec.height or 400
  local inner = W - 16
  local col = { gap = spec.gap or 18, width = inner, x = 0 }
  for _, group in ipairs(spec.groups or {}) do
    if type(group) == "table" and not group.kind and (group.rows or group.title) then
      local s = {}
      for k, v in pairs(group) do s[k] = v end
      s.width = s.width or inner
      group = (M.preferences_group(s))
    elseif type(group) == "table" and group.kind then
      group = (M.make(group))
    end
    col[#col + 1] = group
  end
  local node, flick = scroll.make("scroll_view", { id = spec.id, x = spec.x, y = spec.y, width = W, height = H,
    clip = true, focus_policy = "none", ui.Column(col) })
  return node, { node = node, flick = flick }
end

--- Builds `spec.kind`: any row, `preferences_group` or `preferences_page`.
function M.make(spec)
  spec = spec or {}
  local build = M[spec.kind or "action_row"]
  if type(build) ~= "function" or spec.kind == "make" then error("rows: no kind `" .. tostring(spec.kind) .. "`", 2) end
  return build(spec)
end

return M
