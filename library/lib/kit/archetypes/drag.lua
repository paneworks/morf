-- Drags (the Drag archetype): GtkPaned, GtkWindowHandle, GtkDragSource,
-- GtkDropTarget, and the swipe a carousel or a toast is let go by.
--
--     drag.make("paned", { start_child = list, end_child = detail, width = 800, height = 500,
--       position = 0.35, minimum = 0.2, maximum = 0.8, on_moved = function(ratio) end })
--     drag.make("window_handle", { width = 400, height = 46, on_dragged = move_by })
--     drag.make("drag_source", { payload = { ["text/plain"] = "hello" }, width = 120, height = 40,
--       on_transferred = function(dropped) end })
--     drag.make("drop_target", { keys = { "text/plain" }, on_dropped = function(drop) end, width = 200, height = 120 })
--     drag.swipe(card, { on_swiped = function(direction) dismiss() end })  -- on a node of the layout's
--
-- The control is the handle; the skin draws `handle`, `ghost` and
-- `drop_indicator`. `drag.swipe` gives a layout's own node the swipe: a
-- headless Drag it feeds the pointer to, which moves the node with the
-- finger, lets it go past its distance or speed, and springs it back
-- otherwise.
local ui = require("morf.ui")
local control = require("lib.kit.control")

local M = {}

local function get(v) if type(v) == "function" then return v() end return v end

-- What each widget is unless its spec says otherwise.
local DEFAULTS = {
  window_handle = { mode = "move", axis = "both", cursor = "move" },
  drag_source = { mode = "transfer", axis = "both", cursor = "grab" },
}

--- GtkPaned: two children and the divider between them, which drags the
--- ratio. `start_child`, `end_child` (nodes), `width`, `height`,
--- `orientation` ("horizontal": side by side), `position` (0.5, a ratio),
--- `minimum`, `maximum`, `wide` (a variant: a 6 px divider, 1 px
--- otherwise; it is grabbed 6 px wide either way), `on_moved(ratio)`.
local function paned(spec)
  local horizontal = (spec.orientation or "horizontal") == "horizontal"
  local W, H = spec.width, spec.height
  local D = require("lib.kit.archetypes.press").has(spec, "wide") and 6 or 1
  local grab = math.max(D, 6)
  local length = horizontal and W or H
  local ratio = morf.signal("kit.paned." .. tostring(spec.start_child), spec.position or 0.5)
  local function first_size() return math.floor((length - D) * ratio:get()) end
  local first = ui.Item { width = horizontal and first_size or W, height = horizontal and H or first_size, clip = true,
    spec.start_child }
  local second = ui.Item { clip = true,
    x = horizontal and function() return first_size() + D end or 0,
    y = horizontal and 0 or function() return first_size() + D end,
    width = horizontal and function() return W - first_size() - D end or W,
    height = horizontal and H or function() return H - first_size() - D end,
    spec.end_child }
  local divider, t, ctl = control.make("Drag", "paned", {
    widget = "paned", variant = spec.variant, mode = "split", axis = horizontal and "x" or "y", extent = length - D,
    minimum = spec.minimum or 0.1, maximum = spec.maximum or 0.9, value = function() return ratio:get() end,
    on_moved = function(v) ratio:set(v) if spec.on_moved then spec.on_moved(v) end end,
    x = horizontal and function() return first_size() + D / 2 - grab / 2 end or 0,
    y = horizontal and 0 or function() return first_size() + D / 2 - grab / 2 end,
    width = horizontal and grab or W, height = horizontal and H or grab,
    cursor = horizontal and "col_resize" or "row_resize",
  })
  local node = ui.Item { id = spec.id, x = spec.x, y = spec.y, anchors = spec.anchors, width = W, height = H,
    first, second, divider }
  return node, t, ctl, ratio
end

--- GtkDropTarget: where a drag from anywhere lands; `keys` (the payload
--- types it takes), `on_entered(info)`, `on_exited()`, `on_dropped(drop)`.
--- `t.accepting` while a drag it takes is over it.
local function drop_target(spec)
  local own = {}
  for k, v in pairs(spec) do own[k] = v end
  own.mode, own.widget = "transfer", "drop_target"
  own.on_dropped = nil
  local t
  local area = ui.DropArea { anchors = { fill = true }, keys = spec.keys or {},
    on_entered = function(info)
      if t then t.accepting = info == nil or info.accepted ~= nil end
      if spec.on_entered then spec.on_entered(info) end
    end,
    on_exited = function() if t then t.accepting = false end if spec.on_exited then spec.on_exited() end end,
    on_dropped = function(drop) if spec.on_dropped then spec.on_dropped(drop) end end }
  local root, ctl
  root, t, ctl = control.make("Drag", "drop_target", own, { state = { accepting = false }, children = { area } })
  return root, t, ctl
end

--- A drag of `widget` (paned, window_handle, drag_source, drop_target).
--- A drag source starts a transfer of `payload` (a binding or a table of
--- type -> data) as the drag begins: `on_transferred(dropped)` hears how
--- it ended.
function M.make(widget, spec)
  spec = spec or {}
  if widget == "paned" then return paned(spec) end
  if widget == "drop_target" then return drop_target(spec) end
  local given = spec
  spec = {}
  for k, v in pairs(DEFAULTS[widget] or {}) do spec[k] = v end
  for k, v in pairs(given) do spec[k] = v end
  local children = {}
  for i, child in ipairs(spec) do children[i] = child spec[i] = nil end
  local root, t, ctl = control.make("Drag", widget, spec, { children = children })
  if get(spec.mode) == "transfer" then
    root.on_drag_started = function(...)
      local payload = get(spec.payload)
      if payload then
        morf.drag.start(payload, function(dropped)
          if spec.on_transferred then spec.on_transferred(dropped) end
        end)
      end
      if spec.on_drag_started then spec.on_drag_started(...) end
    end
  end
  return root, t, ctl
end

--- The swipe, on a node of the layout's own: `axis` ("x"), `distance`
--- (80), `speed` (600), `on_swiped(direction)`. The node follows the drag
--- (`translate_x`/`_y`) and springs back when let go short. Returns the
--- behaviour.
function M.swipe(node, spec)
  spec = spec or {}
  local axis = spec.axis or "x"
  local behaviour = control.headless("Drag", { mode = "swipe", axis = axis, swipe_distance = spec.distance or 80,
    swipe_speed = spec.speed or 600, owner = node, on_swiped = spec.on_swiped })
  local t = behaviour.t
  -- The node's pointer goes to the behaviour, after whatever the node
  -- already does with it (handlers set on it now chain the ones it had
  -- only through the spec: give `on_pressed` there to keep one).
  node.on_pressed = function(sx, sy, ...) behaviour.send("pressed", sx, sy) if spec.on_pressed then spec.on_pressed(sx, sy, ...) end end
  node.on_dragged = function(sx, sy) behaviour.send("dragged", sx, sy) end
  node.drag_axis = axis
  node.on_released = function(...) behaviour.send("released", 0, 0) if spec.on_released then spec.on_released(...) end end
  node.on_swiped = function(_, vx, vy) behaviour.send("fling", vx, vy) end
  local property = axis == "y" and "translate_y" or "translate_x"
  node[property] = function() return axis == "y" and t.delta_y or t.delta_x end
  return behaviour
end

return M
