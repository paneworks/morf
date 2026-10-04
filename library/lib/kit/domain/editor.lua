-- Domain instruments: editor -- the editing surfaces of a synth and a
-- sequencer, over the kit's archetypes and in a theme's style (see
-- lib.kit.display and lib.kit.domain.audio, whose helpers these share).
--
-- An envelope's points are Planes, as an EQ's are; a node editor's cards
-- and a piano roll's notes are Drags (a headless Drag fed by the card or
-- note's own pointer, its delta moving or resizing it on the grid); a step
-- sequencer is a Selection grid in "multi" mode -- a click or Space
-- toggles a step, the arrows walk the grid. Links and curves are laid out
-- in Lua when something moves, never per frame.
--
-- The look is the style's: Material tonal cards that stretch as they are
-- dragged, rounded notes and steps, springs; Tsugumori hairline cards with
-- hatched headers and registration marks, square notes, mono captions.
local morf = require("morf")
local ui = require("morf.ui")
local U = require("lib.kit.display.util")
local control = require("lib.kit.control")
local H = require("lib.kit.domain.audio")._H
local get, clamp01 = U.get, U.clamp01

local M = {}

local function ms(t) if t < 1 then return ("%dms"):format(math.floor(t * 1000 + .5)) end return ("%.2fs"):format(t) end

-- ------------------------------------------------------- envelope_editor --

