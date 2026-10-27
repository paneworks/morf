-- The editor's canvas models: a node graph's items, ports and wires, and a
-- small district as vector map layers. The data carries no colours: items
-- name a `tone` ("accent", "success", "warning", "error", "info", "extra")
-- or, on a map, a `layer`, and the theme's skin colours them.
local S = {}
local serial = 0
local function key(name) serial = serial + 1 return "editor.canvas." .. name .. "." .. serial end

-- ------------------------------------------------------------ node graph --

local HEADER, ROW = 30, 24

--- A node graph's model from node definitions: the items, their ports
--- (inputs down the left edge, outputs down the right, a row each) and
--- the wires.
function S.graph(defs, edges)
  local nodes = {}
  for _, d in ipairs(defs) do
    local rows = math.max(#(d.inputs or {}), #(d.outputs or {}))
    nodes[#nodes + 1] = { id = d.id, x = d.x, y = d.y, w = d.w or 150, h = HEADER + rows * ROW + 10,
      title = d.title, tone = d.tone, inputs = d.inputs, outputs = d.outputs }
  end
  local items = morf.signal(key("graph"), nodes)
  local wires = morf.signal(key("wires"), edges or {})
  local function ports()
    local out = {}
    for _, n in ipairs(items:get()) do
      for i, label in ipairs(n.inputs or {}) do
        out[#out + 1] = { id = n.id .. "." .. label, item = n.id, kind = "in", label = label,
          x = n.x, y = n.y + HEADER + (i - 1) * ROW + ROW / 2 + 2 }
      end
      for i, label in ipairs(n.outputs or {}) do
        out[#out + 1] = { id = n.id .. "." .. label, item = n.id, kind = "out", label = label,
          x = n.x + n.w, y = n.y + HEADER + (i - 1) * ROW + ROW / 2 + 2 }
      end
    end
    return out
  end
  local model = { items = items, wires = wires, ports = ports }
  function model.on_moved(ids, dx, dy)
    local moved = {}
    for _, id in ipairs(ids) do moved[id] = true end
    local out = {}
    for _, n in ipairs(items:get()) do
      if moved[n.id] then
        local copy = {}
        for k, v in pairs(n) do copy[k] = v end
        copy.x, copy.y = n.x + dx, n.y + dy
        out[#out + 1] = copy
      else
        out[#out + 1] = n
      end
    end
    items:set(out)
  end
  function model.on_connected(from, to)
    local list = {}
    -- An input takes one wire: a new one replaces the old.
    for _, w in ipairs(wires:get()) do if w.to ~= to then list[#list + 1] = w end end
    list[#list + 1] = { id = from .. ">" .. to, from = from, to = to }
    wires:set(list)
  end
  function model.on_deleted(ids)
    local gone = {}
    for _, id in ipairs(ids) do gone[id] = true end
    local keep = {}
    for _, n in ipairs(items:get()) do if not gone[n.id] then keep[#keep + 1] = n end end
    items:set(keep)
    local w2 = {}
    for _, w in ipairs(wires:get()) do
      if not gone[w.from:match("^[^.]+")] and not gone[w.to:match("^[^.]+")] then w2[#w2 + 1] = w end
    end
    wires:set(w2)
  end
  return model
end

-- ------------------------------------------------------------------ map --

--- A small district as vector layers -- water, parks, blocks, roads, a
--- route and places -- in metres; the skin colours each `layer`.
function S.district()
  local items = {
    { id = "lake", layer = "water", shape = "polygon", selectable = false,
      points = { 620, 80, 760, 60, 880, 120, 900, 260, 820, 340, 700, 320, 600, 220 } },
    { id = "river", layer = "water", shape = "line", width = 14, selectable = false,
      points = { 0, 520, 180, 480, 340, 500, 520, 420, 640, 320 } },
    { id = "park", layer = "park", shape = "polygon", selectable = false,
      points = { 120, 120, 380, 100, 420, 300, 160, 340 } },
    { id = "wood", layer = "park", shape = "polygon", selectable = false,
      points = { 760, 520, 1000, 480, 1060, 700, 820, 760 } },
  }
  -- City blocks along a grid of streets.
  local n = 0
  for bx = 0, 4 do
    for by = 0, 2 do
      local x, y = 460 + bx * 110, 520 + by * 90
      if not (bx >= 3 and by <= 1) then
        n = n + 1
        items[#items + 1] = { id = "block-" .. n, layer = "building", shape = "polygon", selectable = false,
          points = { x, y, x + 86, y, x + 86, y + 66, x, y + 66 } }
      end
    end
  end
  for _, road in ipairs {
    { id = "ring", width = 8, points = { 40, 60, 460, 40, 560, 380, 440, 470, 60, 420, 40, 60 } },
    { id = "avenue", width = 10, points = { 0, 360, 450, 400, 1100, 470 } },
    { id = "north", width = 6, points = { 450, 0, 450, 1000 } },
    { id = "east", width = 6, points = { 560, 500, 560, 1000 } },
    { id = "cross-1", width = 4, points = { 440, 600, 1100, 600 } },
    { id = "cross-2", width = 4, points = { 440, 690, 1100, 690 } },
  } do
    items[#items + 1] = { id = road.id, layer = "road", shape = "line", width = road.width, points = road.points,
      selectable = false }
  end
  items[#items + 1] = { id = "route", layer = "route", shape = "line", width = 5,
    points = { 200, 220, 300, 380, 450, 400, 500, 560, 680, 600, 760, 420, 780, 300 } }
  for _, place in ipairs {
    { id = "station", x = 450, y = 400, label = "Station" },
    { id = "cafe", x = 200, y = 220, label = "Café" },
    { id = "pier", x = 780, y = 300, label = "Pier" },
    { id = "school", x = 680, y = 600, label = "School" },
  } do
    items[#items + 1] = { id = place.id, layer = "place", shape = "point", x = place.x, y = place.y, r = 7,
      label = place.label }
  end
  return items
end

return S
