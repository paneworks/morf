-- The base every kit control is built on: a Rust archetype (crates/morf-kit,
-- `morf.kit.native`) owning its state and behaviour, and a skin
-- (lib.kit.skin) drawing it in slots.
--
--     local control = require("lib.kit.control")
--     local node = control.make("Control", "Card", { width = 200, height = 80 })
--
-- `make(archetype, widget, spec)` returns the control's node, a MouseArea
-- whose pointer, focus and key events go to the archetype. The archetype's
-- answers update `t`, the live state its skin reads (`t.hovered`,
-- `t.down`, `t.visual_focus`, `t.width`, ...), and raise the spec's
-- handlers (`on_clicked`, `on_pressed`, ...). Slots stack in the
-- archetype's order; the node's size is the spec's, or the implicit size
-- of its `background` and `content` with `padding` and `insets` around
-- them.
local ui = require("morf.ui")
local native = require("morf.kit.native")
local skin = require("lib.kit.skin")

local M = {}

-- Spec fields that are the archetype's settings, not the node's.
local BASE = { "enabled", "mirrored", "highlighted" }
local SETTINGS = {
  Control = {},
  Press = { "checkable", "checked", "tristate", "partial", "group", "exclusive", "allow_none", "auto_repeat",
    "repeat_delay", "repeat_interval" },
  Range = { "from", "to", "value", "step", "page_step", "snap", "live", "orientation", "inverted", "logarithmic",
    "wrap", "range", "first", "second", "handle_size" },
  Plane = { "x_from", "x_to", "y_from", "y_to", "x", "y", "step_x", "step_y", "constraint", "y_up" },
  Selection = { "count", "labels", "current", "selected", "mode", "wrap", "orientation", "columns", "page",
    "disabled", "follow_focus" },
  Popup = { "modal", "dim", "close_policy", "placement", "focus_on_open", "restore_focus" },
  TextField = { "text", "placeholder", "echo", "read_only", "max_length", "validator", "minimum", "maximum",
    "required", "revert_on_escape" },
  Scroll = { "scroll_policy_x", "scroll_policy_y", "snap", "item_size", "step" },
}
-- How each archetype takes focus by default: a press by Tab only, so a click
-- leaves a search field typing; a range by click too, so the arrows move
-- what was just dragged.
local POLICY = { Control = "none", Press = "tab", Range = "strong", Plane = "strong", Selection = "strong",
  TextField = "none", Scroll = "none" }
-- Which take keys and the wheel.
local KEYS = { Press = true, Range = true, Plane = true, Selection = true, Scroll = true }
local WHEEL = { Range = true, Plane = true }
-- The clock typeahead measures pauses on.
local clock = morf.elapsed_timer()
-- Handlers that are an archetype's signals, raised through `apply`, not
-- given to the node.
local SIGNALS = { on_clicked = true, on_toggled = true, on_moved = true, on_value_changed = true,
  on_long_pressed = true, on_double_clicked = true, on_current_changed = true, on_selection_changed = true,
  on_activated = true, on_opened = true, on_closed = true, on_about_to_close = true, on_edited = true,
  on_invalid = true, on_scrolled = true, on_reached_start = true, on_reached_end = true, on_set_text = true,
  on_scroll_to = true }
-- Every live control's way to take effects another control's event caused
-- (an exclusive group), by id.
local appliers = {}
-- Spec fields that go to the node as they are.
local NODE = { id = true, x = true, y = true, z = true, anchors = true, visible = true, opacity = true,
  layout = true, focus_policy = true, cursor = true, scale = true, rotation = true, stretch = true,
  behavior = true, translate_x = true, translate_y = true }

local function value(v) if type(v) == "function" then return v() end return v end

