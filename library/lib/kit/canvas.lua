-- Canvases (the Canvas archetype): a world a viewport looks into, panned
-- and zoomed, holding items that are picked, selected, moved, connected
-- and drawn. A node graph, a whiteboard, a map, an image viewer, a
-- zoomable chart and a timeline are each one drawn differently.
--
--     local node, view = canvas.make("node_graph", {
--       width = 800, height = 600,
--       items = function() return graph.nodes end,   -- { id, x, y, w, h, shape?, ... }, world units
--       ports = function() return graph.ports end,   -- { id, item, x, y, kind = "in" | "out" }
--       wires = function() return graph.edges end,   -- { id, from = port id, to = port id }
--       delegate = function(item, s) return ui.Rect { anchors = { fill = true }, ... } end,
--       content = world_drawing,                     -- nodes in world units, under the items
--       overlay = zoom_buttons,                      -- nodes in screen units, over everything
--       tool = "select", grid = 16, snap = true,
--       on_moved = function(ids, dx, dy) end, on_connected = function(from, to) end,
--       on_drawn = function(tool, points) end, on_deleted = function(ids) end,
--       resizable = true, on_resized = function(id, x, y, w, h) end,  -- the one selected box, by its handles
--     })
--     view.fit()  view.zoom_by(2)  view.center_on(x, y)  view.select { "a" }
--     view.to_screen(x, y)  view.to_world(x, y)
--
-- The world is one node under a transform, so panning and zooming move a
-- drawing that stays drawn; the items are laid in it at their world
-- boxes (a rect or an ellipse at x, y, w, h; a line, polygon or point at
-- the origin, drawing its own points). A delegate draws an item -- it
-- takes no pointer: the canvas picks -- and is told `s.item()`,
-- `s.selected()`, `s.hovered()` and `s.zoom()`. Without a delegate the
-- skin's `item` builder draws it from its fields (`fill`, `stroke`,
-- `label`). Selected items move with a drag before the configuration
-- hears `on_moved` and moves them.
--
-- The skin draws `background` and `grid` under the world and `wires`
-- (a builder: a wire between two world points), `selection` (a builder:
-- the outline round a selected item, in screen units), `draft`, `band`,
-- `crosshair` and `overlay` over it.
local ui = require("morf.ui")
local morf = require("morf")
local control = require("lib.kit.control")

local M = {}

local function get(v) if type(v) == "function" then return v() end return v end

