-- Every widget the Press, Range, Plane and Selection archetypes make
-- (contract.lua), as a
-- constructor: `widgets.switch { checked = on, on_toggled = set }`,
-- `widgets.slider { value = level, on_moved = set_level }`. A widget is its
-- archetype with the settings that make it what it is -- a switch is a
-- checkable press, a radio button a press in an exclusive group, a seek
-- bar a range whose value waits for the release -- and the theme's skin
-- for it (lib.kit.skin; the skin is told which widget through
-- `spec.widget`).
local control = require("lib.kit.control")
local contract = require("lib.kit.contract")

local M = {}

-- What each widget sets unless its spec says otherwise.
local PRESS = {
  toggle = { checkable = true },
  switch = { checkable = true, width = 52, height = 32 },
  checkbox = { checkable = true, tristate = true, width = 24, height = 24 },
  radio = { checkable = true, exclusive = true, width = 24, height = 24 },
  chip_filter = { checkable = true },
  check_menu_item = { checkable = true },
  radio_menu_item = { checkable = true, exclusive = true },
  toggle_group_member = { checkable = true, exclusive = true },
  segment = { checkable = true, exclusive = true },
  rating_star = { checkable = true },
  repeat_button = { auto_repeat = true },
  disclosure_button = { checkable = true },
}
local RANGE = {
  vertical_slider = { orientation = "vertical" },
  fader = { orientation = "vertical" },
  range_slider = { range = true },
  discrete_slider = { snap = "always", step = 1 },
  stepped_knob = { snap = "always", step = 1, drag_mode = "vertical" },
  rating = { snap = "always", step = 1, from = 0, to = 5 },
  log_slider = { logarithmic = true },
  angle_slider = { wrap = true, from = 0, to = 360, drag_mode = "angular", angle_from = 0, angle_sweep = 360 },
  knob = { drag_mode = "vertical" },
  bipolar_knob = { from = -1, to = 1, value = 0, drag_mode = "vertical" },
  zoom = { from = 0.25, to = 4, value = 1, logarithmic = true },
  spin_button = { step = 1, snap = "always" },
  seek_bar = { live = false },
  scroll_bar = { wheel = false },
  osd_level = { enabled = false },
}

local PLANE = {
  hue_wheel = { polar = true },
  joystick = { constraint = "circle", x_from = -1, x_to = 1, y_from = -1, y_to = 1, y_up = true, spring = true },
  xy_pad = { y_up = true },
}
local SELECTION = {
  tabs = { orientation = "horizontal" },
  segmented = { orientation = "horizontal" },
  list_selection = { orientation = "vertical" },
  sidebar_list = { orientation = "vertical" },
  grid_selection = { orientation = "grid" },
  swatch_grid = { orientation = "grid" },
  emoji_grid = { orientation = "grid" },
  icon_chooser = { orientation = "grid" },
  day_grid = { orientation = "grid", columns = 7 },
  toggle_group = { mode = "multi" },
  transfer_side = { orientation = "vertical", mode = "range" },
}

local function with_defaults(widget, spec, defaults)
  local merged = {}
  for k, v in pairs(defaults[widget] or {}) do merged[k] = v end
  for k, v in pairs(spec or {}) do merged[k] = v end
  merged.widget = widget
  -- A group's name without `exclusive` still makes radio buttons one group.
  if merged.group == nil and merged.exclusive then merged.group = "default" end
  return merged
end

for _, widget in ipairs(contract.archetypes.Press.widgets) do
  M[widget] = function(spec) return (control.make("Press", widget, with_defaults(widget, spec, PRESS))) end
end
for _, widget in ipairs(contract.archetypes.Plane.widgets) do
  M[widget] = function(spec) return (control.make("Plane", widget, with_defaults(widget, spec, PLANE))) end
end
for _, widget in ipairs(contract.archetypes.Selection.widgets) do
  M[widget] = function(spec)
    return (require("lib.kit.selection").make(widget, with_defaults(widget, spec, SELECTION)))
  end