--- An ADSR envelope: four points to drag -- the attack's peak (across),
--- the decay's end (across, and up for the sustain level), the sustain's
--- end (up) and the release's end (across); each takes the arrows.
--- `spec`: `attack`, `decay`, `release` (seconds: .1, .3, .8), `sustain`
--- (0..1, .6), `max_attack` (2), `max_decay` (2), `max_release` (4),
--- `width` (260), `height` (190), `label`, `color`, `on_changed(a, d, s,
--- r)`, `id` (points `<id>-attack`, `-decay`, `-sustain`, `-release`).
function M.envelope_editor(spec, style)
  local W, Hh = spec.width or 260, spec.height or 190
  local color = H.color(spec, style)
  local root = H.frame(spec, style, "Envelope editor", W, Hh)
  local pad = H.pad(spec, style)
  local st = morf.state { a = get(spec.attack) or .1, d = get(spec.decay) or .3, s = get(spec.sustain) or .6,
    r = get(spec.release) or .8 }
  local AMAX, DMAX, RMAX = spec.max_attack or 2, spec.max_decay or 2, spec.max_release or 4
  local head = 20
  local pw, ph = W - 2 * pad, Hh - 2 * pad - head - 4
  local q = pw / 4
  local HOLD = q * .6
  -- Where each stage ends, in px across the plot.
  local function ax() return clamp01(st.a / AMAX) * q end
  local function dx() return ax() + clamp01(st.d / DMAX) * q end
  local function sx() return dx() + HOLD end
  local function rx() return sx() + clamp01(st.r / RMAX) * (pw - 2 * q - HOLD) end
  local function changed() if spec.on_changed then spec.on_changed(st.a, st.d, st.s, st.r) end end
  -- The readings across the top.
  local cols = { { "A", function() return ms(st.a) end }, { "D", function() return ms(st.d) end },
    { "S", function() return ("%d%%"):format(math.floor(st.s * 100 + .5)) end }, { "R", function() return ms(st.r) end } }
  local cw = pw / 4
  for i, c in ipairs(cols) do
    H.add(root, H.txt(style, { x = pad + (i - 1) * cw, y = pad - 1, width = cw - 4, height = head, elide = "right",
      font_size = H.small(style) + 1, font_family = style.hatched and style.mono_font or nil, color = style.ink,
      text = function() return c[1] .. " " .. c[2]() end }))
  end
  local plot = ui.Item { x = pad, y = pad + head + 4, width = pw, height = ph }
  H.add(root, plot)
  H.add(plot, H.well(style, pw, ph))
  H.add(plot, H.guides(style, pw, ph, {}, { .25, .5, .75 }))
  local top, bottom = 3, ph - 2
  local function py(v) return bottom - v * (bottom - top) end
  local function curve_d()
    local a, d, s, r = ax(), dx(), sx(), rx()
    local ys = py(st.s)
    if style.hatched then
      return ("M0 %.1f L%.1f %.1f L%.1f %.1f L%.1f %.1f L%.1f %.1f"):format(bottom, a, top, d, ys, s, ys, r, bottom)
    end
    -- Material: a bowed attack and falling exponentials.
    return ("M0 %.1f Q%.1f %.1f %.1f %.1f Q%.1f %.1f %.1f %.1f L%.1f %.1f Q%.1f %.1f %.1f %.1f"):format(bottom,
      a * .35, top + (bottom - top) * .1, a, top, a + (d - a) * .15, ys, d, ys, s, ys, s + (r - s) * .15, bottom, r, bottom)
  end
  local inside = ui.Item { width = pw, height = ph, clip = true }
  H.add(plot, inside)
  -- The stages' edges.
  for _, f in ipairs { ax, dx, sx } do
    H.add(inside, ui.Rect { x = function() return math.floor(f()) end, y = 0, width = 1, height = ph,
      color = H.alpha(style.ink_lo, style.hatched and .35 or .25) })
  end
  H.add(inside, H.shape(style, pw, ph, color, function() return curve_d() .. " Z" end, { alpha = .2 }))
  H.add(inside, H.path(pw, ph, { d = curve_d, stroke_color = color, stroke_width = style.hatched and 1.5 or 2.5,
    stroke_cap = "round", stroke_join = style.hatched and "miter" or "round" }))
  local function id(part) return spec.id and (spec.id .. "-" .. part) or nil end
  local function nx(px) return px / pw end
  local function ny(y) return 1 - y / ph end
  local points = {
    { "attack", "Attack", function() return nx(ax()) end, function() return ny(top) end, function() return st.a end, AMAX,
      function(x) st.a = math.max(.001, clamp01(x * pw / q) * AMAX) end, .0025 },
    { "decay", "Decay", function() return nx(dx()) end, function() return ny(py(st.s)) end, function() return st.d end, DMAX,
      function(x, y) st.d = math.max(.001, clamp01((x * pw - ax()) / q) * DMAX) st.s = clamp01((y * ph - (ph - bottom)) / (bottom - top)) end, .0025 },
    { "sustain", "Sustain", function() return nx(sx()) end, function() return ny(py(st.s)) end, function() return st.s end, 1,
      function(_, y) st.s = clamp01((y * ph - (ph - bottom)) / (bottom - top)) end, nil },
    { "release", "Release", function() return nx(rx()) end, function() return ny(bottom) end, function() return st.r end, RMAX,
      function(x) st.r = math.max(.001, clamp01((x * pw - sx()) / (pw - 2 * q - HOLD)) * RMAX) end, .0025 },
  }
  for i, p in ipairs(points) do
    H.add(plot, (H.point(style, { plot = plot, w = pw, h = ph, id = id(p[1]), name = p[2], color = i == 3 and style.series(2) or color,
      x = p[3], y = p[4], value = p[5], min = 0, max = p[6], step_x = p[8], step_y = .01,
      on_moved = function(x, y) p[7](x, y) changed() end })))
  end
  return root
end

-- ----------------------------------------------------------- node_editor --

