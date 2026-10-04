-- The layout maths of the structured charts (lib.kit.display.layout).
--
--     morf test --no-dbus library/tests/display_layout_spec.lua
local test = morf.test

local function check(name, body)
  test.it(name, function()
    test.load { source = 'local L = require("lib.kit.display.layout")\n'
      .. 'local function near(a, b, eps) return math.abs(a - b) <= (eps or 1e-6) end\n'
      .. body .. '\nmorf.ipc.ok = function() return true end' }
    test.truthy(test.ipc("ok"))
    test.eq(#test.logs("error"), 0)
  end)
end

check("matrices and series become the runs plots read", [[
  local flat, rows, cols = L.columns { { 1, 2, 3 }, { 4, 5, 6 } }
  assert(rows == 2 and cols == 3)
  assert(table.concat(flat, ",") == "1,4,2,5,3,6")
  assert(table.concat(L.interleave { { 1, 2 }, { 3, 4 }, { 5, 6 } }, ",") == "1,3,5,2,4,6")
  assert(L.total { 1, -2, "x", 3 } == 4)
  assert(L.value { children = { { value = 2 }, { children = { { value = 3 }, { value = 1 } } } } } == 6)
]])

check("pie slices share the turn in proportion, padded inside their share", [[
  local s = L.pie({ 1, 1, 2 })
  assert(near(s[1].from, 0) and near(s[1].sweep, 90) and near(s[2].from, 90) and near(s[3].sweep, 180))
  assert(near(s[3].fraction, 0.5))
  local p = L.pie({ 1, 1 }, { pad = 4, start = -90, sweep = 180 })
  assert(near(p[1].from, -88) and near(p[1].sweep, 86) and near(p[2].from, 2))
  assert(#L.pie({}) == 0 and L.pie({ 0, 0 })[1].sweep == 0)
]])

check("squarified tiles cover the box with areas in proportion", [[
  local values = { 6, 6, 4, 3, 2, 2, 1 }
  local tiles = L.squarify(values, 0, 0, 600, 400)
  local area = 0
  for i, t in ipairs(tiles) do
    area = area + t.w * t.h
    assert(near(t.w * t.h, values[i] / 24 * 240000, 1e-3), "tile " .. i .. " area")
    assert(t.x >= -1e-6 and t.y >= -1e-6 and t.x + t.w <= 600 + 1e-6 and t.y + t.h <= 400 + 1e-6)
  end
  assert(near(area, 240000, 1e-3))
  -- The first two (the classic example) are squarish halves of the left.
  assert(near(tiles[1].w, 300, 1e-6) and near(tiles[1].h, 200, 1e-6))
  local leaves = L.treemap({ children = { { name = "a", children = { { value = 3 }, { value = 1 } } }, { name = "b", value = 4 } } },
    100, 100, { gap = 0 })
  assert(#leaves == 3 and leaves[1].group == 1 and leaves[3].group == 2 and leaves[1].parent == "a")
  assert(near(leaves[1].w * leaves[1].h, 3750, 1e-3) and near(leaves[3].w * leaves[3].h, 5000, 1e-3))
]])

check("sunburst rings split each parent's sweep among its children", [[
  local arcs = L.sunburst({ children = {
    { name = "a", children = { { name = "a1", value = 1 }, { name = "a2", value = 3 } } },
    { name = "b", value = 4 } } }, { inner = 0.2, outer = 1 })
  assert(#arcs == 4)
  local by = {} for _, a in ipairs(arcs) do by[a.name] = a end
  assert(near(by.a.sweep, 180) and near(by.b.from, 180) and by.b.depth == 1)
  assert(near(by.a1.sweep, 45) and near(by.a2.from, 45) and near(by.a2.sweep, 135) and by.a2.group == 1)
  assert(near(by.a.r0, 0.2) and near(by.a.r1, 0.6) and near(by.a1.r0, 0.6) and near(by.a1.r1, 1))
]])

check("sankey nodes take their longest-path column and one scale", [[
  local nodes, links, columns = L.sankey({ "in", "a", "b", "out" }, {
    { source = "in", target = "a", value = 6 }, { source = "in", target = "b", value = 4 },
    { source = "a", target = "out", value = 6 }, { source = 3, target = 4, value = 4 },
  }, 300, 110, { gap = 10, node_width = 10 })
  assert(columns == 3)
  assert(nodes[1].column == 1 and nodes[2].column == 2 and nodes[3].column == 2 and nodes[4].column == 3)
  assert(near(nodes[1].x, 0) and near(nodes[4].x, 290) and near(nodes[2].x, 145))
  -- The middle column has the least room per unit: 100 px for 10.
  assert(near(nodes[2].h, 60) and near(nodes[3].h, 40) and near(nodes[1].h, 100) and near(nodes[1].y, 5))
  -- Ribbons stack at both ends and are as thick as their value.
  assert(near(links[1].y0, nodes[1].y) and near(links[2].y0, nodes[1].y + 60) and near(links[1].t, 60))
  assert(near(links[1].x0, 10) and near(links[1].x1, 145))
  assert(L.ribbon_d(links[1]):match("^M10%.00 5%.00 C"))
]])

check("funnel stages narrow from one value to the next", [[
  local s = L.funnel({ 100, 50, 5 }, 200, 92, { gap = 4, min = 0.1 })
  assert(#s == 3 and near(s[1].h, 28) and near(s[2].y, 32))
  assert(near(s[1].top, 200) and near(s[1].bottom, 100) and near(s[2].bottom, 20) and near(s[3].top, 20))
  assert(L.trapezoid_d(200, 0, 28, 200, 100):match("^M0%.00 0%.00 L200%.00 0%.00 L150%.00 28%.00"))
  assert(L.trapezoid_d(200, 0, 28, 200, 100, 4):find("Q", 1, true))
]])

check("flame frames stack children over their parent's share", [[
  local f = L.flame({ name = "main", value = 10, children = { { name = "a", value = 6, children = { { name = "x", value = 3 } } },
    { name = "b", value = 2 } } }, 100, 60, { row = 19, gap = 1 })
  assert(#f == 4)
  assert(f[1].name == "main" and near(f[1].y, 41) and near(f[1].w, 100))
  assert(f[2].name == "a" and near(f[2].w, 60) and near(f[2].y, 21) and f[2].depth == 1)
  assert(f[3].name == "x" and near(f[3].w, 30) and near(f[3].y, 1))
  assert(f[4].name == "b" and near(f[4].x, 60) and near(f[4].w, 20))
  local ice = L.flame({ name = "r", value = 1, children = { { name = "c", value = 1, children = { { name = "d", value = 1 } } } } },
    10, 40, { row = 19, gap = 1, icicle = true })
  assert(#ice == 2 and near(ice[1].y, 0) and near(ice[2].y, 20))
]])

check("gantt bars sit on their row across the time range", [[
  local bars, lo, hi = L.gantt({ { label = "a", start = 0, finish = 5 }, { label = "b", start = 5, finish = 10, progress = .5 } },
    200, 40, { gap = 4 })
  assert(lo == 0 and hi == 10)
  assert(near(bars[1].x, 0) and near(bars[1].w, 100) and near(bars[1].y, 2) and near(bars[1].h, 16))
  assert(near(bars[2].x, 100) and near(bars[2].y, 22) and bars[2].progress == .5 and bars[2].label == "b")
  local clipped = L.gantt({ { start = -5, finish = 20 } }, 100, 10, { from = 0, to = 10 })
  assert(near(clipped[1].x, 0) and near(clipped[1].w, 100))
  assert(near(L.scale(2.5, 0, 10, 100), 25))
  assert(L.rect_d(0, 0, 10, 10, 0) == "M0.00 0.00 h10.00 v10.00 h-10.00 Z " and L.rect_d(0, 0, 0, 5) == "")
]])
