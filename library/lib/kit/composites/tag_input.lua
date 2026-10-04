-- A tag input: a text field that turns what is typed into removable chips
-- (TextField + Press).
--
--     local node, tags = composites.tag_input {
--       id = "labels", width = 420, tags = { "lua", "ui" },
--       placeholder = "Add a tag", on_changed = function(list) end,
--       suggestions = nil, unique = true,
--     }
--     tags.add("rust") tags.remove(1) tags.list()
--
-- Return or a comma adds what is typed; Backspace in an empty field
-- removes the last chip; a press on a chip (or Return/Space on it) removes
-- it. Chips are `<id>-tag-<index>`. Chips wrap onto new lines as the
-- width fills; `height` is one line's (40) and the node grows with them.
local ui = require("morf.ui")
local control = require("lib.kit.control")
local text_field = require("lib.kit.text_field")
local group = require("lib.kit.composites.input_group")

local function K() local ok, kit = pcall(require, "kit") return ok and type(kit) == "table" and kit or {} end

local serial = 0

local function make(spec)
  spec = spec or {}
  local kit = K()
  serial = serial + 1
  local id = spec.id
  local W, H = spec.width or 400, spec.height or 40
  local CH = H - 12
  local initial = {}
  for i, t in ipairs(spec.tags or {}) do initial[i] = t end
  local tags = morf.signal("kit.composites.tag_input." .. tostring(id) .. "." .. serial, initial)
  local input, rebuild
  local edits, seen = 0, 0
  local function changed(list) if spec.on_changed then spec.on_changed(list) end end
  local function add(text)
    text = tostring(text or ""):gsub("^%s+", ""):gsub("%s+$", "")
    if text == "" then return false end
    local list = {}
    for i, t in ipairs(tags:get()) do
      if spec.unique ~= false and t == text then return false end
      list[i] = t
    end
    list[#list + 1] = text
    tags:set(list)
    rebuild()
    changed(list)
    return true
  end
  local function remove(index)
    local list = {}
    for i, t in ipairs(tags:get()) do if i ~= index then list[#list + 1] = t end end
    tags:set(list)
    rebuild()
    changed(list)
  end
  local props = group.style {
    id = id, height = CH, width = 140, layout = { grow = 1, minimum_width = 120 },
    placeholder = spec.placeholder or "Add…", inset = { 4, 0, 4, 0 },
    on_edited = function(text)
      edits = edits + 1
      -- A comma ends a tag.
      if text:find(",", 1, true) then
        local rest = text:match("([^,]*)$") or ""
        for part in text:gmatch("([^,]*),") do add(part) end
        input.text = rest
      end
      if spec.on_edited then spec.on_edited(text) end
    end,
    on_accepted = function(text) if add(text) then input.text = "" end end,
    -- Backspace in a field that was already empty takes the last chip:
    -- the field keeps the key, so it is heard as it comes up, when no
    -- edit came of it.
    on_key_released = function(_, _, _, _, name)
      if name == "BackSpace" and (input.text or "") == "" and edits == seen then
        local n = #tags:get()
        if n > 0 then remove(n) end
      end
      seen = edits
    end,
  }
  local field_node
  field_node, input = text_field.make("entry", props)
  -- The chips: a Press each, a press removes it.
  local flow = { direction = "row", wrap = true, gap = 6, align = "center", padding = 6,
    width = W }
  local chips_holder = ui.Flex(flow)
  local built = {}
  function rebuild()
    for _, chip in ipairs(built) do ui.destroy(chip, true) end
    built = {}
    for i, t in ipairs(tags:get()) do
      local chip = control.make("Press", "chip_input", { widget = "chip_input", id = id and (id .. "-tag-" .. i) or nil,
        label = t, icon = "close", height = CH, width = math.min(W - 24, 44 + 8 * utf8.len(t)),
        accessible_name = "Remove " .. t, on_clicked = function() remove(i) morf.focus.set(input, false) end })
      built[#built + 1] = chip
      ui.reparent(chip, chips_holder)
    end
    -- The field after the chips.
    ui.reparent(field_node, chips_holder)
  end
  rebuild()
  local ground = kit.card and kit.card { anchors = { fill = true } } or nil
  local node = ui.Item { x = spec.x, y = spec.y, width = W,
    height = function() return math.max(H, chips_holder.layout_height or H) end, ground, chips_holder }
  local handle = { node = node, input = input, add = add, remove = remove, list = function() return tags:get() end }
  return node, handle
end

return { make = make }
