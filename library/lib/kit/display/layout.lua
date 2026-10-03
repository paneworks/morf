-- The layout maths of the structured charts (lib.kit.display.charts):
-- where a pie's slices, a treemap's tiles, a sunburst's rings, a sankey's
-- nodes and ribbons, a funnel's stages, a flame graph's frames and a
-- gantt's bars go. Pure functions of numbers -- no nodes, no colours -- so
-- they are tested on their own (library/tests/display_layout_spec.lua) and
-- run only when a chart's data changes, never per frame.
--
-- Angles are degrees clockwise from twelve o'clock (morf.geometry's).
-- A hierarchy is `{ name, value, children = { ... } }`; a node's value is
-- its own when given, else the sum of its children's.

local L = {}

local function num(v) v = tonumber(v) return (v and v == v) and v or 0 end

--- The sum of a list of numbers (non-numbers count 0, negatives 0).
function L.total(values)
  local s = 0
  for _, v in ipairs(values or {}) do s = s + math.max(0, num(v)) end
  return s
end

--- A hierarchy node's value: its own, else the sum of its children's.
function L.value(node)
  if type(node) ~= "table" then return math.max(0, num(node)) end
  if node.value ~= nil then return math.max(0, num(node.value)) end
  local s = 0
  for _, child in ipairs(node.children or {}) do s = s + L.value(child) end
  return s
end

