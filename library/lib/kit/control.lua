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
local SETTINGS = { enabled = true, mirrored = true, highlighted = true }
-- Spec fields that go to the node as they are.
local NODE = { id = true, x = true, y = true, z = true, anchors = true, visible = true, opacity = true,
  layout = true, focus_policy = true, cursor = true, scale = true, rotation = true, stretch = true,
  behavior = true, translate_x = true, translate_y = true }

local function value(v) if type(v) == "function" then return v() end return v end

local function sides(v)
  if type(v) == "table" then return { v[1] or 0, v[2] or 0, v[3] or 0, v[4] or 0 } end
  return v or 0
end

--- Builds a control. See the head of this file.
function M.make(archetype, widget, spec)
  spec = spec or {}
  local slot_names = native.slots(archetype)
  local settings = {}
  for field in pairs(SETTINGS) do
    local v = spec[field]
    if v ~= nil and type(v) ~= "function" then settings[field] = v end
  end
  local id, state = native.new(archetype, settings)
  state.width, state.height = 0, 0
  local t = morf.state(state)
  local root, slots
  local function apply(effects)
    for field, v in pairs(effects.state) do t[field] = v end
    for _, signal in ipairs(effects.signals) do
      local handler = spec["on_" .. signal[1]]
      if handler then handler(table.unpack(signal, 2)) end
    end
  end
  local function send(event, ...) apply(native.send(id, event, ...)) end
  local props = {
    focus_policy = "none",
    on_entered = function() send("entered") end,
    on_exited = function() send("exited") end,
    on_pressed = function(_, _, x, y, button) send("pressed", x, y, button) end,
    on_released = function() send("released") end,
    on_clicked = function() send("clicked") end,
    on_focus_changed = function(on) send("focus", on, root and root.visual_focus or false) end,
    on_destroyed = function()
      skin.untrack(root)
      native.drop(id)
    end,
  }
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
  props.width = spec.width or function() return implicit("width") end
  props.height = spec.height or function() return implicit("height") end
  root = ui.MouseArea(props)
  -- The live size, for skins that draw to it.
  morf.effect("kit.control.size." .. id, function()
    t.width, t.height = root.layout_width or 0, root.layout_height or 0
  end, { owner = root })
  -- Settings given as bindings follow them.
  for field in pairs(SETTINGS) do
    if type(spec[field]) == "function" then
      morf.effect("kit.control." .. field .. "." .. id, function()
        apply(native.configure(id, field, spec[field]()))
      end, { owner = root })
    end
  end
  local function build()
    if slots then for _, node in pairs(slots) do ui.destroy(node, true) end end
    slots = skin.build(widget, archetype, slot_names, t, spec)
    for _, name in ipairs(slot_names) do
      local node = slots[name]
      if node then ui.reparent(node, root) end
    end
    generation:set(generation:get() + 1)
  end
  build()
  skin.track(root, build)
  return root, t, { id = id, send = send, configure = function(field, v) apply(native.configure(id, field, value(v))) end,
    slots = function() return slots end }
end

return M
