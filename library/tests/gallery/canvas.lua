-- Gallery samples for the Canvas archetype's widgets: each a canvas as it
-- is used -- a node graph of a few operators wired together, a whiteboard
-- with strokes and notes, a flow diagram, a map of a lake district, an
-- image under inspection, a zoomable chart, a timeline of clips, a pixel
-- board. The data carries no colours: items name a `tone` ("accent",
-- "success", "warning", "error", "info", "extra") or, on a map, a `layer`,
-- and the theme's skin colours them.
local ui = require("morf.ui")

local S = { span = {} }
local serial = 0
local function key(name) serial = serial + 1 return "kit.sample.canvas." .. name .. "." .. serial end

-- Items kept in a signal that a move moves (the canvas only reports it).
local function movable(name, list, ports_of)
  local items = morf.signal(key(name), list)
  local function on_moved(ids, dx, dy)
    local moved = {}
    for _, id in ipairs(ids) do moved[id] = true end
    local out = {}
    for _, it in ipairs(items:get()) do
      if moved[it.id] then
        local copy = {}
        for k, v in pairs(it) do copy[k] = v end
        copy.x, copy.y = (it.x or 0) + dx, (it.y or 0) + dy
        if it.points then
          copy.points = {}
          for i = 1, #it.points - 1, 2 do copy.points[i], copy.points[i + 1] = it.points[i] + dx, it.points[i + 1] + dy end
        end
        out[#out + 1] = copy
      else
        out[#out + 1] = it
      end
    end
    items:set(out)
  end
  return items, on_moved
end

-- ------------------------------------------------------------ node graph --

local HEADER, ROW = 30, 24

--- A node graph's model from node definitions: the items, their ports
--- (inputs down the left edge, outputs down the right, a row each) and
--- the wires. Shared with the editor example.
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

function S.node_graph(_, w)
  local g = S.graph({
    { id = "image", title = "Image", tone = "info", x = 16, y = 32, outputs = { "Color", "Alpha" } },
    { id = "noise", title = "Noise", tone = "extra", x = 16, y = 236, inputs = { "Scale" }, outputs = { "Value" } },
    { id = "blur", title = "Blur", tone = "accent", x = 224, y = 64, inputs = { "Image", "Radius" }, outputs = { "Result" } },
    { id = "mix", title = "Mix", tone = "warning", x = 416, y = 176, w = 160,
      inputs = { "A", "B", "Factor" }, outputs = { "Out" } },
  }, {
    { from = "image.Color", to = "blur.Image" },
    { from = "noise.Value", to = "blur.Radius" },
    { from = "blur.Result", to = "mix.A" },
    { from = "image.Alpha", to = "mix.Factor" },
  })
  return w.node_graph { id = "sample-node-graph", width = 600, height = 480,
    items = function() return g.items:get() end, ports = g.ports, wires = function() return g.wires:get() end,
    selection = { "blur" },
    on_moved = g.on_moved, on_connected = g.on_connected, on_deleted = g.on_deleted }
end
S.span.node_graph = { 2, 2 }

-- --------------------------------------------------------- zoomable canvas --

function S.zoomable_canvas(_, w)
  local items, on_moved = movable("board", {
    { id = "brief", x = 24, y = 28, w = 200, h = 120, label = "Brief", tone = "info" },
    { id = "moods", x = 256, y = 28, w = 150, h = 150, shape = "ellipse", label = "Moods", tone = "extra" },
    { id = "type", x = 436, y = 40, w = 130, h = 90, label = "Type", tone = "accent" },
    { id = "palette", x = 24, y = 184, w = 200, h = 96, label = "Palette", tone = "warning" },
    { id = "logo", x = 256, y = 216, w = 160, h = 110, label = "Logo", tone = "success" },
    { id = "notes", shape = "polygon", points = { 450, 190, 570, 210, 548, 330, 440, 312 }, tone = "error" },
  })
  return w.zoomable_canvas { id = "sample-zoomable-canvas", width = 600, height = 480,
    items = function() return items:get() end, on_moved = on_moved, zoom = 0.9, view_x = -24, view_y = -40 }
end
S.span.zoomable_canvas = { 2, 2 }

-- ------------------------------------------------------------ whiteboard --

-- A stroke as a hand draws it: points along a wobbly path.
local function scribble(x, y, len, rise, wobble, phase)
  local pts = {}
  for i = 0, 24 do
    local f = i / 24
    pts[#pts + 1] = x + f * len
    pts[#pts + 1] = y + rise * f + math.sin(f * math.pi * 3 + (phase or 0)) * wobble
  end
  return pts
end
local function loop(cx, cy, rx, ry)
  local pts = {}
  for i = 0, 30 do
    local a = i / 30 * math.pi * 2.1
    pts[#pts + 1] = cx + math.cos(a) * rx * (1 + 0.04 * math.sin(a * 3))
    pts[#pts + 1] = cy + math.sin(a) * ry
  end
  return pts
end

function S.whiteboard(_, w)
  local items, on_moved = movable("whiteboard", {
    { id = "note-1", x = 28, y = 30, w = 150, h = 110, label = "Ship the dock", tone = "warning", kind = "note" },
    { id = "note-2", x = 196, y = 44, w = 150, h = 110, label = "Pan & zoom", tone = "info", kind = "note" },
    { id = "note-3", x = 410, y = 300, w = 150, h = 110, label = "Ask design", tone = "success", kind = "note" },
    { id = "ring", shape = "line", points = loop(270, 100, 100, 74), width = 3, tone = "error" },
    { id = "arrow", shape = "line", points = scribble(360, 160, 120, 130, 6, 0.5), width = 3, tone = "error" },
    { id = "under", shape = "line", points = scribble(40, 190, 290, 8, 3, 1), width = 4, tone = "accent" },
    { id = "wave", shape = "line", points = scribble(40, 330, 300, -40, 18, 0), width = 3, tone = "extra" },
    { id = "box", x = 60, y = 380, w = 220, h = 60, shape = "rect", kind = "frame", label = "Next week" },
  })
  return w.whiteboard { id = "sample-whiteboard", width = 600, height = 480,
    items = function() return items:get() end, on_moved = on_moved }
end
S.span.whiteboard = { 2, 2 }

-- -------------------------------------------------------------- diagram --

function S.diagram(_, w)
  local items, on_moved = movable("diagram", {
    { id = "start", x = 220, y = 20, w = 160, h = 50, shape = "ellipse", label = "Request", tone = "success" },
    { id = "auth", x = 210, y = 110, w = 180, h = 60, label = "Authenticate" },
    { id = "ok", shape = "polygon", points = { 300, 210, 380, 250, 300, 290, 220, 250 }, label = "Valid?", tone = "warning" },
    { id = "serve", x = 400, y = 330, w = 170, h = 60, label = "Serve page" },
    { id = "deny", x = 30, y = 330, w = 170, h = 60, label = "Deny", tone = "error" },
    { id = "e1", shape = "line", points = { 300, 70, 300, 110 }, width = 2, arrow = true, selectable = false },
    { id = "e2", shape = "line", points = { 300, 170, 300, 210 }, width = 2, arrow = true, selectable = false },
    { id = "e3", shape = "line", points = { 380, 250, 485, 250, 485, 330 }, width = 2, arrow = true, selectable = false },
    { id = "e4", shape = "line", points = { 220, 250, 115, 250, 115, 330 }, width = 2, arrow = true, selectable = false },
  })
  return w.diagram { id = "sample-diagram", width = 600, height = 480,
    items = function() return items:get() end, on_moved = on_moved, selection = { "auth" }, view_y = -20 }
end
S.span.diagram = { 2, 2 }

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

function S.map_view(_, w)
  local items = S.district()
  return w.map_view { id = "sample-map-view", width = 600, height = 480, items = items,
    zoom = 0.62, view_x = 60, view_y = 40, selection = { "pier" }, attribution = "© Sample Atlas" }
end
S.span.map_view = { 2, 2 }

-- --------------------------------------------------------- image viewer --

--- A picture made of shapes -- dusk over hills and water -- as the
--- image the viewer inspects (an application gives an `Image`). Its own
--- colours: it is a picture, not part of the look.
function S.picture(width, height)
  local W, H = width or 640, height or 420
  local horizon = H * 0.62
  return ui.Item { width = W, height = H,
    ui.Rect { width = W, height = horizon + 2,
      gradient = { angle = 180, stops = { "#2a3d7a", "#8d5a9e", "#f3a46b" } } },
    ui.Rect { x = W * 0.62, y = horizon - 110, width = 92, height = 92, radius = 46, color = "#ffd88a" },
    ui.Path { width = W, height = H, view_box = { 0, 0, W, H }, fill_color = "#4b3a6e",
      d = ("M0 %.0f C%.0f %.0f %.0f %.0f %.0f %.0f S%.0f %.0f %.0f %.0f V%.0f H0 Z"):format(horizon - 40,
        W * 0.18, horizon - 120, W * 0.32, horizon - 30, W * 0.46, horizon - 70, W * 0.8, horizon - 150, W, horizon - 60,
        horizon + 2) },
    ui.Path { width = W, height = H, view_box = { 0, 0, W, H }, fill_color = "#2d2747",
      d = ("M0 %.0f C%.0f %.0f %.0f %.0f %.0f %.0f S%.0f %.0f %.0f %.0f V%.0f H0 Z"):format(horizon - 10,
        W * 0.2, horizon - 60, W * 0.4, horizon + 5, W * 0.6, horizon - 30, W * 0.85, horizon - 10, W, horizon - 40,
        horizon + 2) },
    ui.Rect { y = horizon, width = W, height = H - horizon,
      gradient = { angle = 180, stops = { "#e39468", "#5a3f7d", "#1d2347" } } },
    ui.Rect { x = W * 0.62 + 10, y = horizon + 14, width = 72, height = 4, radius = 2, color = "#ffd88a", opacity = 0.7 },
    ui.Rect { x = W * 0.62 + 22, y = horizon + 30, width = 48, height = 3, radius = 1.5, color = "#ffd88a", opacity = 0.5 },
  }
end

function S.image_viewer(_, w)
  local picture = S.picture(640, 420)
  return w.image_viewer { id = "sample-image-viewer", width = 600, height = 480, content = picture,
    image_size = { 640, 420 }, zoom = 0.78, view_x = -64, view_y = -64,
    items = { { id = "sun", x = 390, y = 150, w = 112, h = 112, shape = "ellipse", label = "Sun" } } }
end
S.span.image_viewer = { 2, 2 }

-- ------------------------------------------------------- chart inspector --

--- A day of readings, one every ten minutes: x in minutes, y the value.
function S.series(n, seed)
  local pts = {}
  local v = 42
  for i = 0, (n or 144) - 1 do
    v = v + math.sin(i * 0.21 + (seed or 0)) * 2.2 + math.cos(i * 0.057) * 1.4
    pts[#pts + 1] = i * 10
    pts[#pts + 1] = v + 18 * math.sin(i / 24)
  end
  return pts
end

function S.chart_inspector(_, w)
  local series = S.series(144, 1)
  return w.chart_inspector { id = "sample-chart-inspector", width = 1240, height = 180,
    series = series, unit = "°C", value_from = 0, value_to = 100,
    -- Minutes across a day, values down from the top.
    zoom = 1240 / 1440, view_x = 0, view_y = 0, bounds = { 0, 0, 1440, 100 },
    readout = function(x, y) return ("%02d:%02d  %.1f °C"):format(math.floor(x / 60) % 24, math.floor(x % 60), y) end }
end
S.span.chart_inspector = { 4, 1 }

-- -------------------------------------------------------------- timeline --

function S.timeline_track(_, w)
  local TRACK = 44
  local clip = function(id, track, from, to, label, tone)
    return { id = id, x = from, y = 30 + (track - 1) * TRACK + 4, w = to - from, h = TRACK - 8, label = label, tone = tone }
  end
  local items, on_moved = movable("timeline", {
    clip("intro", 1, 0, 180, "Intro", "info"), clip("talk", 1, 190, 640, "Interview", "info"),
    clip("broll", 1, 650, 900, "B-roll", "info"),
    clip("title", 2, 30, 220, "Title card", "extra"), clip("lower", 2, 260, 460, "Lower third", "extra"),
    clip("music", 3, 0, 900, "Score", "success"), clip("vo", 3, 200, 520, "Voice-over", "warning"),
  })
  return w.timeline_track { id = "sample-timeline-track", width = 1240, height = 180, track_height = TRACK,
    tracks = { "Video", "Titles", "Audio" }, ruler_height = 30,
    items = function() return items:get() end, on_moved = on_moved, zoom = 1.25, view_x = -24,
    fps = 1, selection = { "talk" } }
end
S.span.timeline_track = { 4, 1 }

-- --------------------------------------------------------- drawing board --

-- A sprite, a row a string: each letter a tone, a dot empty.
local SPRITE = {
  "....rrrrrr....",
  "...rrrrrrrr...",
  "..rrwwrrwwrr..",
  ".rrrwwrrwwrrr.",
  ".rrrrrrrrrrrr.",
  "rrwwrrrrrrwwrr",
  "rrwwrrrrrrwwrr",
  ".rrrrrrrrrrrr.",
  "...kkkkkkkk...",
  "...kwkkkkwk...",
  "...kkkkkkkk...",
  "....kkkkkk....",
}
local TONES = { r = "error", w = "paper", k = "warning" }

function S.drawing_board(_, w)
  local items = {}
  local G = 8
  for row, line in ipairs(SPRITE) do
    for col = 1, #line do
      local c = line:sub(col, col)
      if TONES[c] then
        items[#items + 1] = { id = ("px-%d-%d"):format(col, row), x = 24 + (col - 1) * G, y = 16 + (row - 1) * G, w = G, h = G,
          tone = TONES[c], kind = "pixel" }
      end
    end
  end
  return w.drawing_board { id = "sample-drawing-board", width = 600, height = 480, items = items,
    zoom = 2.5, view_x = -40, view_y = -32, board = { 0, 0, 160, 128 } }
end
S.span.drawing_board = { 2, 2 }

return S
