-- A combo box: a closed field showing the current item that opens a list
-- of them (Press + Popup + Selection); with `search = true` the field is a
-- text field that filters the list as it is typed into (TextField + Popup
-- + Collection: autocomplete).
--
--     local node, combo = composites.combo_box {
--       id = "size", width = 220, items = { "Small", "Medium", "Large" },
--       current = 2,                      -- or a function (then follow it in on_changed)
--       on_changed = function(index, item) end,
--       variant = "dropdown",             -- "select": no ground, for a row's end
--       placeholder = "Choose…", search = false,
--     }
--     combo.open() combo.close() combo.current() combo.set(3)
--
-- Items are strings or tables (`label`, `icon`, ...). The field opens on a
-- press, Space, Return and Alt+Down; the list's arrows move, Return or a
-- press picks, Escape closes, and focus goes back to the field. Other
-- fields: `x`, `y`, `height` (40), `icon` (leading), `item_height` (36),
-- `visible_items` (8), `list_width`, `placement` ("bottom-start"),
-- `header` (a node over the list), `item_id(index, item)` (each entry's
-- id; `<id>-item-<index>` otherwise), `except` (nodes a press on which
-- does not close the list), `text_size`, `accessible_name`,
-- `on_opened`, `on_closed(reason)`.
--
-- The field's look -- a theme's card under a menu row's wash, the label
-- and a chevron -- is exported as `field(spec)` for the composites that
-- open something the same way (picker, rows.combo_row).
local ui = require("morf.ui")
local control = require("lib.kit.control")
local popup = require("lib.kit.popup")
local selection = require("lib.kit.selection")
local collection = require("lib.kit.collection")
local scroll = require("lib.kit.scroll")

local M = {}

local function get(v) if type(v) == "function" then return v() end return v end
local function K() local ok, kit = pcall(require, "kit") return ok and type(kit) == "table" and kit or {} end

local function label_of(item)
  if item == nil then return nil end
  if type(item) == "table" then return tostring(item.label or item.name or item.text or item.title or "") end
  return tostring(item)
end
M.label_of = label_of

local serial = 0
local function next_key(spec, what)
  serial = serial + 1
  return ("kit.composites.%s.%s.%d"):format(what, tostring(spec.id or "anon"), serial)
end
M.next_key = next_key

