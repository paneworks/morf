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
  root, t, ctl = control.make("Canvas", widget, full, {
    props = { clip = true, accepted_buttons = { "left", "middle", "right" }, cursor = spec.cursor },
    builders = { item = true, selection = true, wires = true },
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
  local world = ui.Item { transform_origin_x = 0, transform_origin_y = 0,
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
  local function place(e)
    local shape = e.item.shape or "rect"
    local spread = shape == "rect" or shape == "ellipse" or shape == "circle"
    e.holder = ui.Item {
      x = function() e.version:get() return spread and (e.item.x or 0) or 0 end,
      y = function() e.version:get() return spread and (e.item.y or 0) or 0 end,
      width = function() e.version:get() return spread and (e.item.w or 0) or 0 end,
      height = function() e.version:get() return spread and (e.item.h or 0) or 0 end,
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
        e = { key = key, item = item, version = morf.signal("kit.canvas.item." .. ctl.id .. "." .. key, 0) }
        by_id[key] = e
        place(e)
      elseif e.item ~= item then
        e.item = item
        e.version:set(e.version:get() + 1)
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
  if spec.wires then
    morf.effect("kit.canvas.wires." .. ctl.id, function()
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
    local selected = {}
    for _, key in ipairs(M.ids(t.selection)) do selected[key] = true end
    local build = (ctl.builders() or {}).selection
    for key, e in pairs(by_id) do
      if selected[key] and not e.mark and build then
        local function screen_box()
          e.version:get()
          local x, y, w, h = box(e.item)
          if moving(e) then x, y = x + t.move_dx, y + t.move_dy end
          return (x - t.view_x) * t.zoom_x, (y - t.view_y) * t.zoom_y, w * t.zoom_x, h * t.zoom_y
        end
        e.mark = ui.Item {
          x = function() local x = screen_box() return x end,
          y = function() local _, y = screen_box() return y end,
          width = function() local _, _, w = screen_box() return w end,
          height = function() local _, _, _, h = screen_box() return h end,
        }
        local node = build(item_state(e))
        if node then ui.reparent(node, e.mark) end
        ui.reparent(e.mark, marks)
      elseif not selected[key] and e.mark then
        ui.destroy(e.mark)
        e.mark = nil
      end
    end
  end, { owner = root })

  -- The skin's grid goes under the world, the rest over it; the
  -- configuration's overlay over all.
  local function settle()
    local s = ctl.slots and ctl.slots() or {}
    if s.grid then s.grid.z = -1 end
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
