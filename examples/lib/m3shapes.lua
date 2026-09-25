-- Material 3's expressive shapes, as `ui.Path` outlines that morph into
-- one another.
--
-- A shape is a polygon whose corners are rounded, each by a radius of its
-- own: a star with soft points is a cookie, one with deep round lobes a
-- clover, eight vertices rounded as far as they go a circle. The outline is
-- a run of cubic curves, and every shape is cut into the same number of
-- them, starting at the top, so `ui.Path`'s `morph_to` walks any shape onto
-- any other point for point.
--
--   local shapes = require("lib.m3shapes")
--   ui.Path { width = 48, height = 48, view_box = { 0, 0, 100, 100 },
--             d = shapes.path("cookie9"), fill_color = accent }
--
--   -- A shape that morphs whenever `which` changes, and may spin.
--   shapes.Shape { width = 96, height = 96, shape = function() return which:get() end,
--                  color = accent, duration = 350, easing = "out_back" }
--
-- The names follow Material 3's shape library; the outlines are this
-- library's own construction (corner rounding as in androidx's
-- graphics-shapes, Apache-2.0, without its smoothing).

local ui = require("morf.ui")

local shapes = {}

-- How many cubics every outline is cut into: a star's points and valleys
-- are a corner and an edge each, so the fifteen-pointed boom needs sixty.
shapes.SEGMENTS = 72

local pi = math.pi

-- ---------------------------------------------------------------- geometry

local function norm(x, y)
  local l = math.sqrt(x * x + y * y)
  if l < 1e-9 then return 0, 0, 0 end
  return x / l, y / l, l
end

-- A cubic as eight numbers: start, two controls, end.
local function line(ax, ay, bx, by)
  return { ax, ay, ax + (bx - ax) / 3, ay + (by - ay) / 3, ax + 2 * (bx - ax) / 3, ay + 2 * (by - ay) / 3, bx, by }
end

