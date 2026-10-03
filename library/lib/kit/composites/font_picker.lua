-- A font picker (composite: TextField search + Collection + preview).
--
--     local node, picker = composites.font_picker {
--       id = "font", width = 360, height = 320,
--       fonts = { "Inter", "IBM Plex Mono", ... },   -- or a binding
--       value = function() return font:get() end,
--       on_picked = function(family) font:set(family) end,
--     }
--
-- A search field (a kit `search` entry) over the families, a kit list (a
-- Collection: the arrows walk it, typing jumps, Return or a press picks)
-- whose rows set each family's name in the theme's face and a line in
-- the family itself, and under them the family the list is on in a
-- larger sample (`sample = false` leaves it out). The families are
-- `fonts`, or `morf.text.families()` where the engine lists them. The
-- search ranks them fuzzily (`filter = false` leaves the list as given
-- and only calls `on_search(query)` -- a configuration that pages its own
-- list); Return in the search picks the first. An empty name keeps a
-- row's place, empty. `status` (a binding) is said while the list is
-- empty. Ids: `<id>-search`, `<id>-list`, `<id>-option-<i>`,
-- `<id>-preview-<i>`, `<id>-sample`.
local ui = require("morf.ui")
local widgets = require("lib.kit.widgets")

local function get(v) if type(v) == "function" then return v() end return v end

local function installed()
  local text = morf.text
  if type(text) == "table" and type(text.families) == "function" then
    local ok, list = pcall(text.families)
    if ok and type(list) == "table" then return list end
  end
  return { "sans-serif", "serif", "monospace" }
end

