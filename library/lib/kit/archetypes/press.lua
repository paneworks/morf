-- The Press widgets (contract.lua): a press with the settings that make it
-- what it is -- a toggle button is a checkable press, a check button in a
-- `group` is a radio button, a menu item of `kind = "check"` toggles -- and
-- the theme's skin for it. `variant` is the widget's style classes, a
-- string ("suggested pill") or a list.
--
--     local press = require("lib.kit.archetypes.press")
--     local node = press.make("button", { label = "Apply", variant = "suggested", on_clicked = apply })
--
-- `caption` lays a button's legend out between its adornments; `fade` dims
-- a disabled press whole, caption, icon and ground together.
local ui = require("morf.ui")
local control = require("lib.kit.control")

local M = {}

-- What each widget sets unless its spec says otherwise.
local DEFAULTS = {
  toggle_button = { checkable = true },
  switch = { checkable = true, width = 52, height = 32 },
  check_button = { checkable = true, tristate = true },
}

--- Whether `spec` carries the style class `name`.
function M.has(spec, name)
  local v = spec.variant
  if type(v) == "function" then v = v() end
  if type(v) == "string" then return (" " .. v .. " "):find(" " .. name .. " ", 1, true) ~= nil end
  if type(v) == "table" then
    for _, item in ipairs(v) do if item == name then return true end end
  end
  return false
end

--- A press of `widget` (button, toggle_button, check_button, switch,
--- link_button, menu_item, tab_button) built from `spec`.
function M.make(widget, spec)
  local merged = {}
  for k, v in pairs(DEFAULTS[widget] or {}) do merged[k] = v end
  for k, v in pairs(spec or {}) do merged[k] = v end
  merged.widget = widget
  if widget == "check_button" then
    if merged.group ~= nil then
      merged.exclusive, merged.tristate = true, false
      merged.accessible_role = merged.accessible_role or "radio_button"
    end
    if merged.label == nil then
      merged.width, merged.height = merged.width or 24, merged.height or 24
    end
  elseif widget == "menu_item" then
    if merged.kind == "check" or merged.kind == "radio" then
      merged.checkable = true
      merged.accessible_role = merged.accessible_role or (merged.kind == "check" and "menu_item_check" or "menu_item_radio")
    end
    if merged.kind == "radio" then merged.exclusive = true end
  elseif widget == "button" and M.has(merged, "circular") then
    merged.width, merged.height = merged.width or 34, merged.height or 34
  end
  -- A group's name without `exclusive` still makes radio buttons one group.
  if merged.group == nil and merged.exclusive then merged.group = "default" end
  return (control.make("Press", widget, merged))
end

-- ---------------------------------------------------------------- caption --

local preferred = setmetatable({}, { __mode = "k" })
local function get(v) if type(v) == "function" then return v() end return v end

--- How wide a themed caption wants to be, adornments included, even while
--- the caption shown is constrained and elided.
function M.preferred_width(node)
  return preferred[node] and preferred[node]() or nil
end

--- A bounded button legend: `label(width)` builds the label to fit the
--- room the `before` and `after` adornments leave; `measure` is the label
--- at its natural width (drawn invisibly), `gap`, `left`, `right` the
--- spacing, `align = "start"` packs it from the start.
function M.caption(spec)
  local gap = spec.gap or 8
  local left, right = spec.left or 12, spec.right or 12
  local before, after = spec.before or {}, spec.after or {}
  local function has_label() return spec.label ~= nil and get(spec.label_visible) ~= false end
  local function extras()
    local width, n = 0, has_label() and 1 or 0
    for _, list in ipairs { before, after } do
      for _, node in ipairs(list) do
        if node.visible ~= false then width, n = width + (node.layout_width or 0), n + 1 end
      end
    end
    return width + math.max(0, n - 1) * gap
  end
  local function natural() return has_label() and spec.measure and (spec.measure.layout_width or 0) or 0 end
  local function wanted() return natural() + extras() + left + right end
  local function width() return math.max(1e-3, get(spec.width) or wanted()) end
  local function label_width() return math.max(1e-3, math.min(natural(), width() - left - right - extras())) end
  local row = { gap = gap, align = "center", anchors = { vertical_center = true } }
  for _, node in ipairs(before) do row[#row + 1] = node end
  if spec.label then
    local label = spec.label(label_width)
    label.visible = has_label
    row[#row + 1] = label
  end
  for _, node in ipairs(after) do row[#row + 1] = node end
  row = ui.Row(row)
  row.x = function()
    return left + (spec.align == "start" and 0 or math.max(0, (width() - left - right - (row.layout_width or 0)) / 2))
  end
  local node = ui.Item { anchors = { center_in = true }, width = width, height = spec.height, clip = true, row,
    spec.measure and ui.Item { width = 1, height = 1, clip = true, spec.measure } or nil }
  preferred[node] = wanted
  return node
end

-- ------------------------------------------------------------------- fade --

--- Wraps every Press skin in `skins` so a disabled press fades whole to
--- `opacity`; a configuration's own `opacity` keeps its presentation.
function M.fade(skins, widgets, opacity)
  local names = { "Press" }
  for _, name in ipairs(widgets) do names[#names + 1] = name end
  for _, name in ipairs(names) do
    local draw = skins[name]
    if type(draw) == "function" then
      skins[name] = function(t, spec, node, ...)
        if node and spec.opacity == nil then
          node.opacity = function() return t.enabled == false and opacity or 1 end
        end
        return draw(t, spec, node, ...)
      end
    end
  end
end

return M
