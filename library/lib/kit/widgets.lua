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
  stepped_knob = { snap = "always", step = 1 },
  rating = { snap = "always", step = 1, from = 0, to = 5 },
  log_slider = { logarithmic = true },
  angle_slider = { wrap = true, from = 0, to = 360 },
  knob = {},
  bipolar_knob = { from = -1, to = 1 },
  spin_button = { step = 1, snap = "always" },
  seek_bar = { live = false },
  scroll_bar = { wheel = false },
  osd_level = { enabled = false },
}

local PLANE = {
  hue_wheel = { constraint = "circle" },
  joystick = { constraint = "circle", x_from = -1, x_to = 1, y_from = -1, y_to = 1, y_up = true },
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
for _, widget in ipairs(contract.archetypes.Range.widgets) do
  M[widget] = function(spec) return (control.make("Range", widget, with_defaults(widget, spec, RANGE))) end
end

return M
