-- A shortcuts window: an application's keys, grouped, with a search that
-- narrows them, in a dialog (Popup + Collection, grouped + TextField).
--
--     local node, keys = composites.shortcuts_window {
--       id = "keys", title = "Keyboard shortcuts", root = surface_root,
--       groups = {
--         { title = "General", shortcuts = {
--           { keys = "Ctrl+K", description = "Command palette" },
--           { keys = "Ctrl+K Ctrl+S", description = "Save all" },      -- a sequence
--         } },
--       },
--     }
--     keys.open() keys.close() keys.is_open() keys.filter("save")
--
-- Each shortcut's `keys` is a chord (`"Ctrl+Shift+P"`), a sequence of
-- chords apart by spaces, or a list of either (alternatives); every key is
-- drawn as the theme's keycap. The search ranks the shortcuts fuzzily by
-- description and keys and keeps them under their groups. Escape closes
-- the window and focus goes back to what opened it; the list scrolls with
-- the wheel and walks with the arrows once it has focus (Down from the
-- search goes to it). `open_key` (e.g. `"ctrl+?"`) on `root` opens it.
-- `inline = true` returns the window as a node to place. Other fields:
-- `width` (480), `height` (440), `placeholder`, `on_closed(reason)`. Ids:
-- `<id>-search`, `<id>-list`, `<id>-row-<g>-<n>` (group, shortcut),
-- `<id>-close`, `<id>-popup`.
local ui = require("morf.ui")
local popup = require("lib.kit.popup")
local collection = require("lib.kit.collection")
local control = require("lib.kit.control")
local group_field = require("lib.kit.composites.input_group")

local function K() local ok, kit = pcall(require, "kit") return ok and type(kit) == "table" and kit or {} end
local function get(v) if type(v) == "function" then return v() end return v end