--- Rows of a matrix (`{ {r1c1, r1c2, ...}, {r2c1, ...} }`) as the flat
--- column-major run a `cells` plot reads, and how many rows it has.
function L.columns(matrix)
  local rows = #matrix
  local cols = 0
  for _, row in ipairs(matrix) do cols = math.max(cols, #row) end
  local out = {}
  for c = 1, cols do
    for r = 1, rows do out[#out + 1] = num(matrix[r][c]) end
  end
  return out, rows, cols
end

--- Several series (lists of equal length) interleaved sample by sample,
--- the run a `stack`/`stack_bars` plot reads with `layers = #series`.
function L.interleave(series)
  local out, n = {}, 0
  for _, s in ipairs(series) do n = math.max(n, #s) end
  for i = 1, n do
    for _, s in ipairs(series) do out[#out + 1] = num(s[i]) end
  end
  return out
end

-- ------------------------------------------------------------------ pie --

--- Slices of a pie: `{ from, sweep, value, fraction }` per value, laid
--- clockwise from `opts.start` (0) across `opts.sweep` (360), each slice
--- `opts.pad` degrees (0) narrower, centred in its share.
function L.pie(values, opts)
  opts = opts or {}
  local start, span, pad = opts.start or 0, opts.sweep or 360, opts.pad or 0
  local total = L.total(values)
  local out, at = {}, start
  for i, v in ipairs(values or {}) do
    local f = total > 0 and math.max(0, num(v)) / total or 0
    local sweep = span * f
    local p = math.min(pad, sweep * 0.5)
    out[i] = { from = at + p / 2, sweep = math.max(0, sweep - p), value = num(v), fraction = f, mid = at + sweep / 2 }
    at = at + sweep
  end
  return out
end

-- -------------------------------------------------------------- treemap --

local function worst(row, sum, side)
  local mx, mn = 0, math.huge
  for _, a in ipairs(row) do mx = math.max(mx, a) mn = math.min(mn, a) end
  if sum <= 0 or mn <= 0 then return math.huge end
  local s2, side2 = sum * sum, side * side
  return math.max(side2 * mx / s2, s2 / (side2 * mn))
end

--- Squarified tiles (Bruls, Huizing, van Wijk) for `values` in the box
--- `x, y, w, h`: one `{ x, y, w, h }` per value, in the values' order, each
--- with an area in proportion to its value and as square as the row allows.
function L.squarify(values, x, y, w, h)
  local order = {}
  for i = 1, #values do order[i] = i end
  table.sort(order, function(a, b) return num(values[a]) > num(values[b]) end)
  local total = L.total(values)
  local out = {}
  for i = 1, #values do out[i] = { x = x, y = y, w = 0, h = 0 } end
  if total <= 0 or w <= 0 or h <= 0 then return out end
  local scale = w * h / total
  local i, n = 1, #order
  while i <= n do
    local side = math.min(w, h)
    local row, ids = {}, {}
    local sum = 0
    local j = i
    while j <= n do
      local a = math.max(0, num(values[order[j]])) * scale
      if #row > 0 then
        local trial = { table.unpack(row) }
        trial[#trial + 1] = a
        if worst(trial, sum + a, side) > worst(row, sum, side) then break end
      end
      row[#row + 1], ids[#ids + 1], sum = a, order[j], sum + a
      j = j + 1
    end
    if w >= h then
      -- A column down the left side.
      local cw = h > 0 and sum / h or 0
      local yy = y
      for k, a in ipairs(row) do
        local rh = cw > 0 and a / cw or 0
        out[ids[k]] = { x = x, y = yy, w = cw, h = rh }
        yy = yy + rh
      end
      x, w = x + cw, w - cw
    else
      -- A row along the top.
      local rh = w > 0 and sum / w or 0
      local xx = x
      for k, a in ipairs(row) do
        local rw = rh > 0 and a / rh or 0
        out[ids[k]] = { x = xx, y = y, w = rw, h = rh }
        xx = xx + rw
      end
      y, h = y + rh, h - rh
    end
    i = j
  end
  return out
end

--- A two-level treemap of `tree` in `w` by `h`: its children squarified
--- (the groups), each group's children squarified inside it (a group with
--- none is one leaf). Returns the leaves `{ x, y, w, h, group, name, value,
--- parent }`, each tile `opts.gap` (2) inside its share, and the groups'
--- boxes as a second list.
function L.treemap(tree, w, h, opts)
  opts = opts or {}
  local gap = opts.gap or 2
  local groups = (type(tree) == "table" and (tree.children or tree)) or {}
  local values = {}
  for i, g in ipairs(groups) do values[i] = L.value(g) end
  local boxes = L.squarify(values, 0, 0, w, h)
  local leaves = {}
  for gi, g in ipairs(groups) do
    local b = boxes[gi]
    local kids = type(g) == "table" and g.children or nil
    if kids and #kids > 0 then
      local kv = {}
      for k, c in ipairs(kids) do kv[k] = L.value(c) end
      for k, t in ipairs(L.squarify(kv, b.x, b.y, b.w, b.h)) do
        leaves[#leaves + 1] = { x = t.x + gap / 2, y = t.y + gap / 2, w = math.max(0, t.w - gap), h = math.max(0, t.h - gap),
          group = gi, name = kids[k].name, value = kv[k], parent = g.name }
      end
    else
      leaves[#leaves + 1] = { x = b.x + gap / 2, y = b.y + gap / 2, w = math.max(0, b.w - gap), h = math.max(0, b.h - gap),
        group = gi, name = type(g) == "table" and g.name or nil, value = values[gi] }
    end
  end
  return leaves, boxes
end

-- ------------------------------------------------------------- sunburst --

--- The rings of a sunburst: every node below `tree`'s root as `{ depth,
--- group, from, sweep, r0, r1, name, value }`, depth 1 the innermost ring
--- from radius `opts.inner` (0) to `opts.outer` (1) split into `opts.depth`
--- rings (the tree's depth), a child's sweep its share of its parent's.
function L.sunburst(tree, opts)
  opts = opts or {}
  local inner, outer = opts.inner or 0, opts.outer or 1
  local function depth_of(node)
    local d = 0
    for _, c in ipairs(type(node) == "table" and node.children or {}) do d = math.max(d, depth_of(c)) end
    return d + 1
  end
  local levels = opts.depth or (depth_of(tree) - 1)
  levels = math.max(1, levels)
  local band = (outer - inner) / levels
  local out = {}
  local function walk(node, depth, from, sweep, group)
    local kids = type(node) == "table" and node.children or {}
    local total = 0
    for _, c in ipairs(kids) do total = total + L.value(c) end
    -- A parent whose own value exceeds its children's leaves the rest empty.
    total = math.max(total, depth > 0 and L.value(node) or total)
    local at = from
    for i, c in ipairs(kids) do
      local s = total > 0 and sweep * L.value(c) / total or 0
      if depth + 1 <= levels then
        local g = group or i
        out[#out + 1] = { depth = depth + 1, group = g, from = at, sweep = s,
          r0 = inner + depth * band, r1 = inner + (depth + 1) * band, name = c.name, value = L.value(c) }
        walk(c, depth + 1, at, s, g)
      end
      at = at + s
    end
  end
  walk(tree, 0, opts.start or 0, opts.sweep or 360, nil)
  return out
end

-- --------------------------------------------------------------- sankey --

--- A sankey's layout in `w` by `h`. `nodes`: `{ name }` (or names);
--- `links`: `{ source, target, value }` with 1-based node indices (or
--- names). Nodes take the column of their longest path from a source;
--- each column is stacked top-down, centred, `opts.gap` (8) apart, a node
--- `opts.node_width` (8) wide and as tall as the larger of what flows in
--- and out, all columns on one scale. Returns `nodes` `{ x, y, w, h,
--- column, name, value }`, `links` `{ source, target, value, x0, y0, x1,
--- y1, t }` (a ribbon from `x0, y0` to `x1, y1`, `t` thick) and the column
--- count.
function L.sankey(nodes, links, w, h, opts)
  opts = opts or {}
  local gap, nw = opts.gap or 8, opts.node_width or 8
  local index, N = {}, {}
  for i, n in ipairs(nodes or {}) do
    local name = type(n) == "table" and n.name or tostring(n)
    index[name] = i
    N[i] = { name = name, inflow = 0, outflow = 0, column = 0, out = {}, ["in"] = {} }
  end
  local function at(v) return type(v) == "number" and v or index[v] end
  local Lk = {}
  for _, l in ipairs(links or {}) do
    local s, t = at(l.source), at(l.target)
    if s and t and N[s] and N[t] and s ~= t then
      local link = { source = s, target = t, value = math.max(0, num(l.value)) }
      Lk[#Lk + 1] = link
      N[s].outflow = N[s].outflow + link.value
      N[t].inflow = N[t].inflow + link.value
      table.insert(N[s].out, link)
      table.insert(N[t]["in"], link)
    end
  end
  -- Columns: the longest path from a source (relaxed, bounded for cycles).
  for _ = 1, #N do
    local moved = false
    for _, link in ipairs(Lk) do
      local c = N[link.source].column + 1
      if c > N[link.target].column and c < #N then N[link.target].column = c moved = true end
    end
    if not moved then break end
  end
  local columns = 0
  for _, n in ipairs(N) do columns = math.max(columns, n.column + 1) end
  local per = {}
  for c = 1, columns do per[c] = {} end
  for i, n in ipairs(N) do
    n.value = math.max(n.inflow, n.outflow)
    table.insert(per[n.column + 1], i)
  end
  -- One scale: the column with the least room per unit sets it.
  local k = math.huge
  for c = 1, columns do
    local sum = 0
    for _, i in ipairs(per[c]) do sum = sum + N[i].value end
    local room = h - gap * math.max(0, #per[c] - 1)
    if sum > 0 then k = math.min(k, room / sum) end
  end
  if k == math.huge then k = 0 end
  for c = 1, columns do
    local used = 0
    for _, i in ipairs(per[c]) do used = used + N[i].value * k end
    used = used + gap * math.max(0, #per[c] - 1)
    local y = (h - used) / 2
    local x = columns > 1 and (c - 1) * (w - nw) / (columns - 1) or (w - nw) / 2
    for _, i in ipairs(per[c]) do
      local n = N[i]
      n.x, n.y, n.w, n.h = x, y, nw, n.value * k
      y = y + n.h + gap
    end
  end
  -- Ribbons leave a node in the order of their targets, and arrive in the
  -- order of their sources, so they do not cross at the nodes.
  for _, n in ipairs(N) do
    table.sort(n.out, function(a, b) return N[a.target].y < N[b.target].y end)
    table.sort(n["in"], function(a, b) return N[a.source].y < N[b.source].y end)
    local y = n.y
    for _, link in ipairs(n.out) do link.y0 = y y = y + link.value * k end
    y = n.y
    for _, link in ipairs(n["in"]) do link.y1 = y y = y + link.value * k end
  end
  for _, link in ipairs(Lk) do
    link.t = link.value * k
    link.x0 = N[link.source].x + nw
    link.x1 = N[link.target].x
  end
  local out = {}
  for i, n in ipairs(N) do
    out[i] = { x = n.x, y = n.y, w = n.w, h = n.h, column = n.column + 1, name = n.name, value = n.value,
      last = n.column + 1 == columns }
  end
  return out, Lk, columns
end

--- Path data for a sankey ribbon (`L.sankey`'s link): a band `t` thick
--- curving from `x0, y0` to `x1, y1`.
function L.ribbon_d(link)
  local x0, x1, y0, y1, t = link.x0, link.x1, link.y0, link.y1, math.max(0.5, link.t)
  local xm = (x0 + x1) / 2
  return ("M%.2f %.2f C%.2f %.2f %.2f %.2f %.2f %.2f L%.2f %.2f C%.2f %.2f %.2f %.2f %.2f %.2f Z"):format(
    x0, y0, xm, y0, xm, y1, x1, y1, x1, y1 + t, xm, y1 + t, xm, y0 + t, x0, y0 + t)
end

-- --------------------------------------------------------------- funnel --

--- A funnel's stages in `w` by `h`: one band per value, top to bottom,
--- `opts.gap` (4) apart; a band's top edge is as wide as its value (of the
--- largest), its bottom as the next's (its own for the last), never under
--- `opts.min` (0.12) of `w`, centred. Returns `{ y, h, top, bottom, value,
--- fraction }` (`top`/`bottom`: edge widths) per stage.
function L.funnel(values, w, h, opts)
  opts = opts or {}
  local gap, min = opts.gap or 4, opts.min or 0.12
  local n = #(values or {})
  local peak = 0
  for _, v in ipairs(values or {}) do peak = math.max(peak, num(v)) end
  local out = {}
  if n == 0 then return out end
  local bh = (h - gap * (n - 1)) / n
  local function width(v) return w * math.max(min, peak > 0 and num(v) / peak or 0) end
  for i, v in ipairs(values) do
    local nextv = values[i + 1] ~= nil and values[i + 1] or v
    out[i] = { y = (i - 1) * (bh + gap), h = bh, top = width(v), bottom = width(nextv), value = num(v),
      fraction = peak > 0 and num(v) / peak or 0 }
  end
  return out
end

--- Path data for a centred trapezoid in a box `w` wide: `top` wide at `y`,
--- `bottom` wide at `y + h`, its corners rounded by `r` (0).
function L.trapezoid_d(w, y, h, top, bottom, r)
  local cx = w / 2
  local a, b = cx - top / 2, cx + top / 2
  local c, d = cx + bottom / 2, cx - bottom / 2
  r = math.min(r or 0, h / 2, bottom / 2, top / 2)
  if r <= 0.05 then
    return ("M%.2f %.2f L%.2f %.2f L%.2f %.2f L%.2f %.2f Z"):format(a, y, b, y, c, y + h, d, y + h)
  end
  -- Corners cut along each edge by `r`, rounded through the corner.
  local function toward(x0, y0, x1, y1, len)
    local dx, dy = x1 - x0, y1 - y0
    local l = math.sqrt(dx * dx + dy * dy)
    if l < 1e-9 then return x0, y0 end
    return x0 + dx / l * len, y0 + dy / l * len
  end
  local P = { { a, y }, { b, y }, { c, y + h }, { d, y + h } }
  local parts = {}
  for i = 1, 4 do
    local p, prev, nxt = P[i], P[(i - 2) % 4 + 1], P[i % 4 + 1]
    local x1, y1 = toward(p[1], p[2], prev[1], prev[2], r)
    local x2, y2 = toward(p[1], p[2], nxt[1], nxt[2], r)
    parts[#parts + 1] = ("%s%.2f %.2f Q%.2f %.2f %.2f %.2f"):format(i == 1 and "M" or "L", x1, y1, p[1], p[2], x2, y2)
  end
  return table.concat(parts, " ") .. " Z"
end

-- ---------------------------------------------------------- flame graph --

--- A flame graph of `tree` in `w` by `h`: the root across the bottom row,
--- each child above its parent, as wide as its share of the parent's
--- value, laid left to right (an icicle -- the root on top -- when
--- `opts.icicle`). Rows are `opts.row` (18) tall; rows past the box are
--- dropped. Returns `{ x, y, w, h, depth, name, value }`.
function L.flame(tree, w, h, opts)
  opts = opts or {}
  local row = opts.row or 18
  local gap = opts.gap or 1
  local rows = math.max(1, math.floor((h + gap) / (row + gap)))
  local out = {}
  local function walk(node, depth, x, width)
    if depth >= rows or width <= 0 then return end
    local y = opts.icicle and depth * (row + gap) or h - (depth + 1) * (row + gap) + gap
    out[#out + 1] = { x = x, y = y, w = width, h = row, depth = depth, name = node.name, value = L.value(node) }
    local total = L.value(node)
    local at = x
    for _, c in ipairs(node.children or {}) do
      local cw = total > 0 and width * L.value(c) / total or 0
      walk(c, depth + 1, at, cw)
      at = at + cw
    end
  end
  walk(tree, 0, 0, w)
  return out
end

-- ---------------------------------------------------------------- gantt --

--- A gantt's bars in `w` by `h`: one row per task (`{ label, start,
--- finish, progress }`), the time range `opts.from`..`opts.to` (the
--- tasks' when not given) across `w`, each bar `opts.gap` (6) shorter than
--- its row. Returns the bars `{ x, y, w, h, row, label, progress }` and the
--- range used.
function L.gantt(tasks, w, h, opts)
  opts = opts or {}
  local gap = opts.gap or 6
  local lo, hi = opts.from, opts.to
  if lo == nil or hi == nil then
    local a, b = math.huge, -math.huge
    for _, t in ipairs(tasks or {}) do
      a = math.min(a, num(t.start))
      b = math.max(b, num(t.finish))
    end
    if a == math.huge then a, b = 0, 1 end
    lo, hi = lo or a, hi or b
  end
  if hi - lo < 1e-9 then hi = lo + 1 end
  local n = #(tasks or {})
  local rh = n > 0 and h / n or h
  local out = {}
  for i, t in ipairs(tasks or {}) do
    local s = math.max(lo, math.min(hi, num(t.start)))
    local f = math.max(s, math.min(hi, num(t.finish)))
    out[i] = { x = (s - lo) / (hi - lo) * w, y = (i - 1) * rh + gap / 2, w = (f - s) / (hi - lo) * w,
      h = math.max(1, rh - gap), row = i, label = t.label or t.name, progress = t.progress }
  end
  return out, lo, hi
end

--- Where `value` falls across `w` for the range `lo..hi`.
function L.scale(value, lo, hi, w)
  if hi - lo < 1e-9 then return 0 end
  return (num(value) - lo) / (hi - lo) * w
end

-- ---------------------------------------------------------------- paths --

--- Path data for a rectangle, its corners rounded by `r` (0).
function L.rect_d(x, y, w, h, r)
  if w <= 0 or h <= 0 then return "" end
  r = math.max(0, math.min(r or 0, w / 2, h / 2))
  if r <= 0.05 then return ("M%.2f %.2f h%.2f v%.2f h%.2f Z "):format(x, y, w, h, -w) end
  return ("M%.2f %.2f H%.2f A%.2f %.2f 0 0 1 %.2f %.2f V%.2f A%.2f %.2f 0 0 1 %.2f %.2f H%.2f A%.2f %.2f 0 0 1 %.2f %.2f V%.2f A%.2f %.2f 0 0 1 %.2f %.2f Z "):format(
    x + r, y, x + w - r, r, r, x + w, y + r, y + h - r, r, r, x + w - r, y + h, x + r, r, r, x, y + h - r, y + r, r, r, x + r, y)
end

return L