--- The numbers of a list kept in the live state (`t.draft`, `t.band`).
function M.numbers(encoded)
  local out = {}
  if type(encoded) == "string" then
    for part in encoded:gmatch("[^,]+") do out[#out + 1] = tonumber(part) end
  end
  return out
end

--- Path data a skin draws with, in screen units: `polyline(points,
--- closed)` (flat points), `curve(x0, y0, x1, y1)` (a wire leaving and
--- arriving level).
function M.polyline(points, closed)
  if #points < 4 then return "M0 0" end
  local parts = { ("M%.1f %.1f"):format(points[1], points[2]) }
  for i = 3, #points - 1, 2 do parts[#parts + 1] = ("L%.1f %.1f"):format(points[i], points[i + 1]) end
  if closed then parts[#parts + 1] = "Z" end
  return table.concat(parts, " ")
end

function M.curve(x0, y0, x1, y1)
  local d = math.max(30, math.abs(x1 - x0) / 2)
  return ("M%.1f %.1f C%.1f %.1f %.1f %.1f %.1f %.1f"):format(x0, y0, x0 + d, y0, x1 - d, y1, x1, y1)
end

--- The step a grid's lines stand at on screen along `axis` ("x" or "y"):
--- the world's grid (`base` when it has none) doubled or halved until
--- the lines are between `lo` (12) and `hi` (96) pixels apart.
function M.screen_step(t, axis, base, lo, hi)
  local g = (t.grid and t.grid > 0) and t.grid or (base or 50)
  local step = g * ((axis == "y" and t.zoom_y or t.zoom_x) or 1)
  if step <= 0 then return lo or 12 end
  while step < (lo or 12) do step = step * 2 end
  while step > (hi or 96) do step = step / 2 end
  return step
end

--- A grid that pans as one still drawing: its path is made again only as
--- the zoom moves its step (or the canvas its size); the pan only slides
--- it. `opts`: `kind` ("lines", "dots", "crosses": a plus `arm` px each
--- way at each crossing, "rows": horizontal lines only, "columns"), `every` (a line every so many steps: a major grid), `base`,
--- `lo`, `hi` (as `screen_step`), and any `ui.Path` properties
--- (`stroke_color`, `stroke_width`, `opacity`, `visible`, `id`).
function M.grid(t, opts)
  opts = opts or {}
  local ui_ = require("morf.ui")
  local kind, every = opts.kind or "lines", opts.every or 1
  local function step(axis) return M.screen_step(t, axis, opts.base, opts.lo, opts.hi) * every end
  local function offset(axis)
    local s = step(axis)
    local v = axis == "x" and (t.view_x or 0) * (t.zoom_x or 1) or (t.view_y or 0) * (t.zoom_y or 1)
    return -(v % s) - s
  end
  local function W() return (t.width or 0) + step("x") * 2 end
  local function H() return (t.height or 0) + step("y") * 2 end
  local props = {}
  for k, v in pairs(opts) do
    if k ~= "kind" and k ~= "every" and k ~= "base" and k ~= "lo" and k ~= "hi" and k ~= "arm" then props[k] = v end
  end
  props.x = function() return kind == "rows" and 0 or offset("x") end
  props.y = function() return kind == "columns" and 0 or offset("y") end
  props.width, props.height = W, H
  props.fill_color = "transparent"
  if kind == "dots" then props.stroke_cap = "round" end
  props.d = function()
    local sx, sy, w, h = step("x"), step("y"), W(), H()
    local parts = {}
    if kind == "dots" then
      for x = 0, w, sx do
        for y = 0, h, sy do parts[#parts + 1] = ("M%.1f %.1fh0.01"):format(x, y) end
      end
    elseif kind == "crosses" then
      local a = opts.arm or 4
      for x = 0, w, sx do
        for y = 0, h, sy do
          parts[#parts + 1] = ("M%.1f %.1fH%.1fM%.1f %.1fV%.1f"):format(x - a, y, x + a, x, y - a, y + a)
        end
      end
    else
      if kind ~= "rows" then for x = 0, w, sx do parts[#parts + 1] = ("M%.1f 0V%.1f"):format(x, h) end end
      if kind ~= "columns" then for y = 0, h, sy do parts[#parts + 1] = ("M0 %.1fH%.1f"):format(y, w) end end
    end
    return #parts > 0 and table.concat(parts, " ") or "M0 0"
  end
  return ui_.Path(props)
end

--- What a skin's `draft` slot draws, as screen path data: the shape a
--- drag is drawing (a rect, an ellipse, a line), or the points clicked
--- out so far reaching on to the pointer; "M0 0" while there is none.
function M.draft_path(t)
  local function at(x, y) return (x - t.view_x) * t.zoom_x, (y - t.view_y) * t.zoom_y end
  local pts = M.numbers(t.draft)
  local g = t.gesture
  if g == "draw" and #pts >= 4 then
    local x0, y0 = at(pts[1], pts[2])
    local x1, y1 = at(pts[3], pts[4])
    if t.tool == "rect" then
      return ("M%.1f %.1f H%.1f V%.1f H%.1f Z"):format(x0, y0, x1, y1, x0)
    elseif t.tool == "ellipse" then
      local cx, cy, rx, ry = (x0 + x1) / 2, (y0 + y1) / 2, math.abs(x1 - x0) / 2, math.abs(y1 - y0) / 2
      if rx < 0.5 or ry < 0.5 then return "M0 0" end
      return ("M%.1f %.1f A%.1f %.1f 0 1 1 %.1f %.1f A%.1f %.1f 0 1 1 %.1f %.1f Z"):format(cx - rx, cy, rx, ry,
        cx + rx, cy, rx, ry, cx - rx, cy)
    end
    return ("M%.1f %.1f L%.1f %.1f"):format(x0, y0, x1, y1)
  end
  if #pts >= 2 then
    local screen = {}
    for i = 1, #pts - 1, 2 do
      local x, y = at(pts[i], pts[i + 1])
      local n = #screen
      screen[n + 1], screen[n + 2] = x, y
    end
    if g == "draft" and t.tool ~= "freehand" and t.pointer_inside then
      local x, y = at(t.pointer_x, t.pointer_y)
      local n = #screen
      screen[n + 1], screen[n + 2] = x, y
    end
    if #screen >= 4 then return M.polyline(screen, false) end
  end
  return "M0 0"
end

--- The wire being pulled from a port, as screen path data (`spec` is the
--- skin's: its `port_point`); `shape(x0, y0, x1, y1)` draws it (a curve).
function M.pull_path(t, spec, shape)
  shape = shape or M.curve
  if t.gesture ~= "connect" or not spec.port_point then return "M0 0" end
  local fx, fy = spec.port_point(t.connect_from)
  if not fx then return "M0 0" end
  local x0, y0 = (fx - t.view_x) * t.zoom_x, (fy - t.view_y) * t.zoom_y
  local x1, y1 = (t.connect_x - t.view_x) * t.zoom_x, (t.connect_y - t.view_y) * t.zoom_y
  -- (Pulled backwards from an in port, the curve leaves the other way.)
  if x1 < x0 and spec.port and (spec.port(t.connect_from) or {}).kind == "in" then return shape(x1, y1, x0, y0) end
  return shape(x0, y0, x1, y1)
end

--- The rubber band (or brush, or zoom box) on screen: x, y, w, h.
function M.band_box(t)
  local b = M.numbers(t.band)
  if #b < 4 then return 0, 0, 0, 0 end
  local x0, y0 = (b[1] - t.view_x) * t.zoom_x, (b[2] - t.view_y) * t.zoom_y
  local x1, y1 = (b[3] - t.view_x) * t.zoom_x, (b[4] - t.view_y) * t.zoom_y
  return math.min(x0, x1), math.min(y0, y1), math.abs(x1 - x0), math.abs(y1 - y0)
end

--- The far corner of a flat list of points from the origin (a path's
--- box, so nothing it draws is cut): w, h.
function M.extent(points)
  local w, h = 1, 1
  for i = 1, #points - 1, 2 do w, h = math.max(w, points[i]), math.max(h, points[i + 1]) end
  return w + 1, h + 1
end

--- The middle of a flat list of points and its size: cx, cy, w, h.
function M.centre(points)
  local x0, y0, x1, y1 = math.huge, math.huge, -math.huge, -math.huge
  for i = 1, #points - 1, 2 do
    x0, x1 = math.min(x0, points[i]), math.max(x1, points[i])
    y0, y1 = math.min(y0, points[i + 1]), math.max(y1, points[i + 1])
  end
  if x0 == math.huge then return 0, 0, 0, 0 end
  return (x0 + x1) / 2, (y0 + y1) / 2, x1 - x0, y1 - y0
end

--- An arrowhead at a line's last point, `size` long, as path data.
function M.arrow_path(points, size)
  local n = #points
  if n < 4 then return "M0 0" end
  local x1, y1, x0, y0 = points[n - 1], points[n], points[n - 3], points[n - 2]
  local dx, dy = x1 - x0, y1 - y0
  local l = math.max(1e-6, math.sqrt(dx * dx + dy * dy))
  dx, dy = dx / l, dy / l
  local bx, by = x1 - dx * size, y1 - dy * size
  local px, py = -dy * size * 0.55, dx * size * 0.55
  return ("M%.2f %.2f L%.2f %.2f L%.2f %.2f Z"):format(x1, y1, bx + px, by + py, bx - px, by - py)
end

--- A series's value at `x` (flat x, y pairs, x rising; straight between
--- samples), or nil when it has none.
function M.value_at(series, x)
  local n = #series
  if n < 4 then return nil end
  if x <= series[1] then return series[2] end
  for i = 3, n - 1, 2 do
    if series[i] >= x then
      local x0, y0, x1, y1 = series[i - 2], series[i - 1], series[i], series[i + 1]
      local f = (x1 > x0) and (x - x0) / (x1 - x0) or 0
      return y0 + (y1 - y0) * f
    end
  end
  return series[n]
end

-- Seconds between a time ruler's numbered ticks and the steps between:
-- the shortest round span at least `min` (80) px long at `zoom`.
local SPANS = { { 1, 5 }, { 2, 4 }, { 5, 5 }, { 10, 5 }, { 15, 3 }, { 30, 6 }, { 60, 6 }, { 120, 4 }, { 300, 5 },
  { 600, 5 }, { 900, 3 }, { 1800, 6 }, { 3600, 6 } }
function M.time_span(zoom, min)
  for _, sp in ipairs(SPANS) do if sp[1] * zoom >= (min or 80) then return sp[1], sp[2] end end
  return SPANS[#SPANS][1], SPANS[#SPANS][2]
end

--- Seconds as a clock reads them: "m:ss", or "h:mm:ss".
function M.stamp(seconds)
  seconds = math.floor(seconds + 0.5)
  if seconds >= 3600 then return ("%d:%02d:%02d"):format(seconds // 3600, seconds // 60 % 60, seconds % 60) end
  return ("%d:%02d"):format(seconds // 60, seconds % 60)
end

--- The longest round distance (metres) under `max` px at `zoom` (px a
--- metre), and its length on screen.
function M.scale_bar(zoom, max)
  local per_px = 1 / math.max(1e-12, zoom)
  local best = 1
  for _, m in ipairs { 1, 2, 5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000, 50000, 100000, 200000,
    500000, 1000000 } do
    if m / per_px <= (max or 120) then best = m end
  end
  return best, best / per_px, best >= 1000 and ("%g km"):format(best / 1000) or ("%d m"):format(best)
end

--- Restyles a skin a theme took from another (the default look's
--- layouts in a theme's palette): `S[name]` becomes the old skin with
--- `change(slots, t, spec, send)` run over what it gave. A slot `change`
--- replaces or clears is destroyed, unless `change` returns it in a list
--- of nodes it kept (put inside its own).
function M.restyle(S, name, change)
  local base = S[name]
  S[name] = function(t, spec, node, send)
    local slots = base and base(t, spec, node, send) or {}
    local before = {}
    for k, v in pairs(slots) do before[k] = v end
    local kept = {}
    for _, n in ipairs(change(slots, t, spec, send) or {}) do kept[n] = true end
    for k, v in pairs(before) do
      if type(v) ~= "function" and slots[k] ~= v and not kept[v] then require("morf.ui").destroy(v, true) end
    end
    return slots
  end
end

--- A list of items or ports given as a value or a binding, read now.
function M.list(v) if type(v) == "function" then return v() or {} end return v or {} end

--- The ids of a list kept in the live state (`t.selection`).
function M.ids(encoded)
  local out = {}
  if type(encoded) == "string" then
    for part in encoded:gmatch("[^,]+") do out[#out + 1] = part end
  end
  return out
end

-- An item's box, in world units, as the archetype reads it.
local function box(item)
  local shape = item.shape or "rect"
  if shape == "line" or shape == "polygon" then
    local pts = item.points or {}
    local x0, y0, x1, y1 = math.huge, math.huge, -math.huge, -math.huge
    for i = 1, #pts - 1, 2 do
      x0, x1 = math.min(x0, pts[i]), math.max(x1, pts[i])
      y0, y1 = math.min(y0, pts[i + 1]), math.max(y1, pts[i + 1])
    end
    if x0 == math.huge then return 0, 0, 0, 0 end
    return x0, y0, x1 - x0, y1 - y0
  elseif shape == "point" then
    return item.x or 0, item.y or 0, 0, 0
  end
  return item.x or 0, item.y or 0, item.w or 0, item.h or 0
end
M.box = box

function M.make(widget, spec)
  spec = spec or {}
  local t, ctl, root
  local by_id, port_at = {}, {}
  local full = {}
  for k, v in pairs(spec) do full[k] = v end
  full.widget = widget
  -- The skin is told how to find a box and where a world point shows.
  function full.item_box(id) local e = by_id[tostring(id)] if e then return box(e.item) end end
  function full.to_screen(x, y) return (x - t.view_x) * t.zoom_x, (y - t.view_y) * t.zoom_y end
  function full.port(id) return port_at[tostring(id)] end
  -- (Content and overlay are the canvas's to place, not the node's children.)
  full.content, full.overlay = nil, nil
  local rebuilt
  root, t, ctl = control.make("Canvas", widget, full, {
    props = { clip = true, accepted_buttons = { "left", "middle", "right" },
      -- Over a handle, the way it resizes.
      cursor = function()
        local h = t and t.hovered_handle or ""
        if h ~= "" then return h .. "_resize" end
        return get(spec.cursor) or "default"
      end },
    builders = { item = true, selection = true, wires = true, grip = true },
    -- (The band is nil while there is none; known from the start, a skin
    -- may read it.)
    state = { band = "" },
    on_rebuild = function() if rebuilt then rebuilt() end end,
  })
  local send = ctl.send
  -- The pointer's other news: where it hovers, a double click, the wheel
  -- with what is held, a pinch.
  root.on_position_changed = function(_, _, _, _, x, y) send("hover", x, y) end
  root.on_double_clicked = function(sx, sy, x, y)
    send("double_clicked", x, y)
    if spec.on_double_clicked then spec.on_double_clicked(sx, sy, x, y) end
  end
  root.on_wheel = function(_, _, px, py, step_x, step_y, x, y, modifiers)
    send("wheel", step_x or 0, step_y or 0, px or 0, py or 0, x or 0, y or 0, modifiers or "")
  end
  root.on_pinched = function(scale, phase, x, y)
    send("pinch", scale, phase, x - (root.layout_x or 0), y - (root.layout_y or 0))
  end
  morf.effect("kit.canvas.size." .. ctl.id, function()
    send("resize", root.layout_width or 0, root.layout_height or 0)
  end, { owner = root })

  -- The world: one node, the view's transform on it.
  local world = ui.Item { width = 1, height = 1, transform_origin_x = 0, transform_origin_y = 0,
    transform_matrix = function()
      return { t.zoom_x, 0, 0, t.zoom_y, -t.view_x * t.zoom_x, -t.view_y * t.zoom_y }
    end }
  if spec.content then ui.reparent(spec.content, world) end
  local wires_layer = ui.Item {}
  local items_layer = ui.Item {}
  ui.reparent(wires_layer, world)
  ui.reparent(items_layer, world)
  ui.reparent(world, root)
  -- Screen-space decorations: the outlines of what is selected.
  local marks = ui.Item {}
  ui.reparent(marks, root)

  local function moving(e)
    return t.gesture == "move" and control.has(t.selection, e.key)
  end
  local function item_state(e)
    return {
      item = function() e.version:get() return e.item end,
      selected = function() return control.has(t.selection, e.key) end,
      hovered = function() return t.hovered == e.key end,
      zoom = function() return t.zoom end,
      id = e.key,
    }
  end
  -- The box a resize under way has reached for this item, or nil.
  local function resizing(e)
    if t.gesture ~= "resize" or t.resize_id ~= e.key then return nil end
    local r = M.numbers(t.resize)
    if #r < 4 then return nil end
    return r
  end
  local function place(e)
    local shape = e.item.shape or "rect"
    local spread = shape == "rect" or shape == "ellipse" or shape == "circle"
    local function at(i, field)
      e.version:get()
      if not spread then return 0 end
      local r = resizing(e)
      if r then return r[i] end
      return e.item[field] or 0
    end
    e.holder = ui.Item {
      x = function() return at(1, "x") end,
      y = function() return at(2, "y") end,
      width = function() return at(3, "w") end,
      height = function() return at(4, "h") end,
      translate_x = function() return moving(e) and t.move_dx or 0 end,
      translate_y = function() return moving(e) and t.move_dy or 0 end,
    }
    local draw = spec.delegate or (ctl.builders() or {}).item
    local node = draw and draw(e.item, item_state(e))
    if node then ui.reparent(node, e.holder) end
    ui.reparent(e.holder, items_layer)
  end
  -- The items, kept by id: one new is placed, one gone goes, one changed
  -- tells its delegate.
  morf.effect("kit.canvas.items." .. ctl.id, function()
    local list = get(spec.items) or {}
    local seen = {}
    for _, item in ipairs(list) do
      local key = tostring(item.id)
      seen[key] = true
      local e = by_id[key]
      if not e then
        e = { key = key, item = item, count = 0,
          version = morf.signal("kit.canvas.item." .. ctl.id .. "." .. key, 0) }
        by_id[key] = e
        place(e)
      elseif e.item ~= item then
        e.item = item
        e.count = e.count + 1
        e.version:set(e.count)
      end
    end
    for key, e in pairs(by_id) do
      if not seen[key] then
        ui.destroy(e.holder)
        if e.mark then ui.destroy(e.mark) end
        by_id[key] = nil
      end
    end
    ctl.configure("items", list)
  end, { owner = root })

  -- Ports, by id, for the wires and the skin.
  morf.effect("kit.canvas.ports." .. ctl.id, function()
    local list = get(spec.ports) or {}
    port_at = {}
    for _, port in ipairs(list) do port_at[tostring(port.id)] = port end
    ctl.configure("ports", list)
  end, { owner = root })

  -- A port's place, as a drag moves its item.
  local function port_point(id)
    local port = port_at[tostring(id)]
    if not port then return nil end
    local x, y = port.x, port.y
    local e = port.item ~= nil and by_id[tostring(port.item)]
    if e and moving(e) then x, y = x + (t.move_dx or 0), y + (t.move_dy or 0) end
    return x, y
  end
  full.port_point = port_point

  -- Wires, kept by id, drawn by the skin's builder between their ports.
  local wires = {}
  -- (Bumped when the theme changes: the items, wires and outlines are
  -- drawn again by the new skin.)
  local generation = morf.signal("kit.canvas.generation." .. ctl.id, 0)
  if spec.wires then
    morf.effect("kit.canvas.wires." .. ctl.id, function()
      generation:get()
      local list = get(spec.wires) or {}
      local seen = {}
      for i, wire in ipairs(list) do
        local key = tostring(wire.id or (tostring(wire.from) .. ">" .. tostring(wire.to)))
        seen[key] = true
        if not wires[key] then
          local w = { wire = wire }
          local build = (ctl.builders() or {}).wires
          w.node = build and build(function()
            local x0, y0 = port_point(w.wire.from)
            local x1, y1 = port_point(w.wire.to)
            if not x0 or not x1 then return nil end
            return x0, y0, x1, y1
          end, { wire = function() return w.wire end, zoom = function() return t.zoom end })
          if w.node then ui.reparent(w.node, wires_layer) end
          wires[key] = w
        else
          wires[key].wire = wire
        end
      end
      for key, w in pairs(wires) do
        if not seen[key] then if w.node then ui.destroy(w.node) end wires[key] = nil end
      end
    end, { owner = root })
  end

  -- The outline round each selected item, in screen units so its line
  -- stays one width at any zoom.
  morf.effect("kit.canvas.selection." .. ctl.id, function()
    generation:get()
    local selected = {}
    for _, key in ipairs(M.ids(t.selection)) do selected[key] = true end
    local build = (ctl.builders() or {}).selection
    for key, e in pairs(by_id) do
      if selected[key] and not e.mark and build then
        local function screen_box()
          e.version:get()
          local x, y, w, h = box(e.item)
          local r = resizing(e)
          if r then x, y, w, h = r[1], r[2], r[3], r[4] end
          if moving(e) then x, y = x + t.move_dx, y + t.move_dy end
          return (x - t.view_x) * t.zoom_x, (y - t.view_y) * t.zoom_y, w * t.zoom_x, h * t.zoom_y
        end
        e.mark = ui.Item {
          x = function() local x = screen_box() return x end,
          y = function() local _, y = screen_box() return y end,
          -- (A point has no box: a pixel, so what it holds is laid out.)
          width = function() local _, _, w = screen_box() return math.max(1, w) end,
          height = function() local _, _, _, h = screen_box() return math.max(1, h) end,
        }
        local node = build(item_state(e))
        if node then ui.reparent(node, e.mark) end
        -- The handles of a box that resizes, while it is the only one
        -- selected: the skin's grips at its corners and edges.
        local grip = (ctl.builders() or {}).grip
        local shape = e.item.shape or "rect"
        if grip and get(spec.resizable) and (shape == "rect" or shape == "ellipse" or shape == "circle") then
          local function sole() return t.selected_count == 1 end
          for _, name in ipairs { "nw", "n", "ne", "e", "se", "s", "sw", "w" } do
            local fx = name:find("w") and 0 or (name:find("e") and 1 or 0.5)
            local fy = name:sub(1, 1) == "n" and 0 or (name:sub(1, 1) == "s" and 1 or 0.5)
            local g = grip({ name = name,
              hovered = function() return t.hovered_handle == name end,
              held = function() return t.gesture == "resize" and t.resize_id == e.key end })
            if g then
              local holder = ui.Item { width = 0, height = 0, visible = sole,
                x = function() return (e.mark.layout_width or 0) * fx end,
                y = function() return (e.mark.layout_height or 0) * fy end }
              ui.reparent(g, holder)
              ui.reparent(holder, e.mark)
            end
          end
        end
        ui.reparent(e.mark, marks)
      elseif not selected[key] and e.mark then
        ui.destroy(e.mark)
        e.mark = nil
      end
    end
  end, { owner = root })

  -- The skin's grid goes under the world, the rest over it; the
  -- configuration's overlay over all.
  -- (The skin's slots were made before the world was put in, so each is
  -- told where it stands: the ground, grid and content under the world,
  -- the outlines over it, then the draft, the band and the crosshair, the
  -- skin's overlay over those.)
  marks.z = 4
  local function settle()
    local s = ctl.slots and ctl.slots() or {}
    if s.grid then s.grid.z = -1 end
    if s.content then s.content.z = -1 end
    for _, name in ipairs { "draft", "band", "crosshair" } do if s[name] then s[name].z = 5 end end
    if s.overlay then s.overlay.z = 6 end
  end
  rebuilt = function()
    settle()
    for _, e in pairs(by_id) do
      ui.destroy(e.holder)
      if e.mark then ui.destroy(e.mark) e.mark = nil end
      place(e)
    end
    for key, w in pairs(wires) do if w.node then ui.destroy(w.node) end wires[key] = nil end
    generation:set(generation:get() + 1)
  end
  settle()
  if spec.overlay then
    local holder = ui.Item { anchors = { fill = true }, z = 10 }
    ui.reparent(spec.overlay, holder)
    ui.reparent(holder, root)
  end

  local view = { node = root, t = t, send = send }
  function view.fit() send("fit") end
  function view.zoom_by(factor, x, y) send("zoom_by", factor, x, y) end
  function view.set_view(x, y, zoom) send("set_view", x, y, zoom) end
  function view.center_on(x, y) send("center_on", x, y) end
  function view.select(ids) ctl.configure("selection", ids or {}) end
  function view.set_tool(name) ctl.configure("tool", name) end
  function view.cancel() send("cancel") end
  function view.to_screen(x, y) return full.to_screen(x, y) end
  function view.to_world(x, y) return t.view_x + x / t.zoom_x, t.view_y + y / t.zoom_y end
  function view.selection() return M.ids(t.selection) end
  return root, view
end

return M