--- The chords of `keys`: a list of alternatives, each a list of chords,
--- each a list of key names.
local function chords(keys)
  local alternatives = keys
  if type(keys) ~= "table" then
    alternatives = {}
    for alt in tostring(keys or ""):gmatch("[^|]+") do alternatives[#alternatives + 1] = alt end
  end
  local out = {}
  for _, alt in ipairs(alternatives) do
    local sequence = {}
    for chord in tostring(alt):gmatch("%S+") do
      local parts = {}
      -- "Ctrl++" is Ctrl and the plus key.
      local rest = chord
      if rest:sub(-2) == "++" then rest = rest:sub(1, -3) .. "+Plus" end
      for part in rest:gmatch("[^+]+") do parts[#parts + 1] = part == "Plus" and "+" or part end
      sequence[#sequence + 1] = parts
    end
    out[#out + 1] = sequence
  end
  return out
end

local function text_of(keys)
  return type(keys) == "table" and table.concat(keys, "|") or tostring(keys or "")
end

local serial = 0

local function make(spec)
  spec = spec or {}
  local kit = K()
  serial = serial + 1
  local key = "kit.composites.shortcuts_window." .. tostring(spec.id) .. "." .. serial
  local id = spec.id
  local W, H = spec.width or 480, spec.height or 440
  local PAD, TITLE_H, FIELD_H = 12, 40, 40
  local ROW_H, HEAD_H = 40, 32
  local LW = W - 2 * PAD
  local LIST_Y = PAD + TITLE_H + FIELD_H + PAD
  local LIST_H = H - LIST_Y - PAD
  local function groups() return get(spec.groups) or {} end
  local model = morf.list_model({})
  local count = morf.signal(key .. ".count", 0)
  local query = morf.signal(key .. ".query", "")

  local function refilter(text)
    local flat, labels = {}, {}
    for g, grp in ipairs(groups()) do
      for n, sc in ipairs(grp.shortcuts or {}) do
        flat[#flat + 1] = { g = g, n = n, sc = sc }
        labels[#labels + 1] = tostring(sc.description or "") .. " " .. text_of(sc.keys)
      end
    end
    local hits = morf.text.fuzzy(text or "", labels, { limit = 500 })
    local by_group, order = {}, {}
    for _, hit in ipairs(hits) do
      local entry = flat[hit.index]
      if not by_group[entry.g] then by_group[entry.g] = {} order[#order + 1] = entry.g end
      local list = by_group[entry.g]
      list[#list + 1] = entry
    end
    -- Without a query the groups keep their own order.
    if (text or "") == "" then table.sort(order) end
    local rows = {}
    for _, g in ipairs(order) do
      local grp = groups()[g]
      rows[#rows + 1] = { key = "group:" .. g, kind = "header", title = tostring(grp.title or ""), height = HEAD_H }
      for _, entry in ipairs(by_group[g]) do
        rows[#rows + 1] = { key = ("row:%d:%d"):format(entry.g, entry.n), kind = "shortcut", g = entry.g, n = entry.n,
          description = tostring(entry.sc.description or ""), keys = text_of(entry.sc.keys), height = ROW_H }
      end
    end
    model:replace(rows, "key")
    local shown = 0
    for _, r in ipairs(rows) do if r.kind == "shortcut" then shown = shown + 1 end end
    count:set(shown)
  end
  refilter("")

  local menu, input, list_handle, list_node, content
  local function is_open() return menu ~= nil and menu.is_open() end
  local function close(reason) if menu then menu.close(reason or "closed") end end

  local function build()
    if content then return content end
    local title = kit.text { x = PAD + 4, font_size = 17, font_weight = 600, color = kit.ink("hi"),
      accessible_role = "heading", y = PAD, height = TITLE_H, vertical_alignment = "center",
      text = spec.title or "Keyboard shortcuts", width = LW - 60, elide = "right" }
    local close_button = not spec.inline and control.make("Press", "icon", { widget = "icon",
      id = id and (id .. "-close") or nil, icon_off = "close", icon_on = "close", width = 34, height = 34, size = 20,
      x = W - PAD - 34, y = PAD + 3, accessible_name = "Close", on_clicked = function() close("closed") end }) or nil
    local field
    field, input = group_field.make {
      id = id and (id .. "-search") or nil, x = PAD, y = PAD + TITLE_H, width = LW, height = FIELD_H,
      widget = "search", icon = "search", placeholder = spec.placeholder or "Search shortcuts…",
      accessible_name = "Search shortcuts",
      on_edited = function(text) query:set(text) refilter(text) end,
      on_escape = function() if not spec.inline then close("escape") end end,
      on_key_pressed = function(_, _, _, _, name)
        if (name == "Down" or name == "Page_Down") and list_node then morf.focus.set(list_node, true) return true end
        return false
      end,
    }
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
        return ui.Item { width = LW, height = HEAD_H,
          kit.label { x = 8, y = 10, width = LW - 16, elide = "right", text = function() return now().title or "" end },
        }, rebind
      end
      local caps = ui.Row { gap = 4, align = "center", anchors = { right = true, right_margin = 8, vertical_center = true } }
      local built = {}
      local function draw(r)
        for _, n in ipairs(built) do ui.destroy(n, true) end
        built = {}
        for a, sequence in ipairs(chords(r.keys)) do
          if a > 1 then built[#built + 1] = kit.label { text = "/" } end
          for c, chord in ipairs(sequence) do
            if c > 1 then built[#built + 1] = kit.label { text = "then" } end
            for _, name in ipairs(chord) do built[#built + 1] = kit.keycap { text = name, height = 24 } end
          end
        end
        for _, n in ipairs(built) do ui.reparent(n, caps) end
      end
      local area = ui.Item { id = id and ("%s-row-%d-%d"):format(id, row.g, row.n) or nil, width = LW, height = ROW_H,
        accessible_role = "list_item",
        accessible_name = function() local r = now() return (r.description or "") .. ", " .. (r.keys or ""):gsub("|", " or ") end,
        kit.surface { anchors = { fill = true, margins = 1 }, radius = kit.round and kit.round(8) or 8,
          color = function() local t = list_handle and list_handle.t return kit.signal("accent")():alpha((t and t.current == index_now() and t.focused) and 0.14 or 0) end },
        kit.text { x = 8, anchors = { vertical_center = true }, width = LW * 0.55, elide = "right",
          text = function() return now().description or "" end, color = kit.ink("hi") },
        caps }
      draw(row)
      return area, function(next_row)
        rebind(next_row)
        if id then area.id = ("%s-row-%d-%d"):format(id, next_row.g, next_row.n) end
        draw(next_row)
      end
    end
    list_node, list_handle = collection.make("list", {
      id = id and (id .. "-list") or nil, rows = model, x = PAD, y = LIST_Y, width = LW, height = LIST_H,
      row_height = ROW_H, size_field = "height", kind_field = "kind", accessible_name = "Shortcuts",
      disabled = function()
        count:get()
        local out = {}
        for n = 1, model:len() do if model:get(n).kind == "header" then out[#out + 1] = n end end
        return out
      end,
      delegate = delegate,
    })
    local empty = kit.label { id = id and (id .. "-empty") or nil, x = PAD + 8, y = LIST_Y + 12, width = LW - 16,
      text = spec.empty_text or "No matching shortcuts", visible = function() return count:get() == 0 end }
    content = ui.Item { width = W, height = H, z = 1, title, close_button, field, list_node, empty }
    return content
  end

  local handle = { model = model }
  local node
  local function built() build() handle.input, handle.list = input, list_handle end
  if spec.inline then
    built()
    node = ui.Item { id = id, x = spec.x, y = spec.y, width = W, height = H, anchors = spec.anchors,
      kit.card { anchors = { fill = true } }, content }
    handle.open, handle.close = function() morf.focus.set(input, true) end, function() end
    handle.is_open = function() return true end
  else
    local function ensure()
      if menu then return menu end
      built()
      menu = popup.make("shortcuts_dialog", {
        id = id and (id .. "-popup") or nil, content = content, width = W, height = H, root = spec.root,
        placement = "center", close_policy = "escape", modal = true, dim = true,
        on_opened = spec.on_opened,
        on_closed = function(reason) if spec.on_closed then spec.on_closed(reason) end end,
      })
      handle.popup = menu
      return menu
    end
    function handle.open(anchor)
      if is_open() then return end
      ensure()
      input.text = ""
      query:set("")
      refilter("")
      menu.open(anchor)
      morf.timer(1, function() if is_open() then morf.focus.set(input, true) end end)
    end
    handle.close = close
    handle.is_open = is_open
    if spec.root and spec.open_key then
      node = ui.Item { id = id, width = 1, height = 1,
        shortcuts = { scope = "surface", [spec.open_key] = function() handle.open() end } }
      ui.reparent(node, spec.root)
    end
  end
  handle.node = node
  handle.toggle = function() if is_open() then close("closed") else handle.open() end end
  handle.query = function() return query:get() end
  handle.count = function() return count:get() end
  -- The descriptions shown, in order.
  handle.shown = function()
    local out = {}
    for n = 1, model:len() do local r = model:get(n) if r.kind == "shortcut" then out[#out + 1] = r.description end end
    return out
  end
  handle.filter = function(text) if input then input.text = text end query:set(text) refilter(text) end
  return node, handle
end

return { make = make }
