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
  repeat_button = { auto_repeat = true, width = 40, height = 40 },
  disclosure_button = { checkable = true },
  -- A quick-settings tile turns something on and off.
  tile = { checkable = true },
  -- Icon-only presses have no label to size them by.
  circular = { width = 48, height = 48 },
  close = { width = 40, height = 40 },
  help = { width = 40, height = 40 },
  back = { width = 40, height = 40 },
  forward = { width = 40, height = 40 },
  fab = { width = 56, height = 56 },
  extended_fab = { height = 56 },
  -- A press that must be held to count: a delete that asks to be meant.
  hold_button = { hold = 800 },
}
local RANGE = {
  vertical_slider = { orientation = "vertical" },
  fader = { orientation = "vertical", value = 0.75 },
  range_slider = { range = true },
  -- Eleven stops, 0 to 10.
  discrete_slider = { snap = "always", step = 1, from = 0, to = 10 },
  stepped_knob = { snap = "always", step = 1, from = 0, to = 10, drag_mode = "vertical" },
  rating = { snap = "always", step = 1, from = 0, to = 5 },
  -- The audible frequencies, in Hz.
  log_slider = { logarithmic = true, from = 20, to = 20000, value = 1000 },
  angle_slider = { wrap = true, from = 0, to = 360, drag_mode = "angular", angle_from = 0, angle_sweep = 360 },
  knob = { drag_mode = "vertical" },
  bipolar_knob = { from = -1, to = 1, value = 0, drag_mode = "vertical" },
  -- Its buttons and the arrows step by a quarter.
  zoom = { from = 0.25, to = 4, value = 1, logarithmic = true, step = 0.25 },
  -- A drag up or down over a spin button scrubs it.
  spin_button = { step = 1, snap = "always", from = 0, to = 100, drag_mode = "vertical" },
  seek_bar = { live = false },
  scroll_bar = { wheel = false },
  volume = { from = 0, to = 1 },
  brightness = { from = 0, to = 1 },
  level_control = { from = 0, to = 1 },
  osd_level = { enabled = false, from = 0, to = 1 },
}

local PLANE = {
  hue_wheel = { polar = true, y = 1 },
  joystick = { constraint = "circle", x_from = -1, x_to = 1, y_from = -1, y_to = 1, y_up = true, spring = true },
  xy_pad = { y_up = true },
  -- Left to right and front to back, the listener in the middle.
  pan_pad = { constraint = "circle", x_from = -1, x_to = 1, y_from = -1, y_to = 1, y_up = true },
  -- A breakpoint's time across and level up.
  envelope_point = { y_up = true },
}
-- (Entry sizes are a fallback for a configuration that names none: an
-- entry with no size of its own draws nothing.)
local SELECTION = {
  tabs = { orientation = "horizontal" },
  segmented = { orientation = "horizontal", item_width = 80, item_height = 36 },
  view_switcher = { orientation = "horizontal", item_width = 88, item_height = 56 },
  inline_view_switcher = { orientation = "horizontal", item_width = 96, item_height = 36 },
  radio_group = { orientation = "vertical", item_width = 200, item_height = 40 },
  toggle_group = { mode = "multi", item_width = 44, item_height = 40 },
  list_selection = { orientation = "vertical", item_width = 220, item_height = 40 },
  sidebar_list = { orientation = "vertical", item_width = 220, item_height = 40 },
  grid_selection = { orientation = "grid", item_width = 56, item_height = 56 },
  carousel_dots = { orientation = "horizontal", item_width = 24, item_height = 24 },
  pagination = { orientation = "horizontal", item_width = 36, item_height = 36 },
  stepper_header = { orientation = "horizontal", item_width = 96, item_height = 64 },
  breadcrumbs = { orientation = "horizontal", item_height = 36 },
  swatch_grid = { orientation = "grid", item_width = 32, item_height = 32 },
  emoji_grid = { orientation = "grid", item_width = 40, item_height = 40 },
  icon_chooser = { orientation = "grid", item_width = 44, item_height = 44 },
  day_grid = { orientation = "grid", columns = 7, item_width = 40, item_height = 36 },
  transfer_side = { orientation = "vertical", mode = "range", item_width = 220, item_height = 36 },
  rating_items = { orientation = "horizontal", item_width = 36, item_height = 36 },
  -- On a circle, item 1 at twelve o'clock (lib.kit.selection's geometries).
  radial_menu = { geometry = "radial", wrap = true, size = 220, item_width = 48, item_height = 48 },
  pie_menu = { geometry = "radial", wrap = true, size = 240, item_width = 64, item_height = 48 },
  -- A drum of entries, five rows showing; it goes round.
  tumbler = { geometry = "tumbler", orientation = "vertical", wrap = true, item_width = 72, item_height = 36,
    rows = 5 },
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
-- A pie menu opens over everything else: its node and its handle
-- (`open_at`, `close`, `attach`; lib.kit.selection's `pie`).
M.pie_menu = function(spec) return require("lib.kit.selection").pie("pie_menu", with_defaults("pie_menu", spec, SELECTION)) end
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
-- A shell returns its control and a handle (`toggle_sidebar`, `dismiss`).
for _, widget in ipairs(contract.archetypes.Shell.widgets) do
  M[widget] = function(spec) return require("lib.kit.shell").make(widget, spec) end
end
-- A canvas or a dock returns its control and a handle (`fit`, `zoom_by`,
-- `select` ...; `activate`, `close`, `float` ...).
local CANVAS = {
  zoomable_canvas = { wheel_zooms = true, resizable = true },
  node_graph = { grid = 16, snap = true },
  whiteboard = { tool = "freehand", resizable = true },
  diagram = { grid = 10, snap = true, resizable = true },
  map_view = { wheel_zooms = true, min_zoom = 0.001, max_zoom = 1e6, movable = false },
  image_viewer = { wheel_zooms = true, movable = false, multi_select = false },
  chart_inspector = { axes = "x", tool = "brush", movable = false, wheel_zooms = true },
  timeline_track = { axes = "x" },
  drawing_board = { tool = "rect", grid = 8, snap = true, resizable = true },
}
for _, widget in ipairs(contract.archetypes.Canvas.widgets) do
  M[widget] = function(spec) return require("lib.kit.canvas").make(widget, with_defaults(widget, spec, CANVAS)) end
end
for _, widget in ipairs(contract.archetypes.Dock.widgets) do
  M[widget] = function(spec) return require("lib.kit.dock").make(widget, spec) end
end
-- The fifth stage's archetypes: each returns its control and a handle.
for archetype, module in pairs { Transform = "transform", Sheet = "sheet", Roving = "roving", Form = "form",
  Overflow = "overflow" } do
  for _, widget in ipairs(contract.archetypes[archetype].widgets) do
    M[widget] = function(spec) return require("lib.kit." .. module).make(widget, spec) end
  end
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
