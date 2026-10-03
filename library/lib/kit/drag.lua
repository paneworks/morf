-- Drags (the Drag archetype): split panes, resize grips, reorderable rows,
-- swipe-to-dismiss, window move.
--
--     drag.make("grip", { mode = "resize", axis = "x", minimum = 200, maximum = 600,
--       value = function() return width:get() end, on_moved = function(w) width:set(w) end,
--       width = 8, height = 300 })
--     drag.split { first = list, second = detail, width = 800, height = 500,
--       ratio = 0.35, minimum = 0.2, maximum = 0.8 }
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

function M.make(widget, spec)
  return control.make("Drag", widget, spec)
end

--- Two panes and the divider between them, which drags the ratio.
--- `first`, `second` (nodes), `width`, `height`, `orientation`
--- ("horizontal": side by side), `ratio` (0.5), `minimum`, `maximum`,
--- `divider` (px, 6), `on_moved(ratio)`.
function M.split(spec)
  local horizontal = (spec.orientation or "horizontal") == "horizontal"
  local W, H, D = spec.width, spec.height, spec.divider or 6
  local length = horizontal and W or H
  local ratio = morf.signal("kit.split." .. tostring(spec.first), spec.ratio or 0.5)
  local function first_size() return math.floor((length - D) * ratio:get()) end
  local first = ui.Item { width = horizontal and first_size or W, height = horizontal and H or first_size, clip = true,
    spec.first }
  local second = ui.Item { clip = true,
    x = horizontal and function() return first_size() + D end or 0,
    y = horizontal and 0 or function() return first_size() + D end,
    width = horizontal and function() return W - first_size() - D end or W,
    height = horizontal and H or function() return H - first_size() - D end,
    spec.second }
  local divider = control.make("Drag", "split_pane", {
    widget = "split_pane", mode = "split", axis = horizontal and "x" or "y", extent = length - D,
    minimum = spec.minimum or 0.1, maximum = spec.maximum or 0.9, value = function() return ratio:get() end,
    on_moved = function(v) ratio:set(v) if spec.on_moved then spec.on_moved(v) end end,
    x = horizontal and first_size or 0, y = horizontal and 0 or first_size,
    width = horizontal and D or W, height = horizontal and H or D,
    cursor = horizontal and "col_resize" or "row_resize",
  })
  return ui.Item { id = spec.id, x = spec.x, y = spec.y, width = W, height = H, first, second, divider }, ratio
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
  node.on_dragged = function(sx, sy, ...) behaviour.send("dragged", sx, sy) end
  node.on_released = function(...) behaviour.send("released", 0, 0) if spec.on_released then spec.on_released(...) end end
  node.on_swiped = function(_, vx, vy) behaviour.send("fling", vx, vy) end
  local property = axis == "y" and "translate_y" or "translate_x"
  node[property] = function() return axis == "y" and t.delta_y or t.delta_x end
  return behaviour
end

return M