-- A list in the live state (a selection's indices) is kept as text,
-- ",2,5,", so a binding compares it as one value: `control.has(t.selected, i)`.
local function encode(v)
  if type(v) ~= "table" then return v end
  local parts = {}
  for i, item in ipairs(v) do parts[i] = tostring(item) end
  return "," .. table.concat(parts, ",") .. ","
end

--- Whether a list kept in the live state holds `item`.
function M.has(list, item) return type(list) == "string" and list:find("," .. tostring(item) .. ",", 1, true) ~= nil end

local function sides(v)
  if type(v) == "table" then return { v[1] or 0, v[2] or 0, v[3] or 0, v[4] or 0 } end
  return v or 0
end

--- Builds a control. See the head of this file.
function M.make(archetype, widget, spec, extra)
  spec = spec or {}
  extra = extra or {}
  local slot_names = native.slots(archetype)
  local fields = {}
  for _, field in ipairs(BASE) do fields[#fields + 1] = field end
  for _, field in ipairs(SETTINGS[archetype] or {}) do fields[#fields + 1] = field end
  local settings = {}
  for _, field in ipairs(fields) do
    local v = spec[field]
    if v ~= nil and type(v) ~= "function" then settings[field] = v end
  end
  local id, state, made = native.new(archetype, settings)
  for field, v in pairs(state) do state[field] = encode(v) end
  state.width, state.height = 0, 0
  for field, v in pairs(extra.state or {}) do state[field] = v end
  local t = morf.state(state)
  local root, slots, waiting, builders
  local repeat_timer
  -- What the click being delivered said, for the configuration's handler.
  local click_args = {}
  local function apply(effects)
    for field, v in pairs(effects.state) do t[field] = encode(v) end
    -- Slots a skin left to be built when first wanted.
    if waiting and (t.hovered or t.down or t.visual_focus) then
      local now = waiting
      waiting = nil
      for _, entry in ipairs(now) do
        local node = entry[2]()
        if node then slots[entry[1]] = node ui.reparent(node, root) end
      end
    end
    for _, signal in ipairs(effects.signals) do
      local name = signal[1]
      if name == "schedule_repeat" then
        if repeat_timer then repeat_timer:cancel() end
        repeat_timer = morf.timer(signal[2], function()
          repeat_timer = nil
          appliers[id](native.send(id, "repeat"))
        end)
      elseif name == "focus_request" then
        if root then morf.focus.set(root, true) end
      elseif name == "set_text" or name == "scroll_to" then
        local handler = spec["on_" .. name]
        if handler then handler(table.unpack(signal, 2)) end
      elseif name == "pressed" or name == "released" then
        -- The configuration's own pointer handlers hear the event itself.
      elseif name == "clicked" then
        if spec.on_clicked then spec.on_clicked(table.unpack(click_args)) end
      else
        local handler = spec["on_" .. name]
        if handler then handler(table.unpack(signal, 2)) end
      end
    end
    for _, other in ipairs(effects.others or {}) do
      local apply_other = appliers[other[1]]
      if apply_other then apply_other(other[2]) end
    end
  end
  appliers[id] = apply
  local function send(event, ...) apply(native.send(id, event, ...)) end
  -- Where a press or a drag is along the skin's track: the archetype maps
  -- the pointer onto the travel the skin drew.
  local function travel(x, y)
    local track = slots and slots.track
    if track then
      -- Both on the surface: the track's place in the control is the difference.
      local tx = (track.layout_x or 0) - (root.layout_x or 0)
      local ty = (track.layout_y or 0) - (root.layout_y or 0)
      return x - tx, y - ty, track.layout_width or 0, track.layout_height or 0
    end
    return x, y, root.layout_width or 0, root.layout_height or 0
  end
  -- After the archetype has the event, the configuration's own handler
  -- for it, with what the pointer said.
  local function also(name, ...) local own = spec[name] if own then return own(...) end end
  local props = {
    focus_policy = POLICY[archetype] or "none",
    on_entered = function(...) send("entered") also("on_entered", ...) end,
    on_exited = function(...) send("exited") also("on_exited", ...) end,
    on_pressed = function(sx, sy, x, y, ...)
      local a, b, w, h = travel(x, y)
      send("pressed", a, b, w, h)
      also("on_pressed", sx, sy, x, y, ...)
    end,
    on_dragged = function(sx, sy, dx, dy, x, y, ...)
      local a, b, w, h = travel(x, y)
      send("dragged", a, b, w, h)
      also("on_dragged", sx, sy, dx, dy, x, y, ...)
    end,
    on_released = function(...) send("released") also("on_released", ...) end,
    on_clicked = function(...)
      click_args = { ... }
      send("clicked")
    end,
    -- A long press takes the click its release would make, so it is
    -- listened for only when the configuration wants it (and the double
    -- click likewise).
    on_long_pressed = spec.on_long_pressed and function() send("long_pressed") end or nil,
    on_double_clicked = spec.on_double_clicked and function() send("double_clicked") end or nil,
    on_focus_changed = function(on) send("focus", on, root and root.visual_focus or false) end,
    on_destroyed = function()
      if repeat_timer then repeat_timer:cancel() end
      appliers[id] = nil
      skin.untrack(root)
      native.drop(id)
    end,
  }
  -- (A control that cannot take focus takes no keys either: it would
  -- otherwise be where a surface with no focus sends them.)
  local focusable = (extra.props and extra.props.focus_policy or props.focus_policy) ~= "none"
  if KEYS[archetype] and focusable then
    -- A key the archetype does not use goes on to what is around it.
    props.on_key_pressed = function(keysym, text, modifiers, repeat_, name)
      local effects = native.send(id, "key", name or "", modifiers or "", text or "", clock:elapsed_ms())
      apply(effects)
      if effects.handled then return true end
      if spec.on_key_pressed then return spec.on_key_pressed(keysym, text, modifiers, repeat_, name) end
      return false
    end
  end
  -- (`wheel = false`: the wheel goes past it -- a scroll bar leaves it to
  -- the view it scrolls.)
  if WHEEL[archetype] and spec.wheel ~= false then
    props.on_wheel = function(_, _, _, _, step_x, step_y) send("wheel", step_x or 0, step_y or 0) end
  end
  -- The configuration's other handlers go to the node as they are.
  for name, handler in pairs(spec) do
    if type(name) == "string" and name:match("^on_") and props[name] == nil and type(handler) == "function"
      and not SIGNALS[name] then
      props[name] = handler
    end
  end
  -- Properties of the node itself, and the children a layout put in it --
  -- the configuration's, not the skin's, so a theme switch keeps them.
  for name, v in pairs(extra.props or {}) do props[name] = v end
  for i, child in ipairs(extra.children or {}) do props[i] = child end
  for field in pairs(NODE) do if spec[field] ~= nil then props[field] = spec[field] end end
  local padding, insets = sides(spec.padding), sides(spec.insets)
  -- Bumped once the slots are built, so the size binding reads the new ones.
  local generation = morf.signal("kit.control.slots." .. id, 0)
  local function slot_size(name, axis)
    local node = slots and slots[name]
    if not node then return 0 end
    local own = node[axis]
    if type(own) == "number" and own > 0 then return own end
    return node["layout_" .. axis] or 0
  end
  local function implicit(axis)
    generation:get()
    local w, h = native.implicit_size(slot_size("background", "width"), slot_size("background", "height"),
      slot_size("content", "width"), slot_size("content", "height"), padding, insets)
    return axis == "width" and w or h
  end
  if props.width == nil then props.width = spec.width or function() return implicit("width") end end
  if props.height == nil then props.height = spec.height or function() return implicit("height") end end
  root = ui.MouseArea(props)
  -- The live size, for skins that draw to it.
  morf.effect("kit.control.size." .. id, function()
    t.width, t.height = root.layout_width or 0, root.layout_height or 0
  end, { owner = root })
  -- Settings given as bindings follow them.
  for _, field in ipairs(fields) do
    if type(spec[field]) == "function" then
      morf.effect("kit.control." .. field .. "." .. id, function()
        apply(native.configure(id, field, spec[field]()))
      end, { owner = root })
    end
  end
  local function build()
    if slots then for _, node in pairs(slots) do ui.destroy(node, true) end end
    slots, waiting, builders = {}, nil, {}
    local built = skin.build(widget, archetype, slot_names, t, spec, nil, root, send)
    for _, name in ipairs(slot_names) do
      local node = built[name]
      if (extra.builders or {})[name] then
        -- Not a node: what the control builds its parts with (a
        -- selection's item delegates).
        builders[name] = node
      elseif type(node) == "function" then
        waiting = waiting or {}
        waiting[#waiting + 1] = { name, node }
      elseif node then
        slots[name] = node
        ui.reparent(node, root)
      end
    end
    generation:set(generation:get() + 1)
    if extra.on_rebuild then extra.on_rebuild(builders, t, send) end
  end
  build()
  skin.track(root, build)
  apply(made)
  return root, t, { id = id, send = send, configure = function(field, v) apply(native.configure(id, field, value(v))) end,
    slots = function() return slots end, builders = function() return builders end }
end

--- An archetype with no node: its state and keys for a view that draws its
--- own items -- a launcher's list, until a Collection draws it. `spec`
--- holds settings (values or bindings) and signal handlers, as for `make`;
--- `spec.owner`, a node, ends its bindings with it. Returns `{ t, key,
--- send, drop }`: `key(name, modifiers, text)` answers whether the key was
--- used.
function M.headless(archetype, spec)
  spec = spec or {}
  local fields = {}
  for _, field in ipairs(BASE) do fields[#fields + 1] = field end
  for _, field in ipairs(SETTINGS[archetype] or {}) do fields[#fields + 1] = field end
  local settings = {}
  for _, field in ipairs(fields) do
    local v = spec[field]
    if v ~= nil and type(v) ~= "function" then settings[field] = v end
  end
  local id, state, made = native.new(archetype, settings)
  for field, v in pairs(state) do state[field] = encode(v) end
  local t = morf.state(state)
  local function apply(effects)
    for field, v in pairs(effects.state) do t[field] = encode(v) end
    for _, signal in ipairs(effects.signals) do
      local handler = spec["on_" .. signal[1]]
      if handler then handler(table.unpack(signal, 2)) end
    end
  end
  for _, field in ipairs(fields) do
    if type(spec[field]) == "function" then
      morf.effect("kit.headless." .. field .. "." .. id, function()
        apply(native.configure(id, field, spec[field]()))
      end, spec.owner and { owner = spec.owner } or nil)
    end
  end
  apply(made)
  local handle = { t = t }
  function handle.send(event, ...) local effects = native.send(id, event, ...) apply(effects) return effects end
  function handle.key(name, modifiers, text)
    return handle.send("key", name or "", modifiers or "", text or "", clock:elapsed_ms()).handled
  end
  function handle.drop() native.drop(id) end
  return handle
end

return M
