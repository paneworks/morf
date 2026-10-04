-- A notification stack: cards that come in at the top, newest first, stay
-- for a while and go -- by a swipe, their close button or their timeout
-- -- in a non-modal overlay at a corner of the surface (Popup + Collection
-- + Drag, swipe). With `mode = "toast"` it is a toast overlay: slips at
-- the bottom centre in the theme's toast look, a few seconds each.
--
--     local node, stack = composites.notification_stack {
--       id = "notes", root = surface_root, width = 360,
--       timeout = 6000,                        -- ms; 0 keeps them until dismissed
--       on_activated = function(note) end, on_dismissed = function(note, reason) end,
--     }
--     local key = stack.notify { title = "Mail", body = "Three new messages", icon = "mail",
--       app = "Mail", actions = { { label = "Open", on_clicked = open_mail } } }
--     stack.dismiss(key) stack.clear() stack.count() stack.expand(key)
--
-- A note is `{ title, body, icon, app, time, urgency ("critical" stays),
-- timeout, actions, key }`. Its card shows the icon, the app and the time,
-- the title and the body's first line; the chevron expands it to the
-- whole body and its actions. A press on the card is `on_activated`. A
-- drag sideways past `swipe_distance` (80 px) or a fling dismisses it; let
-- go short and it springs back. The overlay opens with the first note and
-- closes with the last; it never takes focus or a press outside it.
-- `inline = true` returns the stack as a node to place instead. Other
-- fields: `max_visible` (4; toasts 3) -- how many cards the stack is tall
-- -- `anchor` + `placement` (a corner node of the caller's; by default
-- the root's top right, "bottom-end", or its bottom centre for toasts),
-- `notifications` (shown at once). Ids: `<id>-list`, `<id>-note-<key>`,
-- `<id>-close-<key>`, `<id>-expand-<key>`, `<id>-action-<key>-<n>`.
local ui = require("morf.ui")
local popup = require("lib.kit.popup")
local control = require("lib.kit.control")
local collection = require("lib.kit.collection")
local drag = require("lib.kit.drag")

local function K() local ok, kit = pcall(require, "kit") return ok and type(kit) == "table" and kit or {} end

local serial = 0

local function make(spec)
  spec = spec or {}
  local kit = K()
  serial = serial + 1
  local skey = "kit.composites.notification_stack." .. tostring(spec.id) .. "." .. serial
  local id = spec.id
  local toast = spec.mode == "toast"
  local W = spec.width or (toast and 320 or 360)
  local GAP = 8
  local TRAY = 6
  local CARD_H = toast and 62 or 84
  local MAX = spec.max_visible or (toast and 3 or 4)
  local TIMEOUT = spec.timeout
  if TIMEOUT == nil then TIMEOUT = toast and 3500 or 6000 end
  local model = morf.list_model({})
  local version = morf.signal(skey .. ".version", 0)
  local notes = {}          -- key -> the note as given
  local next_key = 0
  local overlay, content, list_node, list_handle

  local function bump() version:set(version:get() + 1) end
  local function index_of(key)
    for i = 1, model:len() do if model:get(i).key == key then return i end end
  end
  local function total()
    version:get()
    local h, n = 0, 0
    for i = 1, model:len() do
      if n >= MAX then break end
      h, n = h + model:get(i).height, n + 1
    end
    return math.max(1, h - GAP)
  end
  -- An expanded card: its whole body (wrapped, roughly measured) and its
  -- actions.
  local function expanded_height(note)
    local chars = math.max(1, math.floor((W - 90) / 7))
    local lines = math.max(1, math.ceil(utf8.len(tostring(note.body or "")) / chars))
    return CARD_H + (lines - 1) * 18 + ((note.actions and #note.actions > 0) and 44 or 0) + GAP
  end

  local function sync_overlay()
    if spec.inline or not overlay then return end
    if model:len() > 0 and not overlay.is_open() then overlay.open(spec.anchor or overlay.corner)
    elseif model:len() == 0 and overlay.is_open() then overlay.close("closed") end
  end

  local function dismiss(key, reason)
    local i = index_of(key)
    if not i then return end
    model:remove(i)
    bump()
    local note = notes[key]
    notes[key] = nil
    if note and note.on_dismissed then note.on_dismissed(note, reason or "dismissed") end
    if spec.on_dismissed then spec.on_dismissed(note, reason or "dismissed") end
    sync_overlay()
  end

  local function expand(key, on)
    local i = index_of(key)
    if not i or toast then return end
    local row = model:get(i)
    local note = notes[key]
    if on == nil then on = not row.expanded end
    local next_row = {}
    for k, v in pairs(row) do next_row[k] = v end
    next_row.expanded = on
    next_row.height = on and expanded_height(note) or (CARD_H + GAP)
    model:set(i, next_row)
    bump()
  end

  -- ------------------------------------------------------------ a card --
  local function icon_button(name, icon, key, on_clicked)
    return control.make("Press", "icon", { widget = "icon", id = id and ("%s-%s-%s"):format(id, name, key) or nil,
      icon_off = icon, icon_on = icon, width = 28, height = 28, size = 18, accessible_name = name,
      on_clicked = on_clicked })
  end

  local delegates = 0
  local function delegate(row, s)
    -- The row this delegate is bound to, kept here: a row the model keeps
    -- across a keyed replace keeps its delegate but not its index, and the
    -- collection's own row state reads by index.
    delegates = delegates + 1
    local cur, seen = row, morf.signal(skey .. ".bound." .. delegates, 0)
    local function now() seen:get() return cur end
    local function rebind(next_row) cur = next_row seen:set(seen:get() + 1) end
    local function index_now()
      for i = 1, model:len() do if model:get(i).key == cur.key then return i end end
      return 0
    end
    local function key() return now().key end
    local area
    area = ui.MouseArea { id = id and ("%s-note-%s"):format(id, row.key) or nil, width = W,
      height = function() return now().height end, cursor = "pointer",
      accessible_role = "alert", accessible_name = function() return now().title or "" end,
      accessible_description = function() return now().body or "" end,
      on_clicked = function()
        local note = notes[key()]
        if note and note.on_activated then note.on_activated(note) end
        if note and spec.on_activated then spec.on_activated(note) end
      end }
    local swipe = drag.swipe(area, { axis = "x", distance = spec.swipe_distance or 80,
      on_swiped = function() dismiss(key(), "swiped") end })
    -- It fades as it is dragged away.
    area.opacity = function() return math.max(0.15, 1 - math.abs(swipe.t.delta_x or 0) / (W * 0.9)) end
    local card
    local close_button, expand_button
    local actions_row = not toast and ui.Row { gap = 6, x = 56, visible = function() return now().expanded == true end } or nil
    local built = {}
    local function draw(r)
      if toast then
        if card then ui.destroy(card, true) end
        card = kit.toast { width = W, height = CARD_H, title = r.title, text = r.body ~= "" and r.body or nil,
          icon = r.icon }
        ui.reparent(card, area)
        return
      end
      for _, n in ipairs(built) do ui.destroy(n, true) end
      built = {}
      local note = notes[r.key]
      for n, action in ipairs(note and note.actions or {}) do
        built[n] = control.make("Press", "menu_item", { widget = "menu_item",
          id = id and ("%s-action-%s-%d"):format(id, r.key, n) or nil, label = action.label,
          width = 28 + math.ceil(utf8.len(tostring(action.label or "")) * 8), height = 32,
          on_clicked = function()
            if action.on_clicked then action.on_clicked(note) end
            if action.keep ~= true then dismiss(r.key, "action") end
          end })
        ui.reparent(built[n], actions_row)
      end
      if id then
        close_button.id = ("%s-close-%s"):format(id, r.key)
        expand_button.id = ("%s-expand-%s"):format(id, r.key)
      end
    end
    if not toast then
      local hi, lo = kit.ink("hi"), kit.ink("lo")
      close_button = icon_button("close", "close", row.key, function() dismiss(key(), "closed") end)
      expand_button = icon_button("expand", "expand_more", row.key, function() expand(key()) end)
      local urgent = function() return now().urgency == "critical" end
      local tone = function() return urgent() and kit.signal("alert")() or kit.signal("accent")() end
      card = ui.Item { x = 0, y = 0, width = W, height = function() return now().height - GAP end,
        kit.card { anchors = { fill = true } },
        ui.Item { x = 14, y = 14, width = 32, height = 32,
          kit.surface { anchors = { fill = true }, radius = kit.round and kit.round(16) or 16,
            color = function() return tone():alpha(0.18) end },
          kit.icon(function() return now().icon or "notifications" end, 18, tone, { anchors = { center_in = true } }) },
        kit.label { x = 56, y = 10, width = W - 56 - 76, elide = "right",
          text = function()
            local r = now()
            local parts = {}
            if r.app and r.app ~= "" then parts[#parts + 1] = r.app end
            if r.time and r.time ~= "" then parts[#parts + 1] = r.time end
            return table.concat(parts, " · ")
          end },
        kit.text { x = 56, y = 28, width = W - 56 - 14, elide = "right", font_weight = 600, color = hi,
          text = function() return now().title or "" end },
        kit.text { x = 56, y = 48, width = W - 56 - 14, color = lo,
          wrap = function() return now().expanded == true end,
          elide = function() return now().expanded and "none" or "right" end,
          height = function() local r = now() return r.expanded and math.max(18, r.height - GAP - 48 - 12 - ((r.has_actions and 44) or 0)) or 18 end,
          clip = true,
          text = function() return now().body or "" end },
        ui.Row { anchors = { right = true, top = true, right_margin = 8, top_margin = 8 }, gap = 2,
          ui.Item { width = 28, height = 28,
            rotation = function() return now().expanded and 180 or 0 end,
            behavior = { rotation = ui.spring { stiffness = 420, damping = 24 } },
            visible = function() local r = now() return (r.body or "") ~= "" or r.has_actions == true end,
            expand_button },
          close_button },
      }
      actions_row.y = function() return now().height - GAP - 44 end
      ui.reparent(actions_row, card)
      ui.reparent(card, area)
    end
    draw(row)
    return area, function(next_row)
      rebind(next_row)
      if id then area.id = ("%s-note-%s"):format(id, next_row.key) end
      draw(next_row)
    end
  end

  local function build()
    if content then return content end
    list_node, list_handle = collection.make("list", {
      id = id and (id .. "-list") or nil, rows = model, width = W, height = MAX * (CARD_H + GAP) + 400,
      row_height = CARD_H + GAP, size_field = "height", focus_policy = "none", accessible_name = "Notifications",
      delegate = delegate,
    })
    -- Only as tall as its cards: the rest of the corner stays the surface's.
    list_node.height = total
    list_handle.view.height = total
    content = ui.Item { width = W, height = total, z = 1, list_node }
    return content
  end

  local handle = { model = model }
  local node, corner
  if spec.inline then
    build()
    node = ui.Item { id = id, x = spec.x, y = spec.y, anchors = spec.anchors, width = W, height = total, content }
  elseif not spec.anchor and spec.root then
    -- A corner of the root's to open beside.
    corner = ui.Item { width = 1, height = 1, accessible_hidden = true,
      anchors = toast and { bottom = true, horizontal_center = true, bottom_margin = 16 }
        or { top = true, right = true, top_margin = 8, right_margin = 8 } }
    ui.reparent(corner, spec.root)
    node = corner
  end
  -- The overlay is made with the first note: until then its cards would
  -- hang from nothing.
  local function ensure()
    if spec.inline or overlay then return end
    build()
    overlay = popup.make(toast and "toast" or "notification_popup", {
      -- (The popup's ground is a tray round the cards.)
      id = id and (id .. "-popup") or nil, content = content, padding = TRAY, width = W + 2 * TRAY,
      height = function() return total() + 2 * TRAY end, root = spec.root,
      placement = spec.placement or (toast and "top" or "bottom-end"), close_policy = "none",
      focus_on_open = false, modal = false,
    })
    overlay.corner = corner
    handle.list = list_handle
  end

  function handle.notify(note)
    note = note or {}
    ensure()
    next_key = next_key + 1
    local key = note.key and tostring(note.key) or tostring(next_key)
    if notes[key] then dismiss(key, "replaced") end
    notes[key] = note
    local has_actions = note.actions ~= nil and #note.actions > 0
    -- Newest first: at the top of the stack, or the foot of the toasts.
    local row = { key = key, title = tostring(note.title or ""), body = tostring(note.body or ""), icon = note.icon,
      app = note.app, time = note.time, urgency = note.urgency, expanded = false, has_actions = has_actions,
      height = CARD_H + GAP }
    if toast then model:insert(model:len() + 1, row) else model:insert(1, row) end
    bump()
    -- Past what the stack shows, the oldest go.
    while model:len() > MAX * 3 do dismiss(model:get(toast and 1 or model:len()).key, "overflow") end
    sync_overlay()
    local timeout = note.timeout
    if timeout == nil then timeout = TIMEOUT end
    if timeout and timeout > 0 and note.urgency ~= "critical" then
      morf.timer(timeout, function() dismiss(key, "timeout") end, false)
    end
    return key
  end
  handle.dismiss = dismiss
  handle.expand = expand
  function handle.clear() while model:len() > 0 do dismiss(model:get(1).key, "cleared") end end
  function handle.count() version:get() return model:len() end
  function handle.keys()
    local out = {}
    for i = 1, model:len() do out[i] = model:get(i).key end
    return out
  end
  function handle.is_open() return overlay ~= nil and overlay.is_open() end
  handle.node = node
  handle.list = list_handle
  for _, note in ipairs(spec.notifications or {}) do handle.notify(note) end
  return node, handle
end

return { make = make }
