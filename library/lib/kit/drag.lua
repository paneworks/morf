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

local function get(v) if type(v) == "function" then return v() end return v end

-- What each widget is unless its spec says otherwise.
local DEFAULTS = {
  split_pane = { mode = "split", axis = "x", cursor = "col_resize" },
  resizable_panel = { mode = "resize", axis = "x", cursor = "col_resize" },
  resize_grip = { mode = "resize", axis = "both", cursor = "nwse_resize" },
  reorderable_rows = { mode = "reorder", axis = "y", cursor = "grab", follow = true },
  reorderable_tabs = { mode = "reorder", axis = "x", cursor = "grab", follow = true },
  sortable_grid = { mode = "move", axis = "both", cursor = "grab", follow = true },
  swipe_dismiss = { mode = "swipe", axis = "x", follow = true },
  swipe_actions = { mode = "swipe", axis = "x", follow = true },
  pull_to_refresh = { mode = "move", axis = "y" },
  sheet_handle = { mode = "move", axis = "y", cursor = "row_resize" },
  window_move = { mode = "move", axis = "both", cursor = "move" },
  drag_source = { mode = "transfer", axis = "both", cursor = "grab" },
  slide_to_confirm = { mode = "confirm", axis = "x" },
}

-- The spring a followed drag eases with: stiff and critically damped, so
-- it keeps up with the finger; a layout that moves the control's `x`/`y`
-- on the same spring (spec.behavior) keeps it under the finger as it
-- slots into a new place (the sum of two like springs is that spring).
local FOLLOW = { stiffness = 700, damping = 53 }