return function(spec)
  spec = spec or {}
  local kit = require("kit")
  local id = spec.id
  local W, H = spec.width or 360, spec.height or 320
  local ROW = spec.row_height or 46
  local SEARCH = 40
  local SAMPLE = spec.sample == false and 0 or (spec.sample_height or 64)
  local LIST = H - SEARCH - 8 - (SAMPLE > 0 and SAMPLE + 8 or 0)
  local preview = spec.preview or "The quick brown fox jumps over the lazy dog"
  local st = morf.state { query = "", cursor = get(spec.value) or "" }
  local own = spec.fonts == nil and installed() or nil
  local function fonts() return own or get(spec.fonts) or {} end
  local function shown()
    local list = fonts()
    if spec.filter == false or st.query == "" then return list end
    local out = {}
    for _, hit in ipairs(morf.text.fuzzy(st.query, list)) do out[#out + 1] = hit.item end
    return out
  end
  local rows = morf.list_model({})
  local function refill()
    local out = {}
    for i, family in ipairs(shown()) do out[i] = { key = tostring(i), slot = i, family = family, label = family } end
    rows:replace(out, "key")
  end
  refill()
  local function family(i) local r = rows:get(i) return r and r.family or nil end
  local function pick(name)
    if not name or name == "" then return end
    st.cursor = name
    if spec.on_picked then spec.on_picked(name) end
  end
  local function blanks()
    local out = {}
    for i = 1, rows:len() do if family(i) == "" then out[#out + 1] = i end end
    return out
  end

  local list_node
  local function handle_list() return list_node end
  local probe = { text = "" }
  ui.destroy(kit.text(probe), true)
  local focused = morf.state { on = false }
  local search_node, search = widgets.search { id = id and (id .. "-search"), accessible_name = "Search fonts",
    x = 10, width = W - 20, height = SEARCH, inset = { 0, 0, 34, 0 },
    placeholder = spec.placeholder or "Search fonts…",
    font_family = probe.font_family, font_source = probe.font_source, font_size = probe.font_size,
    color = kit.ink("hi"), placeholder_color = kit.ink("lo"), caret_color = kit.signal("accent"),
    selection_color = function() return kit.signal("accent")():alpha(0.3) end, vertical_alignment = "center",
    focus = spec.focus,
    on_focus_changed = function(on) focused.on = on end,
    on_text_changed = function(text)
      st.query = text
      if spec.on_search then spec.on_search(text) end
    end,
    on_accepted = function()
      local first
      for i = 1, rows:len() do if (family(i) or "") ~= "" then first = family(i) break end end
      pick(first)
    end,
    on_escape = spec.on_escape,
    -- Down goes on to the list, onto its first family.
    on_key_pressed = function(_, _, _, _, key)
      if key ~= "Down" and key ~= "Page_Down" then return false end
      if rows:len() == 0 then return false end
      local found = false
      for i = 1, rows:len() do if family(i) == st.cursor then found = true break end end
      if not found then
        for i = 1, rows:len() do if (family(i) or "") ~= "" then st.cursor = family(i) break end end
      end
      morf.focus.set(handle_list(), true)
      return true
    end }
  local field = kit.field { width = W, height = SEARCH, focused = function() return focused.on end, search_node }

  local list
  list_node, list = widgets.list { id = id and (id .. "-list"), accessible_name = "Fonts",
    y = SEARCH + 8, width = W, height = LIST, rows = rows, row_height = ROW,
    current = function()
      for i = 1, rows:len() do if family(i) == st.cursor then return i end end
      return 0
    end,
    disabled = blanks,
    on_current_changed = function(i) local f = family(i) if f and f ~= "" then st.cursor = f end end,
    on_activated = function(i) pick(family(i)) end,
    delegate = function(row, s)
      local function now() return s.row() or row end
      local function name() return now().family or "" end
      local area, sample
      sample = kit.text { id = id and (id .. ("-preview-%d"):format(row.slot)), x = 12, y = ROW / 2, width = W - 24,
        height = ROW / 2 - 4, elide = "right", font_source = "",
        font_family = function() return name() ~= "" and name() or probe.font_family end,
        text = preview, color = kit.ink("accent") }
      area = ui.MouseArea { id = id and (id .. ("-option-%d"):format(row.slot)), width = W, height = ROW, cursor = "pointer",
        visible = function() return name() ~= "" end,
        accessible_role = "list_item", accessible_name = name,
        on_clicked = function() pick(name()) end,
        kit.surface { anchors = { fill = true, margins = 2 }, radius = kit.round(10),
          color = function()
            local c = kit.signal("accent")()
            if s.current() then return c:alpha(0.18) end
            return c:alpha(area and area.hovered and 0.08 or 0)
          end },
        kit.text { x = 12, y = 4, width = W - 24, height = 18, elide = "right", font_size = spec.name_size,
          text = name, color = kit.ink("hi") },
        sample }
      return area, function(next_row)
        if id then
          area.id = id .. ("-option-%d"):format(next_row.slot)
          sample.id = id .. ("-preview-%d"):format(next_row.slot)
        end
      end
    end }
  local root
  root = ui.Item { width = W, height = H, field, list_node,
    kit.subtitle { x = 12, y = SEARCH + 20, width = W - 24, height = 40, wrap = true,
      visible = function() return rows:len() == 0 end,
      text = function() local s = get(spec.status) return (s and s ~= "") and s or "No matching fonts" end } }
  morf.effect("kit.font_picker.rows." .. tostring(root), refill, { owner = root })
  -- The family the list is on, or its first.
  local function sampled()
    if st.cursor ~= "" then return st.cursor end
    for i = 1, rows:len() do if (family(i) or "") ~= "" then return family(i) end end
    return ""
  end
  if SAMPLE > 0 then
    ui.reparent(kit.surface { y = H - SAMPLE, width = W, height = SAMPLE, radius = kit.round(12),
      color = kit.stroke("faint"),
      kit.text { id = id and (id .. "-sample"), x = 14, anchors = { vertical_center = true }, width = W - 28,
        elide = "right", font_size = 22, font_source = "",
        font_family = function() local f = sampled() return f ~= "" and f or probe.font_family end,
        text = function() return sampled() ~= "" and (spec.sample_text or "Aa Bb Cc 0123") or "" end,
        color = kit.ink("hi") } }, root)
  end
  for _, k in ipairs { "x", "y", "anchors", "visible", "z" } do if spec[k] ~= nil then root[k] = spec[k] end end
  if type(spec.value) == "function" then
    morf.effect("kit.font_picker.value." .. tostring(root), function()
      local v = spec.value()
      if v and v ~= "" then st.cursor = v end
    end, { owner = root })
  end
  local handle = { search = search, list = list, rows = rows }
  function handle.query() return st.query end
  function handle.cursor() return st.cursor end
  function handle.clear() search.text = "" st.query = "" end
  function handle.focus() search.focus = true end
  return root, handle
end
