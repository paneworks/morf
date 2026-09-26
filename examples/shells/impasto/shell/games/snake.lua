-- Snake: twenty by twenty; arrows or WASD, and the tail is the enemy. Ten
-- points per apple; each apple shortens the tick a little.
--
-- Port of Snake.qml. The snake thinks in cells and moves in pixels: the
-- tick decides where it goes, and the head slides into the cell it is
-- entering while the tail slides out of the one it is leaving, over the
-- tick's own length. The original repainted a Canvas every frame of that
-- slide; here the body is a chain of round-capped links, one node per pair
-- of neighbouring cells, and only the head link and the tail link move --
-- each by one animation started on the tick, so no Lua runs between ticks.

local ui = require("morf.ui")
local theme = require("theme")
local common = require("games.common")

local C = theme.color

local COLUMNS, ROWS = 20, 20

return function(ctx)
  local size = math.max(1, math.floor(math.min(ctx.width / COLUMNS, ctx.height / ROWS)))
  local id = tostring({})

  local body = {}            -- head first, { x, y } cells
  local apple = { x = 5, y = 5 }
  local dx, dy = 1, 0
  -- Turns queued between ticks, so two quick presses both count and a
  -- reversal is checked against the pending turn, not the current one.
  local queued = {}
  local eaten = morf.signal("impasto.snake." .. id .. ".eaten", 0)
  local apple_at = morf.signal("impasto.snake." .. id .. ".apple", "5,5")

  local function tick() return math.max(55, 130 - eaten:get() * 3) end

  -- ------------------------------------------------------------ drawing --

  local function centre(p) return p.x * size + size / 2, p.y * size + size / 2 end

  -- The body thins and darkens along its length: `at` is 0 at the head and
  -- 1 at the tail; `lift` whitens it, for the light on its back.
  local function paint(at, lift)
    local shade = common.mix(ctx.tint(), C.islandSurface, at * 0.45)
    return common.mix(shade, "#ffffff", lift)
  end
  local function girth(at) return size * (0.78 - 0.34 * at) end

  -- A round-capped stroke between two centres is a stadium, because
  -- neighbouring cells are always in a row or a column.
  local function rect(ax, ay, bx, by, g)
    return math.min(ax, bx) - g / 2, math.min(ay, by) - g / 2,
      math.abs(ax - bx) + g, math.abs(ay - by) + g
  end

  -- Link `k` runs from point k to point k + 1. Nodes come from a Repeater
  -- that is kept a few rows ahead of the snake, so a link exists before it
  -- is needed; `wanted[k]` is what a link should show, read by a node when
  -- it is built and written to it after.
  local links, lights, wanted = {}, {}, {}
  local rows = morf.list_model({})
  local pool = 0
  local function grow(to)
    while pool < to do
      pool = pool + 1
      rows:insert(pool, { id = pool })
    end
  end

  local function apply(node, w, light)
    if not node then return end
    if not w then node.visible = false return end
    node.visible = true
    local g = light and size * 0.2 or w.g
    local off = light and -size * 0.1 or 0
    local x, y, width, height = rect(w.ax + off, w.ay + off, w.bx + off, w.by + off, g)
    node.x, node.y, node.width, node.height, node.radius = x, y, width, height, g / 2
    node.color = light and w.light or w.color
  end

  local function link_delegate(light)
    return function(row)
      local k = row.id
      local node = ui.Rect { x = 0, y = 0, width = 1, height = 1, visible = false, z = -k }
      if light then lights[k] = node else links[k] = node end
      apply(node, wanted[k], light)
      return node
    end
  end

  -- -------------------------------------------------------------- rules --

  local function occupied(x, y)
    for _, s in ipairs(body) do if s.x == x and s.y == y then return true end end
    return false
  end

  local function place_apple()
    local free = {}
    for y = 0, ROWS - 1 do
      for x = 0, COLUMNS - 1 do
        if not occupied(x, y) then free[#free + 1] = { x = x, y = y } end
      end
    end
    if #free == 0 then return end
    apple = free[common.random(#free) + 1]
    apple_at:set(apple.x .. "," .. apple.y)
  end

  local head, disc, face, eyes, burst, play_burst, pop, play_pop

  -- Lays the whole snake out for the tick that starts now: the head link
  -- growing into the cell it is entering, the tail link shrinking out of
  -- the one it left (`ghost`), everything between standing still.
  local function lay(ghost, duration)
    local points = {}
    for i, s in ipairs(body) do points[i] = s end
    if ghost then points[#points + 1] = ghost end
    local count = #points - 1
    grow(count + 4)
    local last = math.max(1, count)
    for k = 1, pool do
      if k <= count then
        local at = (k - 1) / last
        local ax, ay = centre(points[k])
        local bx, by = centre(points[k + 1])
        wanted[k] = {
          ax = ax, ay = ay, bx = bx, by = by, g = girth(at),
          color = paint(at, 0), light = paint(0, 0.34),
        }
      else
        wanted[k] = nil
      end
      apply(links[k], wanted[k], false)
      apply(lights[k], wanted[k], true)
    end

    local hx, hy = centre(body[1])
    head.x, head.y = hx - size * 0.45, hy - size * 0.45
    head.color = paint(0, 0)
    -- Eyes on the sides of the direction of travel, each pupil looking the
    -- way the snake is going.
    for i, side in ipairs { -1, 1 } do
      local ex = size * 0.45 + dx * size * 0.15 - dy * side * size * 0.19
      local ey = size * 0.45 + dy * size * 0.15 + dx * side * size * 0.19
      eyes[i].white.x, eyes[i].white.y = ex - size * 0.13, ey - size * 0.13
      eyes[i].pupil.x = ex + dx * size * 0.05 - size * 0.07
      eyes[i].pupil.y = ey + dy * size * 0.05 - size * 0.07
    end

    if not duration or #body < 2 then
      head.translate_x, head.translate_y = 0, 0
      return
    end
    -- The slide: the head from the cell behind it, the first link from a
    -- point, the last link to one.
    local px, py = centre(body[2])
    local tracks = {}
    for _, n in ipairs { disc, face } do
      tracks[#tracks + 1] = { node = n, property = "translate_x", from = px - hx, to = 0, duration = duration, easing = "linear" }
      tracks[#tracks + 1] = { node = n, property = "translate_y", from = py - hy, to = 0, duration = duration, easing = "linear" }
    end
    local function slide(k, light, from, to)
      local node = light and lights[k] or links[k]
      local w = wanted[k]
      if not node or not w then return end
      local g = light and size * 0.2 or w.g
      local off = light and -size * 0.1 or 0
      local fx, fy, fw, fh = rect(from[1] + off, from[2] + off, from[3] + off, from[4] + off, g)
      local tx, ty, tw, th = rect(to[1] + off, to[2] + off, to[3] + off, to[4] + off, g)
      for _, p in ipairs { { "x", fx, tx }, { "y", fy, ty }, { "width", fw, tw }, { "height", fh, th } } do
        tracks[#tracks + 1] = { node = node, property = p[1], from = p[2], to = p[3], duration = duration, easing = "linear" }
      end
    end
    local w1 = wanted[1]
    if w1 then
      slide(1, false, { w1.bx, w1.by, w1.bx, w1.by }, { w1.ax, w1.ay, w1.bx, w1.by })
      slide(1, true, { w1.bx, w1.by, w1.bx, w1.by }, { w1.ax, w1.ay, w1.bx, w1.by })
    end
    if ghost and count > 1 then
      local wl = wanted[count]
      slide(count, false, { wl.ax, wl.ay, wl.bx, wl.by }, { wl.ax, wl.ay, wl.ax, wl.ay })
      slide(count, true, { wl.ax, wl.ay, wl.bx, wl.by }, { wl.ax, wl.ay, wl.ax, wl.ay })
    end
    morf.animation.play { { parallel = tracks } }
  end

  local function restart()
    body = { { x = 10, y = 10 }, { x = 9, y = 10 }, { x = 8, y = 10 } }
    dx, dy = 1, 0
    queued = {}
    ctx.score:set(0)
    eaten:set(0)
    ctx.over:set(false)
    place_apple()
    lay(nil, nil)
  end

  local function turn(nx, ny)
    local last = queued[#queued] or { x = dx, y = dy }
    if (nx == -last.x and ny == -last.y) or (nx == last.x and ny == last.y) then return end
    queued[#queued + 1] = { x = nx, y = ny }
  end

  local function step()
    if #queued > 0 then
      local next_turn = table.remove(queued, 1)
      dx, dy = next_turn.x, next_turn.y
    end
    local nx, ny = body[1].x + dx, body[1].y + dy
    local ate = nx == apple.x and ny == apple.y
    -- The tail moves out of the way in the same tick unless the snake is
    -- growing.
    local hit = nx < 0 or ny < 0 or nx >= COLUMNS or ny >= ROWS
    local kept = ate and #body or #body - 1
    for i = 1, kept do
      if body[i].x == nx and body[i].y == ny then hit = true end
    end
    if hit then
      ctx.over:set(true)
      ctx.finished(ctx.score:get())
      return
    end
    local ghost = nil
    if not ate then ghost = table.remove(body) end
    table.insert(body, 1, { x = nx, y = ny })
    if ate then
      eaten:set(eaten:get() + 1)
      ctx.score:set(ctx.score:get() + 10)
      local cx, cy = centre(body[1])
      burst.x, burst.y = cx - size * 1.1, cy - size * 1.1
      pop.x, pop.y = cx - size, body[1].y * size - size * 0.4
      play_burst()
      play_pop("+10")
      place_apple()
    end
    lay(ghost, tick())
  end

  -- -------------------------------------------------------------- nodes --

  local board_w, board_h = size * COLUMNS, size * ROWS

  -- The cells, drawn once: a board the snake can be read against, and
  -- nothing that repaints with it. One picture rather than four hundred
  -- dots.
  local dot = math.max(1, math.floor(size * 0.07 + 0.5))
  local circles = {}
  for y = 0, ROWS - 1 do
    for x = 0, COLUMNS - 1 do
      circles[#circles + 1] = string.format('<circle cx="%g" cy="%g" r="%g"/>',
        x * size + size / 2, y * size + size / 2, dot)
    end
  end
  local grid = ui.Image {
    x = 0, y = 0, width = board_w, height = board_h,
    source = string.format(
      '<svg xmlns="http://www.w3.org/2000/svg" width="%d" height="%d"><g fill="%s">%s</g></svg>',
      board_w, board_h, common.over("#ffffff", 0.05):hex(), table.concat(circles)),
  }

  -- The apple, beating gently: a glow, the fruit, its shine and its stalk.
  local radius = size * 0.34
  local function apple_x() local x = apple_at:get():match("^(%d+)") return tonumber(x) * size + size / 2 end
  local function apple_y() local y = apple_at:get():match(",(%d+)$") return tonumber(y) * size + size / 2 end
  local stalk_len = radius * math.sqrt(0.34 * 0.34 + 0.6 * 0.6)
  local stalk_w = math.max(1, size * 0.07)
  local fruit = ui.Item {
    x = function() return apple_x() - radius * 1.7 end,
    y = function() return apple_y() - radius * 1.7 end,
    width = radius * 3.4, height = radius * 3.4,
    scale = 0.95,
    behavior = { scale = { duration = 817, easing = "in_out_sine", loops = "forever", ping_pong = true } },
    ui.Rect {
      x = 0, y = 0, width = radius * 3.4, height = radius * 3.4, radius = radius * 1.7,
      color = function() return common.over(C.red(), 0.18) end,
    },
    ui.Rect {
      x = radius * 0.7, y = radius * 0.7, width = radius * 2, height = radius * 2, radius = radius,
      color = C.red,
    },
    ui.Rect {
      x = radius * 1.7 - radius * 0.34 - radius * 0.26, y = radius * 1.7 - radius * 0.36 - radius * 0.26,
      width = radius * 0.52, height = radius * 0.52, radius = radius * 0.26,
      color = morf.color.rgb(1, 1, 1, 0.55),
    },
    ui.Rect {
      x = radius * 1.7 + radius * 0.17 - stalk_w / 2,
      y = radius * 1.7 - radius * 1.2 - stalk_len / 2,
      width = stalk_w, height = stalk_len, radius = stalk_w / 2,
      rotation = math.deg(math.atan(0.34, 0.6)),
      color = C.green,
    },
  }

  eyes = {}
  local eye_nodes = {}
  for i = 1, 2 do
    local white = ui.Rect { width = size * 0.26, height = size * 0.26, radius = size * 0.13, color = C.indicator }
    local pupil = ui.Rect { width = size * 0.14, height = size * 0.14, radius = size * 0.07, color = C.island }
    eyes[i] = { white = white, pupil = pupil }
    eye_nodes[#eye_nodes + 1] = white
    eye_nodes[#eye_nodes + 1] = pupil
  end
  -- The eyes sit over the light on the back, so the head is two nodes
  -- moved together: its disc under the light, its eyes over it.
  disc = ui.Rect { width = size * 0.9, height = size * 0.9, radius = size * 0.45 }
  face = ui.Item { width = size * 0.9, height = size * 0.9, table.unpack(eye_nodes) }
  head = {}
  setmetatable(head, {
    __newindex = function(_, k, v) disc[k] = v if k ~= "color" then face[k] = v end end,
  })

  burst, play_burst = common.burst { tint = C.red, spread = size * 1.1, sparks = 6 }
  pop, play_pop = common.pop { tint = C.red, rise = size, width = size * 2 }

  local node = ui.Item {
    width = ctx.width, height = ctx.height,
    ui.Timer {
      interval = function() return tick() end,
      ["repeat"] = true,
      running = function() return not ctx.over:get() end,
      on_triggered = function() if ctx.live() then step() end end,
    },
    common.ground {
      x = (ctx.width - board_w) / 2, y = (ctx.height - board_h) / 2,
      width = board_w, height = board_h,
      grid,
      fruit,
      -- Drawn tail first so the head sits on top.
      ui.Repeater { model = rows, delegate = link_delegate(false) },
      disc,
      -- The light on a round back: one line over the whole snake, offset
      -- towards the top left.
      ui.Repeater { model = rows, delegate = link_delegate(true) },
      face,
      burst,
      pop,
    },
  }

  local function key(keysym)
    local tx, ty = common.direction(keysym, true)
    if not tx then return false end
    turn(tx, ty)
    return true
  end

  restart()
  -- Written after construction, so the beat has somewhere to go.
  fruit.scale = 1.05
  return {
    node = node, key = key, restart = restart,
    debug = function() return "len " .. #body .. " head " .. body[1].x .. "," .. body[1].y .. " pool " .. pool end,
  }
end
