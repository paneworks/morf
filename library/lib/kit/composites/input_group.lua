-- An input group: a text field with parts before and after it, on one
-- ground -- "−" [value] "+", "https://" [address] "Go" (TextField + Press).
--
--     local node, input = composites.input_group {
--       id = "url", width = 360, height = 40,
--       prefix = "https://",                                  -- a label
--       suffix = { label = "Go", on_clicked = go },           -- a button
--       placeholder = "example.org", on_accepted = go,
--     }
--
-- `prefix` and `suffix` are a part or a list of them: a string is a label,
-- a table with `on_clicked` a button (`label` makes a filled button,
-- `icon` alone an icon button; `id`, `width`, `auto_repeat`), a node is
-- placed as it is. The field takes a text field's own fields (`widget`
-- -- "entry", "search", "numeric_entry", ... --, `text`, `placeholder`,
-- `validator`, `on_edited` (each edit; `on_text_changed` is heard twice), `on_accepted`, `on_escape`,
-- `on_key_pressed`, ...); its id is the group's. `variant = "select"`
-- leaves the ground off (a row draws its own). Returns the group's node,
-- the text input, and the field's live state and control.
--
-- `style(props)` gives a text input the theme's type and inks (its face
-- read off a kit text, its colours the kit's inks), for the composites
-- that make inputs of their own.
local ui = require("morf.ui")
local control = require("lib.kit.control")
local text_field = require("lib.kit.text_field")

local M = {}

local function K() local ok, kit = pcall(require, "kit") return ok and type(kit) == "table" and kit or {} end

-- The theme's face, read once per theme off a kit text.
local faces = {}
local function face(kit)
  local key = require("lib.kit.skin").current()
  if faces[key] then return faces[key] end
  local out = {}
  if kit.text then
    local probe = kit.text { text = "" }
    out.font_family, out.font_source, out.font_size = probe.font_family, probe.font_source, probe.font_size
    ui.destroy(probe, true)
  end
  faces[key] = out
  return out
end

--- Fills a text input's type and colours from the theme's kit.
function M.style(props)
  local kit = K()
  local f = face(kit)
  for k, v in pairs(f) do if props[k] == nil and v ~= nil and v ~= "" then props[k] = v end end
  if kit.ink then
    props.color = props.color or kit.ink("hi")
    props.placeholder_color = props.placeholder_color or kit.ink("lo")
    props.caret_color = props.caret_color or kit.ink("accent")
    if props.selection_color == nil and kit.stroke then props.selection_color = kit.stroke("faint", kit.ink("accent")) end
  end
  props.vertical_alignment = props.vertical_alignment or "center"
  return props
end

local function list(v)
  if v == nil then return {} end
  if type(v) == "string" or type(v) == "userdata" then return { v } end
  if type(v) == "table" and (v.label or v.icon or v.on_clicked or v.text) then return { v } end
  return v
end

-- One part beside the field.
local function part(p, H, kit, n, id)
  if type(p) == "userdata" then return p end
  if type(p) == "string" or (type(p) == "table" and p.on_clicked == nil) then
    local text = type(p) == "string" and p or (p.text or p.label)
    if p.icon and type(p) == "table" and not text then
      return kit.icon and kit.icon(p.icon, 18, kit.ink and kit.ink("lo")) or ui.Item {}
    end
    return kit.text and kit.text { text = text, color = kit.ink and kit.ink("lo"),
      layout = { shrink = 0 } } or ui.Text { text = text }
  end
  local pid = p.id or (id and (id .. "-part-" .. n)) or nil
  local h = H - 8
  if p.label then
    return (control.make("Press", p.widget or "pill", { widget = p.widget or "pill", id = pid, label = p.label,
      icon = p.icon, width = p.width or math.max(h, 24 + 9 * utf8.len(p.label)), height = h,
      on_clicked = p.on_clicked, auto_repeat = p.auto_repeat, accessible_name = p.accessible_name }))
  end
  return (control.make("Press", "icon", { widget = "icon", id = pid, icon_off = p.icon, icon_on = p.icon,
    width = p.width or h, height = h, size = math.floor(h * 0.6), on_clicked = p.on_clicked,
    auto_repeat = p.auto_repeat, accessible_name = p.accessible_name or p.icon }))
end

function M.make(spec)
  spec = spec or {}
  local kit = K()
  local W, H = spec.width or 300, spec.height or 40
  local before, after = list(spec.prefix), list(spec.suffix)
  local props = {}
  for k, v in pairs(spec) do props[k] = v end
  for _, k in ipairs { "prefix", "suffix", "variant", "x", "y", "width", "anchors", "icon", "widget",
    "on_clicked" } do props[k] = nil end
  props.id = spec.id
  props.height = H
  props.inset = spec.inset or { (#before == 0 and not spec.icon) and 12 or 4, 0, 4, 0 }
  props.layout = { grow = 1, minimum_width = 0 }
  M.style(props)
  local field_node, input, t, ctl = text_field.make(spec.widget or "entry", props)
  local row = { direction = "row", align = "center", gap = 4, width = W, height = H, padding = 4 }
  local n = 0
  if #before > 0 or spec.icon then row[#row + 1] = ui.Item { width = 4, height = 1 } end
  if spec.icon and kit.icon then
    row[#row + 1] = kit.icon(spec.icon, 18, kit.ink and kit.ink("lo"))
  end
  for _, p in ipairs(before) do n = n + 1 row[#row + 1] = part(p, H, kit, n, spec.id) end
  row[#row + 1] = field_node
  for _, p in ipairs(after) do n = n + 1 row[#row + 1] = part(p, H, kit, n, spec.id) end
  local children = {}
  if spec.variant ~= "select" and kit.card then children[#children + 1] = kit.card { anchors = { fill = true } } end
  children[#children + 1] = ui.Flex(row)
  local node = ui.Item { x = spec.x, y = spec.y, width = W, height = H, anchors = spec.anchors,
    table.unpack(children) }
  return node, input, t, ctl
end

return M