--- A drag control. With `mode = "transfer"` (a `drag_source`'s default) a
--- drag carries `payload` -- `{ text =, uris =, paths =, data = { [mime]
--- = bytes } }`, or a function giving one -- out to wherever it is
--- dropped, another application included; `on_transferred(dropped)`
--- hears whether it landed. A `drop_zone` takes such drops: `keys` (what
--- it accepts: mime types, `text`, `image`, `files`), `on_dropped(drop)`
--- (drop.text, drop.uris, drop.paths, drop:read(mime, fn)); its skin
--- shows `t.accepting` while a drag it would take is over it.
---
--- The spec's array part is the control's content (a row's label, a
--- list to pull). `follow = true` (reorderable rows and tabs, a sortable
--- grid, the swipes) moves the control with the drag and springs it back
--- on release; it then rides in a holder Item (returned in its place)
--- that takes the spec's `x`, `y`, `z` and `behavior`; `x`/`y` given as
--- bindings are its place, so a row the reorder moved stays under the
--- finger. A `swipe_dismiss` flies off
--- past its distance (`on_swiped(direction)` then); a `swipe_actions`
--- row latches open `reveal` px (128) to show what is under its trailing
--- edge, and shuts swiped back (`t.open`). A `pull_to_refresh` moves its
--- content down with a rubber band (`t.pull`, 1 at the threshold) and,
--- let go past it, calls `on_refresh(done)` (`t.refreshing` until `done()`).
--- `detents` (a list of values) snaps a move where it is let go.
function M.make(widget, spec)
  spec = spec or {}
  if widget == "drop_zone" then return M.drop_zone(spec) end
  local given = spec
  spec = {}
  for k, v in pairs(DEFAULTS[widget] or {}) do spec[k] = v end
  for k, v in pairs(given) do spec[k] = v end
  local children = {}
  for i, child in ipairs(spec) do children[i] = child spec[i] = nil end
  local state = {}
  local follow = get(spec.follow) == true
  local swipe_out = widget == "swipe_dismiss"
  local reveal = widget == "swipe_actions" and (spec.reveal or 128) or nil
  local pulling = widget == "pull_to_refresh"
  local t, ctl, root
  -- Slide to confirm: a knob (`knob` px, the height) runs the track's
  -- width less its own; `t.done` is up for a moment after a confirm.
  local confirming = widget == "slide_to_confirm"
  if confirming then
    local w, h = tonumber(spec.width) or 280, tonumber(spec.height) or 52
    spec.width, spec.height = w, h
    state.knob = tonumber(spec.knob) or h
    if spec.extent == nil then spec.extent = math.max(1, w - state.knob) end
    state.extent = spec.extent
    state.done = false
    local confirmed_given = spec.on_confirmed
    local done_timer
    spec.on_confirmed = function(...)
      t.done = true
      if done_timer then done_timer:cancel() end
      done_timer = morf.timer(1200, function() done_timer = nil pcall(function() t.done = false end) end, false)
      if confirmed_given then confirmed_given(...) end
    end
  end
  if swipe_out or reveal then
    state.gone, state.open = "", false
    if spec.swipe_distance == nil then spec.swipe_distance = reveal and math.floor(reveal * 0.4) or 80 end
  end
  local REST = spec.pull_distance or 64
  local content
  -- (Not the node's: a handler lib.kit.control does not know goes to it.)
  local on_refresh = spec.on_refresh
  spec.on_refresh = nil
  if pulling then
    state.pull, state.refreshing = 0, false
    -- The content, moved down as it is pulled: the skin's slots stay.
    content = ui.Item { anchors = { fill = true }, behavior = { translate_y = ui.spring(FOLLOW) } }
    for _, child in ipairs(children) do ui.reparent(child, content) end
    children = { content }
    local refresh_given = spec.on_dropped
    spec.on_dropped = function(...)
      local pulled = t.pull
      t.pull = 0
      if pulled >= 1 and not t.refreshing then
        t.refreshing = true
        local function done() pcall(function() t.refreshing = false end) end
        if on_refresh then on_refresh(done) else morf.timer(1200, done, false) end
      end
      if refresh_given then refresh_given(...) end
    end
  end
  -- Right to left the trailing edge is the left one: a row opens swiped
  -- right and its actions are under its left edge.
  local function trailing() return (root ~= nil and root.effective_direction == "rtl") and -1 or 1 end
  if swipe_out or reveal then
    local swiped_given = spec.on_swiped
    spec.on_swiped = function(direction)
      local opening, shutting = "left", "right"
      if trailing() < 0 then opening, shutting = "right", "left" end
      if swipe_out then t.gone = direction
      elseif direction == opening then t.open = true
      elseif direction == shutting then t.open = false end
      if swiped_given then swiped_given(direction) end
    end
  end
  if spec.detents then
    local dropped_given = spec.on_dropped
    spec.on_dropped = function(v, ...)
      local best
      for _, d in ipairs(get(spec.detents)) do
        if best == nil or math.abs(d - v) < math.abs(best - v) then best = d end
      end
      if best then
        if type(given.value) ~= "function" then ctl.configure("value", best) end
        if spec.on_moved then spec.on_moved(best) end
      end
      if dropped_given then dropped_given(best or v, ...) end
    end
  end
  -- (The control's place, as the layout gives it: a followed control's
  -- offset leaves out how far the layout has moved it since the press.)
  local home_x, home_y = 0, 0
  if follow then
    local started_given = spec.on_drag_started
    spec.on_drag_started = function(...)
      home_x, home_y = tonumber(get(given.x)) or 0, tonumber(get(given.y)) or 0
      if started_given then started_given(...) end
    end
  end
  -- A followed control rides in a holder of its own, which takes the
  -- layout's place (and the behaviors a control's node has no room for).
  local holder
  if follow then
    holder = { width = spec.width, height = spec.height, visible = spec.visible }
    for _, k in ipairs { "x", "y", "anchors" } do holder[k] = spec[k] spec[k] = nil end
    spec.z, spec.behavior, spec.opacity = nil, nil, nil
    spec.anchors = { fill = true }
  end
  root, t, ctl = control.make("Drag", widget, spec, { state = state, children = children })
  if follow then
    local function offset(axis)
      if axis == "x" then
        if swipe_out and t.gone ~= "" then return (t.gone == "left" and -1 or 1) * ((t.width or 0) + 48) end
        if reveal then
          local k = trailing()
          local base = t.open and -reveal or 0
          if not t.active then return base * k end
          return k * math.max(-reveal - 24, math.min(24, base + k * t.delta_x))
        end
        if not t.active then return 0 end
        return t.delta_x - ((tonumber(get(given.x)) or 0) - home_x)
      end
      if not t.active or swipe_out or reveal then return 0 end
      return t.delta_y - ((tonumber(get(given.y)) or 0) - home_y)
    end
    holder.translate_x = function() return offset("x") end
    holder.translate_y = function() return offset("y") end
    holder.z = function() return t.active and 10 or (tonumber(get(given.z)) or 0) end
    local behavior = {}
    for k, v in pairs(given.behavior or {}) do behavior[k] = v end
    local spring = (given.behavior or {}).y or (given.behavior or {}).x or ui.spring(FOLLOW)
    behavior.translate_x, behavior.translate_y = spring, spring
    if swipe_out then
      behavior.opacity = behavior.opacity or { duration = 220, easing = "out_cubic" }
      holder.opacity = function() return t.gone ~= "" and 0 or 1 end
    end
    holder.behavior = behavior
    holder[1] = root
    holder = ui.Item(holder)
  end
  if pulling then
    content.translate_y = function()
      if t.refreshing then return REST end
      return t.active and t.pull * REST or 0
    end
    morf.effect("kit.drag.pull." .. ctl.id, function()
      if not t.active then return end
      -- A rubber band: 1 at the threshold, slower past it.
      local d = math.max(0, t.delta_y)
      local band = REST * 1.8
      t.pull = band * (1 - math.exp(-d / band)) / REST
    end, { owner = root })
  end
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
  return holder or root, t, ctl
end

--- Where a drag from anywhere lands; see `make`.
function M.drop_zone(spec)
  spec = spec or {}
  local own = {}
  for k, v in pairs(spec) do own[k] = v end
  own.mode, own.widget = "transfer", "drop_zone"
  own.on_dropped = nil
  local t
  local area = ui.DropArea { anchors = { fill = true }, keys = spec.keys or {},
    on_entered = function(info)
      if t then t.accepting = info == nil or info.accepted ~= nil end
      if spec.on_entered then spec.on_entered(info) end
    end,
    on_exited = function() if t then t.accepting = false end if spec.on_exited then spec.on_exited() end end,
    on_dropped = function(drop) if spec.on_dropped then spec.on_dropped(drop) end end }
  local root
  root, t = control.make("Drag", "drop_zone", own, { state = { accepting = false }, children = { area } })
  return root, t
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