--- An outline from vertices `{ { x, y, rounding }, ... }` (or `{ x, y }`
--- with `opts.rounding` for all), in any units, clockwise on screen. A
--- corner's rounding is the radius of the arc that replaces it, shrunk
--- where the edges beside it are too short for it.
function shapes.polygon(vertices, opts)
  opts = opts or {}
  local n = #vertices
  local corners = {}
  for i = 1, n do
    local p = vertices[(i - 2) % n + 1]
    local v = vertices[i]
    local q = vertices[i % n + 1]
    local e1x, e1y, l1 = norm(p[1] - v[1], p[2] - v[2])
    local e2x, e2y, l2 = norm(q[1] - v[1], q[2] - v[2])
    local cos = math.max(-1, math.min(1, e1x * e2x + e1y * e2y))
    local theta = math.acos(cos) -- the corner's inside angle
    local r = v[3] or opts.rounding or 0
    local cut, radius = 0, 0
    if r > 0 and theta > 1e-4 and theta < pi - 1e-4 then
      local t = math.tan(theta / 2)
      cut = math.min(r / t, l1 / 2, l2 / 2)
      radius = cut * t
    end
    local ax, ay = v[1] + e1x * cut, v[2] + e1y * cut
    local bx, by = v[1] + e2x * cut, v[2] + e2y * cut
    -- An arc of (pi - theta) as one cubic.
    local h = 4 / 3 * math.tan((pi - theta) / 4) * radius
    corners[i] = { ax, ay, ax - e1x * h, ay - e1y * h, bx - e2x * h, by - e2y * h, bx, by }
  end
  local curves = {}
  for i = 1, n do
    local c = corners[i]
    local nxt = corners[i % n + 1]
    curves[#curves + 1] = c
    curves[#curves + 1] = line(c[7], c[8], nxt[1], nxt[2])
  end
  return curves
end

--- A star of `points` points at radius 1 and `inner` between them.
--- Options: `rounding` (the points'), `inner_rounding` (the valleys',
--- the points' by default), `rotation` (degrees; the first point is up).
function shapes.star(points, inner, opts)
  opts = opts or {}
  local rounding = opts.rounding or 0
  local inner_rounding = opts.inner_rounding or rounding
  local rot = math.rad(opts.rotation or 0) - pi / 2
  local vertices = {}
  for i = 0, points - 1 do
    local a = rot + 2 * pi * i / points
    vertices[#vertices + 1] = { math.cos(a), math.sin(a), rounding }
    local b = a + pi / points
    vertices[#vertices + 1] = { inner * math.cos(b), inner * math.sin(b), inner_rounding }
  end
  return shapes.polygon(vertices)
end

--- A regular polygon of `sides` at radius 1, the first vertex up.
function shapes.regular(sides, opts)
  opts = opts or {}
  local rot = math.rad(opts.rotation or 0) - pi / 2
  local vertices = {}
  for i = 0, sides - 1 do
    local a = rot + 2 * pi * i / sides
    vertices[#vertices + 1] = { math.cos(a), math.sin(a) }
  end
  return shapes.polygon(vertices, { rounding = opts.rounding or 0 })
end

--- Round lobes about a centre, as a clover's: each lobe two vertices
--- `spread` degrees either side of its axis, rounded as far as they go, and
--- a valley at `inner` between lobes.
function shapes.lobes(count, inner, opts)
  opts = opts or {}
  local spread = math.rad(opts.spread or 90 / count)
  local rot = math.rad(opts.rotation or 0) - pi / 2
  local vertices = {}
  for i = 0, count - 1 do
    local a = rot + 2 * pi * i / count
    vertices[#vertices + 1] = { math.cos(a - spread), math.sin(a - spread), 10 }
    vertices[#vertices + 1] = { math.cos(a + spread), math.sin(a + spread), 10 }
    local b = a + pi / count
    vertices[#vertices + 1] = { inner * math.cos(b), inner * math.sin(b), opts.inner_rounding or 0.05 }
  end
  return shapes.polygon(vertices)
end

local function rect(w, h, tl, tr, br, bl)
  return shapes.polygon {
    { -w, -h, tl }, { w, -h, tr }, { w, h, br }, { -w, h, bl },
  }
end

-- ------------------------------------------------------------ the library

local LIBRARY = {
  circle = function() return shapes.regular(8, { rounding = 10 }) end,
  square = function() return rect(1, 1, 0.3, 0.3, 0.3, 0.3) end,
  slanted = function()
    return shapes.polygon({ { -0.8, -1 }, { 1, -1 }, { 0.8, 1 }, { -1, 1 } }, { rounding = 0.3 })
  end,
  arch = function() return rect(1, 1, 1, 1, 0.2, 0.2) end,
  semicircle = function() return rect(1, 0.5, 1, 1, 0.1, 0.1) end,
  oval = function()
    local out = {}
    local c, s = math.cos(-pi / 4), math.sin(-pi / 4)
    for i, v in ipairs { { 0, -1 }, { 0.7, 0 }, { 0, 1 }, { -0.7, 0 } } do
      local x, y = v[1], v[2]
      out[i] = { x * c - y * s, x * s + y * c }
    end
    -- Four points rounded as far as they go: an ellipse, turned.
    return shapes.polygon(out, { rounding = 10 })
  end,
  pill = function() return rect(1, 0.55, 10, 10, 10, 10) end,
  triangle = function() return shapes.regular(3, { rounding = 0.25 }) end,
  arrow = function()
    return shapes.polygon {
      { 0, -1, 0.2 }, { 0.95, 0.8, 0.2 }, { 0, 0.35, 0.3 }, { -0.95, 0.8, 0.2 },
    }
  end,
  fan = function() return rect(1, 1, 1, 0.2, 0.2, 0.2) end,
  diamond = function()
    return shapes.polygon({ { 0, -1 }, { 0.8, 0 }, { 0, 1 }, { -0.8, 0 } }, { rounding = 0.2 })
  end,
  clam_shell = function()
    return shapes.polygon({
      { -0.55, -1 }, { 0.55, -1 }, { 1, 0 }, { 0.55, 1 }, { -0.55, 1 }, { -1, 0 },
    }, { rounding = 0.2 })
  end,
  pentagon = function() return shapes.regular(5, { rounding = 0.2 }) end,
  gem = function()
    return shapes.polygon({
      { -0.5, -0.95 }, { 0.5, -0.95 }, { 1, -0.2 }, { 0, 1 }, { -1, -0.2 },
    }, { rounding = 0.25 })
  end,
  sunny = function() return shapes.star(8, 0.8, { rounding = 0.15 }) end,
  very_sunny = function() return shapes.star(8, 0.65, { rounding = 0.15 }) end,
  cookie4 = function() return shapes.star(4, 0.75, { rounding = 0.6, inner_rounding = 0.3 }) end,
  cookie6 = function() return shapes.star(6, 0.8, { rounding = 0.45, inner_rounding = 0.3 }) end,
  cookie7 = function() return shapes.star(7, 0.82, { rounding = 0.4, inner_rounding = 0.3 }) end,
  cookie9 = function() return shapes.star(9, 0.85, { rounding = 0.3, inner_rounding = 0.2 }) end,
  cookie12 = function() return shapes.star(12, 0.88, { rounding = 0.2, inner_rounding = 0.15 }) end,
  clover4 = function() return shapes.lobes(4, 0.3, { rotation = 45, spread = 28 }) end,
  clover8 = function() return shapes.lobes(8, 0.62, { spread = 12 }) end,
  burst = function() return shapes.star(12, 0.72, { rounding = 0.03 }) end,
  soft_burst = function() return shapes.star(10, 0.72, { rounding = 0.15, inner_rounding = 0.08 }) end,
  boom = function() return shapes.star(15, 0.5, { rounding = 0.03 }) end,
  soft_boom = function() return shapes.star(15, 0.55, { rounding = 0.1, inner_rounding = 0.04 }) end,
  flower = function() return shapes.star(8, 0.6, { rounding = 0.35, inner_rounding = 0.1 }) end,
  puffy = function() return shapes.star(10, 0.82, { rounding = 0.5, inner_rounding = 0.02 }) end,
  heart = function()
    return shapes.polygon {
      { 0, -0.5, 0 }, { 0.5, -1, 0.45 }, { 1, -0.35, 0.45 }, { 0, 0.95, 0.12 },
      { -1, -0.35, 0.45 }, { -0.5, -1, 0.45 },
    }
  end,
}

shapes.NAMES = {}
for name in pairs(LIBRARY) do shapes.NAMES[#shapes.NAMES + 1] = name end
table.sort(shapes.NAMES)

-- ------------------------------------------------------------- resampling

local function point(c, t)
  local u = 1 - t
  local a, b, cc, d = u * u * u, 3 * u * u * t, 3 * u * t * t, t * t * t
  return a * c[1] + b * c[3] + cc * c[5] + d * c[7], a * c[2] + b * c[4] + cc * c[6] + d * c[8]
end

local function length(c)
  local total, px, py = 0, c[1], c[2]
  for i = 1, 8 do
    local x, y = point(c, i / 8)
    total = total + math.sqrt((x - px) ^ 2 + (y - py) ^ 2)
    px, py = x, y
  end
  return total
end

-- The cubic cut at t: the part before and the part after.
local function split(c, t)
  local function lerp(a, b) return a + (b - a) * t end
  local x01, y01 = lerp(c[1], c[3]), lerp(c[2], c[4])
  local x12, y12 = lerp(c[3], c[5]), lerp(c[4], c[6])
  local x23, y23 = lerp(c[5], c[7]), lerp(c[6], c[8])
  local xa, ya = lerp(x01, x12), lerp(y01, y12)
  local xb, yb = lerp(x12, x23), lerp(y12, y23)
  local xm, ym = lerp(xa, xb), lerp(ya, yb)
  return { c[1], c[2], x01, y01, xa, ya, xm, ym }, { xm, ym, xb, yb, x23, y23, c[7], c[8] }
end

-- Exactly `count` cubics: each curve gets a share of the cuts for its
-- length (none for a curve of no length), and is cut into that many equal
-- parts in t.
local function resample(curves, count)
  local lengths, total, live = {}, 0, 0
  for i, c in ipairs(curves) do
    lengths[i] = length(c)
    total = total + lengths[i]
    if lengths[i] > 1e-6 then live = live + 1 end
  end
  if live == 0 or count < live then error("m3shapes: too few segments for this outline", 3) end
  -- Largest remainder, every live curve at least one.
  local shares, given, order = {}, 0, {}
  for i, l in ipairs(lengths) do
    if l > 1e-6 then
      local exact = 1 + (count - live) * l / total
      shares[i] = math.floor(exact)
      given = given + shares[i]
      order[#order + 1] = { i, exact - shares[i] }
    else
      shares[i] = 0
    end
  end
  table.sort(order, function(a, b) return a[2] > b[2] end)
  for k = 1, count - given do
    local i = order[(k - 1) % #order + 1][1]
    shares[i] = shares[i] + 1
  end
  local out = {}
  for i, c in ipairs(curves) do
    local rest = c
    for parts = shares[i], 2, -1 do
      local head
      head, rest = split(rest, 1 / parts)
      out[#out + 1] = head
    end
    if shares[i] >= 1 then out[#out + 1] = rest end
  end
  return out
end

-- Fitted into the unit square round (0.5, 0.5), started at the point
-- nearest straight up, so shapes line up with one another when morphing.
local function normalise(curves)
  local minx, miny, maxx, maxy = math.huge, math.huge, -math.huge, -math.huge
  for _, c in ipairs(curves) do
    for t = 0, 8 do
      local x, y = point(c, t / 8)
      minx, maxx = math.min(minx, x), math.max(maxx, x)
      miny, maxy = math.min(miny, y), math.max(maxy, y)
    end
  end
  local scale = 1 / math.max(maxx - minx, maxy - miny)
  local cx, cy = (minx + maxx) / 2, (miny + maxy) / 2
  local best, first = math.huge, 1
  for i, c in ipairs(curves) do
    for k = 1, 8, 2 do
      c[k] = (c[k] - cx) * scale + 0.5
      c[k + 1] = (c[k + 1] - cy) * scale + 0.5
    end
    local a = math.atan(c[2] - 0.5, c[1] - 0.5) + pi / 2
    local off = math.abs((a + pi) % (2 * pi) - pi)
    if off < best - 1e-6 then best, first = off, i end
  end
  local out = {}
  for i = 0, #curves - 1 do out[#out + 1] = curves[(first - 1 + i) % #curves + 1] end
  return out
end

--- The cubics of a named shape (or of an outline from `polygon`/`star`),
--- normalised and cut into `segments` (`shapes.SEGMENTS`). `segments =
--- false` leaves the outline as it was made -- far cheaper, for a shape
--- that is only drawn, never morphed.
local cache = {}
function shapes.curves(shape, segments)
  if segments == nil then segments = shapes.SEGMENTS end
  local key = type(shape) == "string" and (shape .. "/" .. tostring(segments)) or nil
  if key and cache[key] then return cache[key] end
  local curves = shape
  if type(shape) == "string" then
    local make = LIBRARY[shape]
    if not make then error("m3shapes: no shape named " .. shape, 2) end
    curves = make()
  end
  local out = normalise(segments and resample(curves, segments) or curves)
  if key then cache[key] = out end
  return out
end

--- SVG path data for a shape, in a `size` square (100): for a `ui.Path`
--- with `view_box = { 0, 0, size, size }`.
local paths = {}
-- The named shapes at the defaults, made ahead of time by
-- lib/m3shapes_gen.lua: reading them costs nothing, where resampling one
-- outline costs a good part of a module's instruction budget.
local made
function shapes.path(shape, opts)
  opts = opts or {}
  local size = opts.size or 100
  local segments = opts.segments
  if segments == nil then segments = shapes.SEGMENTS end
  if type(shape) == "string" and size == 100 and segments == shapes.SEGMENTS and not opts.fresh then
    if made == nil then
      local ok, list = pcall(require, "lib.m3shapes_paths")
      made = ok and type(list) == "table" and list or false
    end
    if made and made[shape] then return made[shape] end
  end
  local key = type(shape) == "string" and (shape .. "/" .. size .. "/" .. tostring(segments))
  if key and paths[key] then return paths[key] end
  local curves = shapes.curves(shape, segments)
  local function f(v) return string.format("%.2f", v * size) end
  local parts = { "M" .. f(curves[1][1]) .. " " .. f(curves[1][2]) }
  for _, c in ipairs(curves) do
    parts[#parts + 1] = "C" .. f(c[3]) .. " " .. f(c[4]) .. " " .. f(c[5]) .. " " .. f(c[6])
      .. " " .. f(c[7]) .. " " .. f(c[8])
  end
  parts[#parts + 1] = "Z"
  local d = table.concat(parts, " ")
  if key then paths[key] = d end
  return d
end

-- ------------------------------------------------------------ the element

--- A `ui.Path` showing a shape, morphing when it changes. Props: `shape`
--- (a name, or a function returning one), `color`, `duration` (350),
--- `easing` ("out_cubic"); anything else goes to the `ui.Path` (size,
--- anchors, rotation, ...).
function shapes.Shape(props)
  local source = props.shape
  local current = type(source) == "function" and source() or source
  local path_props = {
    view_box = { 0, 0, 100, 100 },
    d = shapes.path(current),
    morph_to = shapes.path(current),
    morph_progress = 0,
    fill_color = props.color or "#000000",
    behavior = {
      morph_progress = { duration = props.duration or 350, easing = props.easing or "out_cubic" },
    },
  }
  for k, v in pairs(props) do
    if k ~= "shape" and k ~= "color" and k ~= "duration" and k ~= "easing" then
      path_props[k] = v
    end
  end
  local node = ui.Path(path_props)
  if type(source) == "function" then
    -- The two ends take turns: the one on show stays put and the other
    -- becomes the new shape, so a change never jumps back to a start.
    local at_end = false
    morf.effect("m3shapes.morph", function()
      local name = source()
      if name == current then return end
      current = name
      if at_end then
        node.d = shapes.path(name)
        node.morph_progress = 0
      else
        node.morph_to = shapes.path(name)
        node.morph_progress = 1
      end
      at_end = not at_end
    end, { owner = node })
  end
  return node
end

return shapes
