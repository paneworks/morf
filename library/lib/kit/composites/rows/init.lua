-- What every boxed-list row is made of: its frame -- a leading icon or
-- control, the title and the subtitle under it, a suffix at its end -- and
-- the press it is when it is activatable (the Press skin `action_row`
-- draws its wash and ring). The rows themselves are the modules beside
-- this one; `build(row, width)` makes a row from a spec with `kind`, for a
-- group or an expander row given specs.
local ui = require("morf.ui")
local control = require("lib.kit.control")
local widgets = require("lib.kit.widgets")
local has = require("lib.kit.archetypes.press").has

local M = {}

M.PAD = 12
local serial = 0

function M.get(v) if type(v) == "function" then return v() end return v end

--- A unique key for one of a row's own signals.
function M.key(spec, what)
  serial = serial + 1
  return ("kit.composites.rows.%s.%s.%d"):format(what, tostring(spec.id or "anon"), serial)
end

function M.height_of(spec) return spec.height or ((spec.subtitle and spec.subtitle ~= "") and 64 or 52) end

-- The title and the subtitle, in `width` from `x`; the `property` variant
-- dims the title and gives the subtitle the body's weight.
local function titles(spec, x, width)
  local property = has(spec, "property")
  local col = { width = width, gap = 2, anchors = { left = true, left_margin = x, vertical_center = true },
    widgets.label { text = spec.title or "", width = width, elide = "right",
      variant = property and "caption dim" or nil } }
  if spec.subtitle and spec.subtitle ~= "" then
    col[#col + 1] = widgets.label { text = spec.subtitle, width = width, elide = "right",
      variant = not property and "caption dim" or nil }
  end
  return ui.Column(col)
end

--- A row: its leading icon or `lead` (a node `lead_w` wide), its titles,
--- and `suffix` (a node `suffix_w` wide) at its end. A press when
--- `on_clicked` is given; a plain item otherwise.
function M.frame(spec, parts)
  local W, H = spec.width or 360, M.height_of(spec)
  local PAD = M.PAD
  local children = {}
  local x = PAD
  local lead = parts.lead
  if not lead and spec.icon then lead = widgets.image { icon = spec.icon, pixel_size = 20 } end
  if lead then
    lead.anchors = { left = true, left_margin = PAD, vertical_center = true }
    children[#children + 1] = lead
    x = PAD + (parts.lead_w or 20) + 12
  end
  local sw = parts.suffix_w or 0
  local tw = math.max(1, W - x - PAD - (sw > 0 and (sw + 12) or 0))
  if not parts.no_titles then children[#children + 1] = titles(spec, x, tw) end
  if parts.suffix then
    parts.suffix.anchors = { right = true, right_margin = PAD, vertical_center = true }
    children[#children + 1] = parts.suffix
  end
  for _, extra in ipairs(parts.extra or {}) do children[#children + 1] = extra end
  if parts.on_clicked then
    return (control.make("Press", "action_row", { widget = "action_row", id = spec.id, width = W, height = H,
      cursor = "pointer", on_clicked = parts.on_clicked, accessible_name = spec.accessible_name or spec.title },
      { children = children }))
  end
  local props = { id = spec.id, width = W, height = H, accessible_role = "group", accessible_name = spec.title }
  for i, child in ipairs(children) do props[i] = child end
  return ui.Item(props)
end

--- A boolean that is the caller's (a binding, `spec.active`) or the row's
--- own; setting it tells `spec.on_toggled(on)`.
function M.flag(spec, name)
  local own = morf.signal(M.key(spec, name), M.get(spec.active) == true)
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

--- A row node from `row`: a node as it is, a spec with `kind` built at
--- `width`.
function M.build(row, width)
  if type(row) ~= "table" or not row.kind then return row end
  local s = {}
  for k, v in pairs(row) do s[k] = v end
  s.width = s.width or width
  return (require("lib.kit.composites")[row.kind](s))
end

return M