end
-- A popup widget makes a popup: `widgets.menu(spec)` returns its handle
-- (lib.kit.popup), not a node -- it lives in the overlay layer.
for _, widget in ipairs(contract.archetypes.Popup.widgets) do
  M[widget] = function(spec) return require("lib.kit.popup").make(widget, spec) end
end
-- A text field or a scrolled view returns its control and the engine node
-- inside it: `local node, input = widgets.entry { ... }`.
for _, widget in ipairs(contract.archetypes.TextField.widgets) do
  M[widget] = function(spec) return require("lib.kit.text_field").make(widget, spec) end
end
for _, widget in ipairs(contract.archetypes.Scroll.widgets) do
  M[widget] = function(spec) return require("lib.kit.scroll").make(widget, spec) end
end
-- A collection returns its control and a handle (`scroll_to`, `model`, ...).
for _, widget in ipairs(contract.archetypes.Collection.widgets) do
  M[widget] = function(spec) return require("lib.kit.collection").make(widget, spec) end
end
for _, widget in ipairs(contract.archetypes.Disclosure.widgets) do
  M[widget] = function(spec) return require("lib.kit.disclosure").make(widget, spec) end
end
for _, widget in ipairs(contract.archetypes.Drag.widgets) do
  M[widget] = function(spec) return require("lib.kit.drag").make(widget, spec) end
end
for _, widget in ipairs(contract.archetypes.Navigation.widgets) do
  M[widget] = function(spec) return require("lib.kit.navigation").make(widget, spec) end
end
-- A canvas or a dock returns its control and a handle (`fit`, `zoom_by`,
-- `select` ...; `activate`, `close`, `float` ...).
local CANVAS = {
  zoomable_canvas = { wheel_zooms = true },
  node_graph = { grid = 16, snap = true },
  whiteboard = { tool = "freehand" },
  diagram = { grid = 10, snap = true },
  map_view = { wheel_zooms = true, min_zoom = 0.001, max_zoom = 1e6, movable = false },
  image_viewer = { wheel_zooms = true, movable = false, multi_select = false },
  chart_inspector = { axes = "x", tool = "brush", movable = false, wheel_zooms = true },
  timeline_track = { axes = "x" },
  drawing_board = { tool = "rect", grid = 8, snap = true },
}
for _, widget in ipairs(contract.archetypes.Canvas.widgets) do
  M[widget] = function(spec) return require("lib.kit.canvas").make(widget, with_defaults(widget, spec, CANVAS)) end
end
for _, widget in ipairs(contract.archetypes.Dock.widgets) do
  M[widget] = function(spec) return require("lib.kit.dock").make(widget, spec) end
end
for _, widget in ipairs(contract.archetypes.Range.widgets) do
  M[widget] = function(spec) return (control.make("Range", widget, with_defaults(widget, spec, RANGE))) end
end

--- A pressable area from a MouseArea-style table -- node properties,
--- children, `on_*` handlers -- as a kit Press `area`: the press, Tab and
--- the keys that click it are the archetype's, the theme's skin marks it,
--- and whoever builds it only says what it looks like and what a press
--- does. `settings` are the Press's own (`checked`, `group`, ...).
function M.area(props, settings)
  local spec, node, children = { widget = "area" }, {}, {}
  for key, value in pairs(props or {}) do
    if type(key) == "number" then children[key] = value
    elseif type(key) == "string" and key:match("^on_") then spec[key] = value
    else node[key] = value end
  end
  for key, value in pairs(settings or {}) do spec[key] = value end
  if node.enabled ~= nil then spec.enabled = node.enabled end
  return (control.make("Press", "area", spec, { props = node, children = children }))
end

--- A shield: a box that takes the pointer so nothing under it does (a
--- curtain over controls while it moves), and nothing else -- no focus, no
--- keys. `props` are its node's.
function M.shield(props)
  local node = {}
  for key, value in pairs(props or {}) do node[key] = value end
  node.focus_policy = "none"
  return (control.make("Control", "shield", { widget = "shield" }, { props = node }))
end

return M
