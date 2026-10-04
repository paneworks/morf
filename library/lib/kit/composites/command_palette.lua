-- A command palette: a search field over a grouped list of commands that
-- narrows as it is typed into, in a popup (Popup + TextField + Collection).
--
--     local node, palette = composites.command_palette {
--       id = "palette", root = surface_root,        -- Ctrl+K and Ctrl+Shift+P open it there
--       commands = {
--         { title = "Open file", subtitle = "Browse the disk", icon = "folder_open",
--           shortcut = "Ctrl+O", group = "File", action = open_file },
--         { title = "Toggle sidebar", icon = "side_navigation", shortcut = "Ctrl+B",
--           group = "View", keywords = "panel", action = toggle },
--       },
--       on_run = function(command, index) end,
--     }
--     palette.open() palette.close() palette.toggle() palette.is_open()
--
-- Commands are `{ title, subtitle, icon, shortcut, group, keywords, action,
-- id }` (or a function returning the list). The query ranks them fuzzily
-- by title and keywords; they stay under their groups' headers, the groups
-- in the order of their best match. Up, Down, Page_Up and Page_Down walk
-- the commands from the field, Return or a press runs one (closing the
-- palette first), Escape closes it and focus goes back to what had it.
-- `inline = true` returns the palette as a node to place, not a popup (a
-- gallery, a launcher's page). Other fields: `width` (520), `list_height`
-- (320), `placeholder`, `anchor` + `placement` (beside a node; centred on
-- `root` otherwise), `shortcuts` (false: no Ctrl+K), `on_opened`,
-- `on_closed(reason)`. The node returned is the inline palette, or the
-- holder of the shortcuts (already on `root`), or nil. Ids: `<id>-search` (the field), `<id>-list`,
-- `<id>-command-<n>` (a command by its index in `commands`), `<id>-popup`.
local ui = require("morf.ui")
local popup = require("lib.kit.popup")
local collection = require("lib.kit.collection")
local group = require("lib.kit.composites.input_group")

local function K() local ok, kit = pcall(require, "kit") return ok and type(kit) == "table" and kit or {} end
local function get(v) if type(v) == "function" then return v() end return v end

local serial = 0

local function make(spec)
  spec = spec or {}
  local kit = K()
  serial = serial + 1
  local key = "kit.composites.command_palette." .. tostring(spec.id) .. "." .. serial
  local id = spec.id
  local W = spec.width or 520
  local LIST_H = spec.list_height or 320
  local FIELD_H = 44
  local PAD = 8
  local CMD_H, SUB_H, HEAD_H = 40, 50, 28
  local LW = W - 2 * PAD
  local function commands() return get(spec.commands) or {} end

  local model = morf.list_model({})
  -- The shown rows' tops (for the highlight) and which rows are headers.
  -- (A signal holds dense arrays only: the layout is kept here, its
  -- version in the signal.)
  local shape = { tops = {}, heights = {}, headers = {}, count = 0, total = 0 }
  local version = morf.signal(key .. ".layout", 0)
  local layout = { get = function() version:get() return shape end,
    set = function(_, v) shape = v version:set(version:get() + 1) end }
  local highlight = morf.signal(key .. ".highlight", 0)
  local query = morf.signal(key .. ".query", "")

  local function refilter(text)
    local list = commands()
    local labels = {}
    for i, c in ipairs(list) do labels[i] = tostring(c.title or "") .. (c.keywords and (" " .. c.keywords) or "") end
    local hits = morf.text.fuzzy(text or "", labels, { limit = spec.limit or 200 })
    -- Grouped: each group where its best hit ranks.
    local order, by_group = {}, {}
    for _, hit in ipairs(hits) do
      local c = list[hit.index]
      local g = c.group or ""
      if not by_group[g] then by_group[g] = {} order[#order + 1] = g end
      local members = by_group[g]
      members[#members + 1] = hit.index
    end
    local rows, tops, heights, headers = {}, {}, {}, {}
    local y = 0
    for _, g in ipairs(order) do
      if g ~= "" then
        rows[#rows + 1] = { key = "group:" .. g, kind = "header", title = g, height = HEAD_H }
        headers[#rows] = true
        tops[#rows], heights[#rows] = y, HEAD_H
        y = y + HEAD_H
      end
      for _, index in ipairs(by_group[g]) do
        local c = list[index]
        local h = (c.subtitle and c.subtitle ~= "") and SUB_H or CMD_H
        rows[#rows + 1] = { key = "command:" .. index, kind = "command", index = index, title = tostring(c.title or ""),
          subtitle = c.subtitle, icon = c.icon, shortcut = c.shortcut, height = h }
        tops[#rows], heights[#rows] = y, h
        y = y + h
      end
    end
    model:replace(rows, "key")
    layout:set { tops = tops, heights = heights, headers = headers, count = #rows, total = y }
    local first = 0
    for n = 1, #rows do if not headers[n] then first = n break end end
    highlight:set(first)
  end

  local menu, input, list_handle, frame
  local function is_open() return menu ~= nil and menu.is_open() end
  local function close(reason) if menu then menu.close(reason or "closed") end end

  local function run(n)
    local row = model:get(n or highlight:get())
    if not row or row.kind ~= "command" then return end
    local c = commands()[row.index]
    if not c then return end
    if not spec.inline then close("activated") end
    if c.action then c.action(c, row.index) end
    if spec.on_run then spec.on_run(c, row.index) end
  end

  -- From the field: the commands in turn, the headers stepped over.
  local function step(by)
    local l = layout:get()
    local n = highlight:get()
    local target = n
    local moved = 0
    local dir = by > 0 and 1 or -1
    local i = n
    while moved < math.abs(by) do
      i = i + dir
      if i < 1 or i > l.count then break end
      if not l.headers[i] then target = i moved = moved + 1 end
    end
    if target ~= n then highlight:set(target) end
  end

  local fit
  if spec.inline then fit = LIST_H
  else fit = function() return math.max(56, math.min(LIST_H, layout:get().total)) end end
  local H = spec.inline and (PAD + FIELD_H + PAD + LIST_H + PAD)
    or function() return PAD + FIELD_H + PAD + fit() + PAD end

  -- Built when first wanted: a popup's until it first opens.
  local content
  local function build()
  if content then return content end
  -- ------------------------------------------------------------ the field --
  local field
  field, input = group.make {
    id = id and (id .. "-search") or nil, x = PAD, y = PAD, width = LW, height = FIELD_H, widget = "search",
    icon = "search", placeholder = spec.placeholder or "Type a command…", variant = spec.variant,
    accessible_name = spec.accessible_name or "Command",
    on_edited = function(text)
      query:set(text)
      refilter(text)
    end,
    on_accepted = function() run() end,
    on_escape = function() if not spec.inline then close("escape") end end,
    on_key_pressed = function(_, _, _, _, name)
      if name == "Down" then step(1) return true end
      if name == "Up" then step(-1) return true end
      if name == "Page_Down" then step(6) return true end
      if name == "Page_Up" then step(-6) return true end
      return false
    end,
  }

  -- ------------------------------------------------------------- the list --
  local delegates = 0
  local function delegate(row, s)
    -- The row this delegate is bound to, kept here: a row the model keeps
    -- across a keyed replace keeps its delegate but not its index, and the
    -- collection's own row state reads by index.
    delegates = delegates + 1
    local cur, seen = row, morf.signal(key .. ".bound." .. delegates, 0)
    local function now() seen:get() return cur end
    local function rebind(next_row) cur = next_row seen:set(seen:get() + 1) end
    local function index_now()
      for i = 1, model:len() do if model:get(i).key == cur.key then return i end end
      return 0
    end
    if row.kind == "header" then
      local node = ui.Item { width = LW, height = HEAD_H,
        kit.label { x = 12, y = 8, width = LW - 24, elide = "right", text = function() return now().title or "" end } }
      return node, rebind
    end
    local area
    local hi, lo = kit.ink and kit.ink("hi"), kit.ink and kit.ink("lo")
    local function has_sub() local r = now() return r.subtitle ~= nil and r.subtitle ~= "" end
    local icon = kit.icon(function() return now().icon or "chevron_right" end, 20, lo,
      { x = 12, anchors = { vertical_center = true } })
    local caps = ui.Row { anchors = { right = true, right_margin = 12, vertical_center = true } }
    local cap
    local function shortcut(r)
      if cap then ui.destroy(cap, true) cap = nil end
      if r.shortcut and r.shortcut ~= "" and kit.keycap then
        cap = kit.keycap { text = r.shortcut, height = 22 }
        ui.reparent(cap, caps)
      end
    end
    area = ui.MouseArea { id = id and (id .. "-command-" .. math.floor(row.index)) or nil, width = LW,
      height = function() return now().height or CMD_H end, cursor = "pointer",
      accessible_role = "list_item", accessible_name = function() return now().title or "" end,
      on_clicked = function() run(index_now()) end,
      -- (The hover wash: the highlight is the Sdf under the rows.)
      kit.surface { anchors = { fill = true, margins = 2 }, radius = kit.round and kit.round(10) or 10,
        color = function()
          local c = kit.ink("hi")()
          return c:alpha((area and area.hovered and index_now() ~= highlight:get()) and 0.06 or 0)
        end },
      icon,
      ui.Column { x = 44, anchors = { vertical_center = true }, gap = 1,
        kit.text { width = LW - 44 - 120, elide = "right", text = function() return now().title or "" end,
          color = hi },
        kit.label { width = LW - 44 - 120, elide = "right", visible = has_sub,
          text = function() return now().subtitle or "" end } },
      caps }
    shortcut(row)
    return area, function(next_row)
      rebind(next_row)
      if id then area.id = id .. "-command-" .. math.floor(next_row.index) end
      shortcut(next_row)
    end
  end

  local list
  list, list_handle = collection.make("list", {
    id = id and (id .. "-list") or nil, rows = model, width = LW, height = LIST_H, row_height = CMD_H,
    size_field = "height", kind_field = "kind", focus_policy = "click",
    accessible_name = "Commands",
    current = function() layout:get() return highlight:get() end,
    disabled = function()
      local out = {}
      for n in pairs(layout:get().headers) do out[#out + 1] = n end
      table.sort(out)
      return out
    end,
    on_current_changed = function(n) highlight:set(n) end,
    on_activated = function(n) run(n) end,
    delegate = delegate,
  })
  -- The highlight: one rounded field riding the current row, which springs
  -- from row to row (and stretches on the way, where the theme does).
  local track = ui.Item { x = 0, width = LW,
    y = function()
      local l, n = layout:get(), highlight:get()
      return (l.tops[n] or 0) - list_handle.offset()
    end,
    height = function() local l = layout:get() return l.heights[highlight:get()] or CMD_H end,
    behavior = { y = ui.spring { stiffness = 520, damping = 30 }, height = ui.spring { stiffness = 520, damping = 30 } } }
  local wash = kit.selection and kit.selection { track = track,
    color = function() return kit.signal("accent")():alpha(0.16) end } or nil
  local empty = kit.label { id = id and (id .. "-empty") or nil, x = 12, y = 14, width = LW - 24, text = spec.empty_text or "No matching commands",
    visible = function() return layout:get().count == 0 end }
  local body = ui.Item { x = PAD, y = PAD + FIELD_H + PAD, width = LW, height = fit, clip = true,
    ui.Item { anchors = { fill = true }, visible = function() return highlight:get() > 0 end, track, wash },
    list, empty }
  content = ui.Item { width = W, height = H, z = 1, field,
    kit.separator and kit.separator { x = PAD, y = PAD + FIELD_H + 3, width = LW } or nil, body }
  return content
  end
  refilter("")

  local handle = { model = model }
  local function built()
    build()
    handle.input, handle.list = input, list_handle
  end
  local function reset()
    input.text = ""
    query:set("")
    refilter("")
  end
  local node
  if spec.inline then
    built()
    frame = ui.Item { id = id, x = spec.x, y = spec.y, width = W, height = H, anchors = spec.anchors,
      kit.card { anchors = { fill = true } }, content }
    node = frame
    handle.open = function() morf.focus.set(input, true) end
    handle.close = function() end
    handle.toggle = handle.open
    handle.is_open = function() return true end
  else
    local function ensure()
      if menu then return menu end
      built()
      menu = popup.make("command_palette", {
        id = id and (id .. "-popup") or nil, content = content, width = W, height = H,
        root = spec.root, placement = spec.placement or (spec.anchor and "bottom" or "center"),
        close_policy = "escape+outside", modal = spec.modal, dim = spec.dim,
        on_opened = spec.on_opened,
        on_closed = function(reason) if spec.on_closed then spec.on_closed(reason) end end,
      })
      handle.popup = menu
      return menu
    end
    local function open(anchor)
      if is_open() then return end
      ensure()
      reset()
      menu.open(anchor or spec.anchor)
      -- Typing goes to the field at once.
      morf.timer(1, function() if is_open() then morf.focus.set(input, true) end end, false)
    end
    handle.open = open
    handle.close = close
    handle.toggle = function(anchor) if is_open() then close("closed") else open(anchor) end end
    handle.is_open = is_open
    -- Where it is called up from: Ctrl+K and Ctrl+Shift+P anywhere on the
    -- root's surface.
    if spec.root and spec.shortcuts ~= false then
      local keys = { scope = "surface" }
      keys["ctrl+k"] = function() handle.toggle() end
      keys["ctrl+shift+p"] = function() open() end
      node = ui.Item { id = id, width = 1, height = 1, shortcuts = keys }
      ui.reparent(node, spec.root)
    end
  end
  handle.node = node
  handle.query = function() return query:get() end
  handle.highlight = function() return highlight:get() end
  -- The commands shown, by their indices in `commands`, in order.
  handle.shown = function()
    local out = {}
    for n = 1, model:len() do local r = model:get(n) if r.kind == "command" then out[#out + 1] = math.floor(r.index) end end
    return out
  end
  handle.run = run
  handle.filter = function(text) if input then input.text = text end query:set(text) refilter(text) end
  return node, handle
end

return { make = make }