--- A node editor: cards with input ports on the left and outputs on the
--- right, joined by bezier links. Each card is a Drag: pressed anywhere it
--- moves with the pointer (kept on the canvas), focused the arrows nudge
--- it. `spec`: `nodes` (`{ {id=, title=, x=, y=, inputs={...},
--- outputs={...}} }`), `links` (`{ {from_node, from_port, to_node,
--- to_port} }`, ports by name or index), `width` (260), `height` (190),
--- `node_width` (88), `label`, `color`, `on_moved(node_id, x, y)`, `id`
--- (cards `<id>-node-<node id>`).
function M.node_editor(spec, style)
  local W, Hh = spec.width or 260, spec.height or 190
  local color = H.color(spec, style)
  local root = H.frame(spec, style, "Node editor", W, Hh)
  local pad = style.hatched and 4 or 6
  local cvw, cvh = W - 2 * pad, Hh - 2 * pad
  local canvas = ui.Item { x = pad, y = pad, width = cvw, height = cvh, clip = true }
  H.add(root, canvas)
  if style.hatched then
    local xs, ys = {}, {}
    for k = 1, 7 do xs[#xs + 1] = k / 8 end
    for k = 1, 5 do ys[#ys + 1] = k / 6 end
    H.add(canvas, H.guides(style, cvw, cvh, xs, ys))
  else
    -- Material: a field of faint dots.
    local d = {}
    for x = 12, cvw - 4, 16 do for y = 12, cvh - 4, 16 do d[#d + 1] = ("M%d %d h.1"):format(x, y) end end
    H.add(canvas, H.path(cvw, cvh, { d = table.concat(d, " "), stroke_color = H.alpha(style.ink_lo, .35), stroke_width = 2,
      stroke_cap = "round" }))
  end
  local NW = spec.node_width or 88
  local HEAD, ROW = 22, 18
  local nodes, by_id = {}, {}
  for i, n in ipairs(get(spec.nodes) or {}) do
    local rows = math.max(#(n.inputs or {}), #(n.outputs or {}), 1)
    local node = { id = n.id or tostring(i), title = n.title or n.id or ("Node " .. i), inputs = n.inputs or {},
      outputs = n.outputs or {}, h = HEAD + rows * ROW + 4,
      pos = morf.state { x = n.x or (10 + (i - 1) * (NW + 20)), y = n.y or 10 } }
    nodes[i] = node
    by_id[node.id] = node
  end
  local front = morf.signal(H.uid("nodes.front"), 0)
  local function port_index(list, port)
    if type(port) == "number" then return port end
    for k, name in ipairs(list) do if name == port then return k end end
    return 1
  end
  local function port_xy(node, side, k)
    local x = node.pos.x + (side == "out" and NW or 0)
    return x, node.pos.y + HEAD + (k - .5) * ROW
  end
  -- The links, under the cards.
  for _, l in ipairs(get(spec.links) or {}) do
    local a = by_id[l.from and l.from[1] or l[1]]
    local b = by_id[l.to and l.to[1] or l[3]]
    if a and b then
      local ka = port_index(a.outputs, l.from and l.from[2] or l[2])
      local kb = port_index(b.inputs, l.to and l.to[2] or l[4])
      local function d()
        local x1, y1 = port_xy(a, "out", ka)
        local x2, y2 = port_xy(b, "in", kb)
        local bend = math.max(28, math.abs(x2 - x1) / 2)
        return ("M%.1f %.1f C%.1f %.1f %.1f %.1f %.1f %.1f"):format(x1, y1, x1 + bend, y1, x2 - bend, y2, x2, y2)
      end
      H.add(canvas, H.path(cvw, cvh, { d = d, stroke_color = style.hatched and color or H.alpha(color, .85),
        stroke_width = style.hatched and 1 or 2.5, stroke_cap = "round" }))
    end
  end
  for i, node in ipairs(nodes) do
    local tone = style.series(i)
    local z = morf.signal(H.uid("node.z"), i)
    local start
    local area, drag
    area = ui.MouseArea { id = spec.id and (spec.id .. "-node-" .. node.id) or nil, width = NW, height = node.h, cursor = "pointer",
      x = function() return node.pos.x end, y = function() return node.pos.y end, z = function() return 10 + z:get() end,
      focus_policy = "strong", accessible_role = "grip", accessible_name = node.title,
      stretch = (not style.hatched) and { stiffness = 260, damping = 16, scale = .1 } or nil,
      on_pressed = function(sx, sy)
        front:set(front:get() + 1)
        z:set(#nodes + front:get())
        drag.send("pressed", sx, sy)
      end,
      on_dragged = function(sx, sy) drag.send("dragged", sx, sy) end,
      on_released = function() drag.send("released", 0, 0) end,
      on_focus_changed = function(on) drag.send("focus", on, area.visual_focus or false) end,
      on_key_pressed = function(_, _, modifiers, _, name)
        local step = (modifiers or ""):find("shift") and 1 or 8
        local mx = name == "Left" and -step or name == "Right" and step or 0
        local my = name == "Up" and -step or name == "Down" and step or 0
        if mx == 0 and my == 0 then return false end
        node.pos.x = math.max(0, math.min(cvw - NW, node.pos.x + mx))
        node.pos.y = math.max(0, math.min(cvh - node.h, node.pos.y + my))
        if spec.on_moved then spec.on_moved(node.id, node.pos.x, node.pos.y) end
        return true
      end,
      on_destroyed = function() drag.drop() end }
    drag = control.headless("Drag", { mode = "move", axis = "both", threshold = 3, owner = area,
      on_drag_started = function() start = { node.pos.x, node.pos.y } end,
      on_dragged = function(_, mx, my)
        if not start then return end
        node.pos.x = math.max(0, math.min(cvw - NW, start[1] + mx))
        node.pos.y = math.max(0, math.min(cvh - node.h, start[2] + my))
        if spec.on_moved then spec.on_moved(node.id, node.pos.x, node.pos.y) end
      end,
      on_dropped = function() start = nil end })
    local t = drag.t
    if style.hatched then
      H.add(area,
        ui.Rect { width = NW, height = node.h, color = style.surface, border_width = 1,
          border_color = function() return (t.active or t.visual_focus) and get(color) or get(style.line) end },
        ui.Item { x = 1, y = 1, width = NW - 2, height = HEAD - 1, clip = true,
          ui.Rect { width = NW - 2, height = HEAD - 1, color = H.alpha(tone, .14) },
          style.stripes.box { width = NW - 2, height = HEAD - 1, gap = 5, weight = 1, color = H.alpha(tone, .45) } },
        ui.Rect { y = HEAD, width = NW, height = 1, color = tone },
        H.cap(style, { text = node.title, x = 6, y = 3, width = NW - 12, color = style.ink, font_size = H.small(style) + 1 }))
      local m = style.marks()
      if m then m.visible = function() return t.active or t.visual_focus end H.add(area, m) end
    else
      H.add(area,
        ui.Rect { width = NW, height = node.h, radius = 10, color = style.track,
          border_width = function() return t.visual_focus and 2 or 0 end, border_color = color },
        ui.Rect { width = NW, height = HEAD, top_left_radius = 10, top_right_radius = 10, color = H.alpha(tone, .32) },
        H.txt(style, { text = node.title, x = 8, y = 0, width = NW - 16, height = HEAD, vertical_alignment = "center",
          elide = "right", font_size = H.small(style) + 1, font_weight = 600, color = style.ink }))
    end
    local P = style.hatched and 6 or 8
    for k, name in ipairs(node.inputs) do
      local y = HEAD + (k - .5) * ROW
      H.add(area, ui.Rect { x = 0, y = y - P / 2, width = P, height = P, radius = style.hatched and 0 or P / 2,
        color = style.hatched and style.surface or tone, border_width = style.hatched and 1 or 0, border_color = tone },
        H.txt(style, { text = style.hatched and tostring(name):upper() or name, x = P + 3, y = y - 8, width = NW / 2 + 6, height = 16,
          elide = "right", font_size = H.small(style), color = style.ink_lo, font_family = style.hatched and style.mono_font or nil }))
    end
    for k, name in ipairs(node.outputs) do
      local y = HEAD + (k - .5) * ROW
      H.add(area, ui.Rect { x = NW - P, y = y - P / 2, width = P, height = P, radius = style.hatched and 0 or P / 2,
        color = tone },
        H.txt(style, { text = style.hatched and tostring(name):upper() or name, x = NW / 2 - 9, y = y - 8, width = NW / 2 + 6 - P,
          height = 16, horizontal_alignment = "right", elide = "right", font_size = H.small(style), color = style.ink_lo,
          font_family = style.hatched and style.mono_font or nil }))
    end
    H.add(canvas, area)
  end
  return root
end

-- ------------------------------------------------------------ piano_roll --

--- A piano roll: notes on a grid of pitches (rows, a keyboard down the
--- left) by steps (columns). A note is a Drag: its body moves it by whole
--- steps and semitones, its right edge resizes it; focused, the arrows
--- move it and Shift with Left or Right resizes it. A double click on the
--- grid adds a note, on a note removes it. `spec`: `notes` (`{ {pitch,
--- start, length} }`, steps from 0), `low`, `high` (MIDI pitches shown,
--- 60..71), `steps` (16), `beat` (steps to a beat, 4), `length` (a new
--- note's, 2), `position` (0..1 or a function: the playhead), `width`
--- (260), `height` (190), `label`, `color`, `on_changed(notes)`, `id`
--- (notes `<id>-note-<i>`, their edges `<id>-note-<i>-edge`).
function M.piano_roll(spec, style)
  local W, Hh = spec.width or 260, spec.height or 190
  local color = H.color(spec, style)
  local root = H.frame(spec, style, "Piano roll", W, Hh)
  local pad = H.pad(spec, style)
  local low, high = spec.low or 60, spec.high or 71
  local steps, beat = spec.steps or 16, spec.beat or 4
  local KW = 30
  local rows = high - low + 1
  local gw = W - 2 * pad - KW
  local cw = gw / steps
  local rh = (Hh - 2 * pad) / rows
  local gh = rh * rows
  -- The keyboard.
  local keys = ui.Item { x = pad, y = pad, width = KW, height = gh }
  H.add(root, keys)
  local dark = H.dark(style)
  for p = low, high do
    local y = (high - p) * rh
    local black = H.BLACK[p % 12]
    if style.hatched then
      H.add(keys, ui.Rect { y = y, width = black and KW * .62 or KW, height = rh, color = black and style.ink or style.surface,
        border_width = black and 0 or 1, border_color = style.line })
    else
      H.add(keys, ui.Rect { y = y + .5, width = black and KW * .62 or KW - 2, height = rh - 1,
        top_right_radius = 3, bottom_right_radius = 3,
        color = black and (dark and style.surface or style.ink) or (dark and style.ink_lo or style.raised) })
    end
    if p % 12 == 0 and rh >= 11 then
      H.add(keys, H.txt(style, { text = H.note_name(p), x = 2, y = y + rh / 2 - 8, width = KW - 4, height = 16,
        horizontal_alignment = "right", vertical_alignment = "center", font_size = H.small(style),
        font_family = style.hatched and style.mono_font or nil, color = style.hatched and style.ink_lo or (dark and style.surface or style.ink) }))
    end
  end
  local grid = ui.Item { x = pad + KW, y = pad, width = gw, height = gh, clip = true }
  H.add(root, grid)
  -- The grid's ground: the sharps' rows shaded, the steps and beats ruled.
  local shade = {}
  for p = low, high do
    if H.BLACK[p % 12] then local y = (high - p) * rh shade[#shade + 1] = ("M0 %.1f H%.1f V%.1f H0 Z"):format(y, gw, y + rh) end
  end
  H.add(grid, H.well(style, gw, gh))
  if #shade > 0 then H.add(grid, H.path(gw, gh, { d = table.concat(shade, " "), fill_color = H.alpha(style.ink_lo, .08) })) end
  local xs, bxs = {}, {}
  for k = 1, steps - 1 do if k % beat == 0 then bxs[#bxs + 1] = k / steps else xs[#xs + 1] = k / steps end end
  local ys = {}
  for k = 1, rows - 1 do ys[#ys + 1] = k / rows end
  H.add(grid, H.guides(style, gw, gh, xs, style.hatched and ys or {}))
  H.add(grid, H.guides(style, gw, gh, bxs, {}, true))
  -- The notes.
  local list = {}
  for _, n in ipairs(get(spec.notes) or { { 60, 0, 4 }, { 64, 4, 2 }, { 67, 6, 2 }, { 71, 8, 4 }, { 69, 12, 2 }, { 67, 14, 2 } }) do
    list[#list + 1] = morf.state { pitch = n.pitch or n[1] or 60, start = n.start or n[2] or 0, length = n.length or n[3] or 1 }
  end
  local function changed()
    if spec.on_changed then
      local out = {}
      for i, n in ipairs(list) do out[i] = { pitch = n.pitch, start = n.start, length = n.length } end
      spec.on_changed(out)
    end
  end
  local layer = ui.Item { width = gw, height = gh, z = 2 }
  local nodes = {}
  local rebuild
  -- A double click on the grid adds a note there.
  H.add(grid, ui.MouseArea { width = gw, height = gh, z = 1,
    on_double_clicked = function(_, _, lx, ly)
      local start = math.max(0, math.min(steps - 1, math.floor(lx / cw)))
      local pitch = math.max(low, math.min(high, high - math.floor(ly / rh)))
      list[#list + 1] = morf.state { pitch = pitch, start = start, length = math.min(spec.length or 2, steps - start) }
      rebuild()
      changed()
    end })
  H.add(grid, layer)
  local function note_node(i, n)
    local origin
    local body, edge, move, size
    local function clampn()
      n.length = math.max(1, math.min(steps - n.start, n.length))
    end
    local nid = spec.id and (spec.id .. "-note-" .. i) or nil
    body = ui.MouseArea { id = nid, cursor = "pointer", focus_policy = "strong",
      x = function() return n.start * cw end, y = function() return (high - n.pitch) * rh end,
      width = function() return n.length * cw end, height = rh,
      behavior = (not style.hatched) and { x = style.spring(700, 32), y = style.spring(700, 32), width = style.spring(700, 32) } or nil,
      accessible_role = "grip", accessible_name = function() return ("%s, step %d, %d long"):format(H.note_name(n.pitch), n.start + 1, n.length) end,
      on_pressed = function(sx, sy) move.send("pressed", sx, sy) end,
      on_dragged = function(sx, sy) move.send("dragged", sx, sy) end,
      on_released = function() move.send("released", 0, 0) end,
      on_focus_changed = function(on) move.send("focus", on, body.visual_focus or false) end,
      on_double_clicked = function()
        morf.timer(0, function() table.remove(list, i) rebuild() changed() end, false)
      end,
      on_key_pressed = function(_, _, modifiers, _, name)
        local shift = (modifiers or ""):find("shift") ~= nil
        if shift and (name == "Left" or name == "Right") then
          n.length = n.length + (name == "Right" and 1 or -1)
          clampn()
        elseif name == "Left" or name == "Right" then
          n.start = math.max(0, math.min(steps - n.length, n.start + (name == "Right" and 1 or -1)))
        elseif name == "Up" or name == "Down" then
          n.pitch = math.max(low, math.min(high, n.pitch + (name == "Up" and 1 or -1)))
        else
          return false
        end
        changed()
        return true
      end,
      on_destroyed = function() move.drop() size.drop() end }
    move = control.headless("Drag", { mode = "move", axis = "both", threshold = 3, owner = body,
      on_drag_started = function() origin = { n.start, n.pitch } end,
      on_dragged = function(_, mx, my)
        if not origin then return end
        local start = math.max(0, math.min(steps - n.length, origin[1] + math.floor(mx / cw + .5)))
        local pitch = math.max(low, math.min(high, origin[2] - math.floor(my / rh + .5)))
        if start ~= n.start or pitch ~= n.pitch then n.start, n.pitch = start, pitch changed() end
      end,
      on_dropped = function() origin = nil end })
    local EW = math.max(4, math.min(8, cw / 2))
    edge = ui.MouseArea { id = nid and (nid .. "-edge"), cursor = "col_resize", z = 3,
      anchors = { right = true, top = true, bottom = true }, width = EW,
      on_pressed = function(sx, sy) size.send("pressed", sx, sy) end,
      on_dragged = function(sx, sy) size.send("dragged", sx, sy) end,
      on_released = function() size.send("released", 0, 0) end }
    local grown
    size = control.headless("Drag", { mode = "resize", axis = "x", threshold = 2, owner = edge,
      on_drag_started = function() grown = n.length end,
      on_dragged = function(_, mx)
        if not grown then return end
        local length = math.max(1, math.min(steps - n.start, grown + math.floor(mx / cw + .5)))
        if length ~= n.length then n.length = length changed() end
      end,
      on_dropped = function() grown = nil end })
    local t = move.t
    if style.hatched then
      H.add(body, ui.Item { anchors = { fill = true, margins = .5 }, clip = true,
        ui.Rect { anchors = { fill = true }, color = H.alpha(color, .3), border_width = 1,
          border_color = function() return (t.active or t.visual_focus) and get(style.ink) or get(color) end },
        style.stripes.box { width = gw, height = rh, gap = 4, weight = 1, color = H.alpha(color, .75) } })
      H.add(body, ui.Rect { anchors = { right = true, top = true, bottom = true }, width = 2, color = color })
    else
      H.add(body, ui.Rect { anchors = { fill = true, margins = 1 }, radius = math.min(4, rh / 3),
        color = function() return t.active and get(color):mix(get(style.ink), .2) or get(color) end,
        border_width = function() return t.visual_focus and 2 or 0 end, border_color = style.ink })
      H.add(body, ui.Rect { anchors = { right = true, vertical_center = true, right_margin = 3 }, width = 2, height = rh * .5,
        radius = 1, color = H.alpha(style.on_accent, .7) })
    end
    H.add(body, edge)
    return body
  end
  function rebuild()
    for _, node in ipairs(nodes) do ui.destroy(node, true) end
    nodes = {}
    for i, n in ipairs(list) do
      nodes[i] = note_node(i, n)
      ui.reparent(nodes[i], layer)
    end
  end
  rebuild()
  if spec.position ~= nil then
    H.add(grid, ui.Rect { z = 4, width = style.hatched and 1 or 2, height = gh, color = style.ink,
      x = function() return clamp01(get(spec.position)) * (gw - 2) end })
  end
  return root
end

-- -------------------------------------------------------- step_sequencer --

--- A step sequencer: a row of steps per track, a kit Selection grid in
--- "multi" mode -- a click toggles a step, the arrows walk the grid and
--- Space toggles the step there -- the playhead's column lit. `spec`:
--- `tracks` (names, or `{ {name=, steps={...}} }`), `steps` (16), `beat`
--- (4: every fourth step's ground stronger), `pattern` (per track a list
--- of the steps on, or of booleans), `position` (a function returning the
--- step playing, 1.., 0 for none), `width` (260), `height` (190), `label`,
--- `on_changed(track, step, on)`, `on_pattern(pattern)`, `id` (the grid
--- `<id>-grid`, steps `<id>-step-<track>-<step>`).
function M.step_sequencer(spec, style)
  local W, Hh = spec.width or 260, spec.height or 190
  local root = H.frame(spec, style, "Step sequencer", W, Hh)
  local pad = H.pad(spec, style)
  local tracks = {}
  local pattern = get(spec.pattern) or {}
  for i, t in ipairs(get(spec.tracks) or { "Kick", "Snare", "Hat", "Clap" }) do
    tracks[i] = type(t) == "table" and (t.name or t.label or ("Track " .. i)) or tostring(t)
    if type(t) == "table" and t.steps and not pattern[i] then pattern[i] = t.steps end
  end
  local steps, beat = spec.steps or 16, spec.beat or 4
  local nt = #tracks
  local LW = 46
  local gap = 2
  local gw = W - 2 * pad - LW
  local cw = math.floor((gw + gap) / steps) - gap
  local rh = math.min(28, math.floor((Hh - 2 * pad + gap) / nt) - gap)
  local gh = nt * (rh + gap) - gap
  local y0 = pad + math.floor((Hh - 2 * pad - gh) / 2)
  local x0 = pad + LW + math.floor((gw - (steps * (cw + gap) - gap)) / 2)
  for i, name in ipairs(tracks) do
    H.add(root, H.cap(style, { text = name, x = pad, y = y0 + (i - 1) * (rh + gap) + rh / 2 - 8, width = LW - 6,
      color = style.ink, font_size = H.small(style) + 1 }))
  end
  -- The steps on, as the selection's indices.
  local selected = {}
  for ti = 1, nt do
    local row = pattern[ti] or {}
    for k, v in pairs(row) do
      if v == true then selected[#selected + 1] = (ti - 1) * steps + k
      elseif type(v) == "number" and v >= 1 and v <= steps then selected[#selected + 1] = (ti - 1) * steps + v end
    end
  end
  local on_now = {}
  for _, idx in ipairs(selected) do on_now[idx] = true end
  local items = {}
  for ti = 1, nt do for k = 1, steps do items[#items + 1] = ("%s %d"):format(tracks[ti], k) end end
  local function dump()
    local out = {}
    for ti = 1, nt do
      out[ti] = {}
      for k = 1, steps do if on_now[(ti - 1) * steps + k] then out[ti][#out[ti] + 1] = k end end
    end
    return out
  end
  local node, t
  node, t = require("lib.kit.selection").make("grid_selection", {
    id = spec.id and (spec.id .. "-grid") or nil, x = x0, y = y0, orientation = "grid", mode = "multi", columns = steps,
    gap = gap, item_width = cw, item_height = rh, items = items, selected = selected, current = 0,
    accessible_name = type(spec.label) == "string" and spec.label or "Steps",
    item_id = function(index)
      local ti, k = math.floor((index - 1) / steps) + 1, (index - 1) % steps + 1
      return spec.id and ("%s-step-%d-%d"):format(spec.id, ti, k) or nil
    end,
    delegate = function(index, _, s)
      local ti, k = math.floor((index - 1) / steps) + 1, (index - 1) % steps + 1
      local tone = style.series(ti)
      local downbeat = (k - 1) % beat == 0
      local function focus() return s.current() and t and t.visual_focus end
      if style.hatched then
        local cell = ui.Item { anchors = { fill = true },
          ui.Rect { anchors = { fill = true }, color = function() return s.selected() and get(tone):alpha(.25) or get(style.surface) end,
            border_width = 1, border_color = function()
              if s.selected() then return get(tone) end
              return downbeat and get(style.stroke_of and style.stroke_of("idle") or style.line) or get(style.line)
            end } }
        if cw >= 6 then
          H.add(cell, style.stripes.box { width = cw, height = rh, gap = 4, weight = 1, color = H.alpha(tone, .8),
            opacity = function() return s.selected() and 1 or 0 end })
        end
        H.add(cell, ui.Rect { x = -2, y = -2, width = cw + 4, height = rh + 4, color = "transparent", border_width = 1,
          border_color = style.ink, visible = focus })
        return cell
      end
      return ui.Item { anchors = { fill = true },
        ui.Rect { anchors = { fill = true }, radius = function() return s.selected() and math.min(6, cw / 3) or math.min(cw, rh) / 2 end,
          color = function()
            if s.selected() then return get(tone) end
            return downbeat and get(style.track) or get(style.track):alpha(.6)
          end,
          scale = function() return s.down() and .86 or 1 end,
          behavior = { radius = style.spring(420, 20), scale = style.spring(520, 18), color = { duration = 120 } } },
        ui.Rect { x = -2, y = -2, width = cw + 4, height = rh + 4, radius = 6, color = "transparent", border_width = 2,
          border_color = style.ink, visible = focus } }
    end,
    on_selection_changed = function(list)
      local next = {}
      if type(list) == "string" then for v in list:gmatch("%d+") do next[tonumber(v)] = true end
      elseif type(list) == "table" then for _, v in ipairs(list) do next[tonumber(v)] = true end end
      for idx = 1, nt * steps do
        if (next[idx] or false) ~= (on_now[idx] or false) then
          on_now[idx] = next[idx]
          if spec.on_changed then spec.on_changed(math.floor((idx - 1) / steps) + 1, (idx - 1) % steps + 1, next[idx] == true) end
        end
      end
      if spec.on_pattern then spec.on_pattern(dump()) end
    end,
  })
  H.add(root, node)
  -- The playhead's column, over the steps (it takes no pointer).
  if spec.position ~= nil then
    local function at() return math.floor(tonumber(get(spec.position)) or 0) end
    H.add(root, ui.Rect { z = 5, y = y0 - 2, width = cw + 4, height = gh + 4,
      x = function() return x0 + (math.max(1, at()) - 1) * (cw + gap) - 2 end,
      visible = function() local a = at() return a >= 1 and a <= steps end,
      radius = style.hatched and 0 or 6,
      color = style.hatched and "transparent" or H.alpha(style.ink, .14),
      border_width = style.hatched and 1 or 0, border_color = style.accent })
  end
  return root
end

return M