--- The closed field: a Press (a menu row's wash and the theme's focus ring)
--- over the theme's card, its `text` (fn) and a chevron that turns over
--- while `open` (fn). `spec`: `id`, `x`, `y`, `width`, `height`, `variant`
--- ("select": no card), `icon`, `text`, `placeholder` (fn: whether the
--- text is a placeholder), `open`, `on_clicked`, `on_key_pressed`,
--- `content` (a node drawn instead of the text: a picker's swatch).
function M.field(spec)
  local kit = K()
  local W, H = spec.width or 220, spec.height or 40
  local pad = 12
  local children = {}
  if spec.variant ~= "select" and kit.card then
    children[#children + 1] = kit.card { anchors = { fill = true } }
  end
  local x = pad
  if spec.icon and kit.icon then
    children[#children + 1] = kit.icon(spec.icon, 18, kit.ink and kit.ink("lo"),
      { x = pad, anchors = { vertical_center = true } })
    x = pad + 26
  end
  if spec.content then
    spec.content.x = x
    spec.content.anchors = { vertical_center = true }
    children[#children + 1] = spec.content
  elseif kit.text then
    local hi, lo = kit.ink and kit.ink("hi"), kit.ink and kit.ink("lo")
    children[#children + 1] = kit.text { x = x, anchors = { vertical_center = true },
      width = math.max(1, W - x - 34), elide = "right", text = spec.text, font_size = spec.text_size,
      color = (hi and lo) and function() return (spec.placeholder and spec.placeholder()) and lo() or hi() end or nil }
  end
  if kit.icon then
    children[#children + 1] = ui.Item { width = 20, height = 20,
      anchors = { right = true, right_margin = 10, vertical_center = true },
      rotation = function() return spec.open and spec.open() and 180 or 0 end,
      behavior = { rotation = { duration = 200, easing = "out_cubic" } },
      kit.icon(spec.chevron or "expand_more", 20, kit.ink and kit.ink("lo")) }
  end
  local settings = { widget = "menu_item", id = spec.id, x = spec.x, y = spec.y, width = W, height = H,
    on_clicked = spec.on_clicked, on_key_pressed = spec.on_key_pressed, anchors = spec.anchors,
    accessible_name = spec.accessible_name or spec.text, cursor = "pointer", enabled = spec.enabled }
  return (control.make("Press", "menu_item", settings, { children = children }))
end

--- Whether a key opens a closed field: Alt with Down (or Up).
function M.opens(name, modifiers)
  return (name == "Down" or name == "Up") and tostring(modifiers or ""):find("alt", 1, true) ~= nil
end

-- The list of a plain combo: a Selection in a Scroll, kept in sight of its
-- current entry.
local function plain_list(spec, items, highlight, pick, item_id)
  local W = (spec.list_width or spec.width or 220) - 8
  local ITEM_H = spec.item_height or 36
  local most = spec.visible_items or 8
  local list, t = selection.make("list_selection", {
    id = spec.id and (spec.id .. "-list") or nil, items = items, orientation = "vertical", gap = 0,
    item_width = W, item_height = ITEM_H, press_activates = true,
    current = function() return highlight:get() end,
    on_current_changed = function(i) highlight:set(i) end,
    on_activated = function(i) pick(i) end,
    item_id = item_id, accessible_name = spec.accessible_name or spec.placeholder,
  })
  local view_h = function() return math.max(1, math.min(#items(), most)) * ITEM_H end
  local node, flick = scroll.make("scroll_view", { width = W, height = view_h, clip = true, list })
  morf.effect(next_key(spec, "combo.follow"), function()
    local top, h = t.current_y or 0, t.current_height or ITEM_H
    local now, room = flick.content_y or 0, view_h()
    if top < now then flick.content_y = top
    elseif top + h > now + room then flick.content_y = top + h - room end
  end, { owner = node })
  return node
end

local function make(spec)
  spec = spec or {}
  local kit = K()
  local id = spec.id
  local W, H = spec.width or 220, spec.height or 40
  local function items() return get(spec.items) or {} end
  local chosen = morf.signal(next_key(spec, "combo.chosen"), type(spec.current) == "number" and spec.current or 0)
  local function current()
    if type(spec.current) == "function" then return tonumber(spec.current()) or 0 end
    return chosen:get()
  end
  local open = morf.signal(next_key(spec, "combo.open"), false)
  local highlight = morf.signal(next_key(spec, "combo.highlight"), 0)
  local item_id = spec.item_id or (id and function(i) return id .. "-item-" .. i end) or nil
  local handle = {}
  local menu, field, build_content
  local function is_open() return menu ~= nil and menu.is_open() end
  -- The list is built the first time it opens: until then it would hang
  -- from nothing.
  local function ensure()
    if menu then return menu end
    local content = build_content()
    local header = get(spec.header)
    if header then content = ui.Column { gap = 4, header, content } end
    -- (Over the popup's ground, which its skin adds after it.)
    content.z = 1
    menu = popup.make("dropdown_list", {
      id = id and (id .. "-popup") or nil, content = content, padding = 4,
      width = (spec.list_width or W),
      placement = spec.placement or "bottom-start", close_policy = "escape+outside",
      focus_on_open = not spec.search, except = spec.except,
      on_closed = function(reason)
        open:set(false)
        if spec.on_closed then spec.on_closed(reason) end
      end,
      on_opened = spec.on_opened,
    })
    handle.popup = menu
    return menu
  end

  local function choose(i)
    local item = items()[i]
    if item == nil then return end
    chosen:set(i)
    if spec.on_changed then spec.on_changed(i, item) end
  end
  local function close(reason) if menu then menu.close(reason or "closed") end end
  local function pick(i)
    choose(i)
    close("activated")
  end
  local function current_text()
    local item = items()[current()]
    return label_of(item) or ""
  end

  -- -------------------------------------------------------- searchable --
  local input, filtered, model, reset
  if spec.search then
    local group = require("lib.kit.composites.input_group")
    filtered = morf.signal(next_key(spec, "combo.filtered"), {})
    model = morf.list_model({})
    reset = morf.signal(next_key(spec, "combo.reset"), 0)
    local function refilter(query)
      local list = items()
      local labels = {}
      for i, item in ipairs(list) do labels[i] = label_of(item) end
      local hits = morf.text.fuzzy(query or "", labels, { limit = spec.limit or 200 })
      local out, rows = {}, {}
      for n, hit in ipairs(hits) do
        out[n] = hit.index
        rows[n] = { key = tostring(hit.index), label = labels[hit.index], index = hit.index }
      end
      filtered:set(out)
      model:replace(rows, "key")
      highlight:set(#out > 0 and 1 or 0)
    end
    local function revert() if input then input.text = current_text() end end
    local function open_list()
      if is_open() then return end
      reset:set(reset:get() + 1)
      refilter("")
      -- The list starts on the current item.
      for n, index in ipairs(filtered:get()) do if index == current() then highlight:set(n) end end
      open:set(true)
      ensure().open(field)
    end
    local function pick_filtered(n)
      local index = filtered:get()[n]
      if not index then return end
      choose(index)
      revert()
      close("activated")
    end
    local ITEM_H = spec.item_height or 36
    local LW = (spec.list_width or W) - 8
    local list_h = function() return math.max(1, math.min(#filtered:get(), spec.visible_items or 8)) * ITEM_H end
    build_content = function()
    local list, list_handle = collection.make("list", {
      id = id and (id .. "-list") or nil, rows = model, width = LW, height = (spec.visible_items or 8) * ITEM_H,
      row_height = ITEM_H,
      -- (Read again whenever the rows change: a new filter may have moved
      -- the list's own current row.)
      current = function() filtered:get() return highlight:get() end,
      on_current_changed = function(n) highlight:set(n) end,
      -- A press selects its row (the list's selection is emptied each time
      -- the list opens): that is the pick.
      selected = function() reset:get() return {} end,
      on_selection_changed = function(sel)
        local n = type(sel) == "table" and sel[1] or tonumber(tostring(sel or ""):match("%d+"))
        if n and is_open() then pick_filtered(n) end
      end,
      on_activated = function(n) pick_filtered(n) end,
    })
    handle.list = list_handle
    return ui.Item { width = LW, height = list_h, clip = true, list }
    end
    local node
    node, input = group.make {
      id = id, x = spec.x, y = spec.y, width = W, height = H, widget = "search",
      text = current_text(), placeholder = spec.placeholder or "Search…",
      variant = spec.variant, icon = spec.icon or "search",
      on_edited = function(text)
        if not is_open() then open_list() end
        refilter(text)
      end,
      on_accepted = function()
        if is_open() then pick_filtered(highlight:get()) else open_list() end
      end,
      -- Typing replaces what the field shows.
      on_focus_changed = function(on)
        if on and input then morf.timer(1, function() if input.focused and input.text == current_text() then input:select_all() end end) end
      end,
      on_escape = function()
        if is_open() then close("escape") end
        revert()
      end,
      on_key_pressed = function(_, _, modifiers, _, name)
        local n = #filtered:get()
        if name == "Down" or name == "Up" then
          if not is_open() then open_list() return true end
          local step = name == "Down" and 1 or -1
          highlight:set(math.max(1, math.min(n, highlight:get() + step)))
          return true
        elseif name == "Page_Down" or name == "Page_Up" then
          local step = (spec.visible_items or 8) * (name == "Page_Down" and 1 or -1)
          highlight:set(math.max(1, math.min(n, highlight:get() + step)))
          return true
        end
        return false
      end,
      suffix = { icon = "expand_more", id = id and (id .. "-toggle") or nil, accessible_name = "Show all",
        on_clicked = function() if is_open() then close("closed") else open_list() end end },
    }
    field = node
    handle.open = open_list
    handle.input = input
    -- What a caller sets from outside shows in the field.
    morf.effect(next_key(spec, "combo.text"), function()
      local text = current_text()
      if input and not open:get() then input.text = text end
    end, { owner = field })
  else
    local function open_list()
      if is_open() then return end
      highlight:set(current() > 0 and current() or 1)
      open:set(true)
      ensure().open(field)
    end
    field = M.field {
      id = id, x = spec.x, y = spec.y, width = W, height = H, variant = spec.variant, icon = spec.icon,
      anchors = spec.anchors, enabled = spec.enabled, text_size = spec.text_size,
      text = function() local t = current_text() return t ~= "" and t or (spec.placeholder or "") end,
      placeholder = function() return current_text() == "" end,
      open = function() return open:get() end,
      accessible_name = spec.accessible_name and function()
        return get(spec.accessible_name) .. ": " .. current_text()
      end or nil,
      on_clicked = function() if is_open() then close("closed") else open_list() end end,
      on_key_pressed = function(_, _, modifiers, _, name)
        if M.opens(name, modifiers) then open_list() return true end
        return false
      end,
    }
    build_content = function() return plain_list(spec, items, highlight, pick, item_id) end
    handle.open = open_list
  end

  handle.node = field
  handle.close = close
  handle.toggle = function() if is_open() then close("closed") else handle.open() end end
  handle.is_open = is_open
  -- (A binding can follow this one: the popup is only made on first open.)
  handle.opened = function() return open:get() end
  handle.current = current
  handle.set = function(i) chosen:set(i) end
  handle.highlight = function() return highlight:get() end
  handle.filtered = filtered and function() return filtered:get() end or nil
  return field, handle
end

M.make = make
return M
