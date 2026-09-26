-- `library/lib/m3shapes.lua` and `examples/demos/sdf/m3shapes.lua`: Material 3's
-- shapes as path outlines that morph into one another.
--
--     morf test examples/demos/tests/m3shapes_spec.lua
--     nixVulkanIntel morf test examples/demos/tests/m3shapes_spec.lua   # with snapshots

local test = morf.test
local shapes = require("lib.m3shapes")

local function count(d, pattern)
  local n = 0
  for _ in d:gmatch(pattern) do n = n + 1 end
  return n
end

test.describe("m3shapes", function()
  test.it("cuts every shape into the same run of curves", function()
    for _, name in ipairs(shapes.NAMES) do
      local d = shapes.path(name)
      test.eq(count(d, "C"), shapes.SEGMENTS, name)
      test.eq(count(d, "M"), 1, name)
      test.truthy(d:sub(-1) == "Z", name)
    end
  end)

  test.it("rounds a polygon's corners as far as they go into a circle", function()
    for _, c in ipairs(shapes.curves("circle")) do
      local r = math.sqrt((c[7] - 0.5) ^ 2 + (c[8] - 0.5) ^ 2)
      test.near(r, 0.5, 0.003)
    end
  end)

  test.it("fits every shape in the unit square and starts at the top", function()
    for _, name in ipairs(shapes.NAMES) do
      local curves = shapes.curves(name)
      for _, c in ipairs(curves) do
        for k = 1, 8 do test.truthy(c[k] > -0.2 and c[k] < 1.2, name) end
      end
      -- The first point is above the centre (the smallest angle from up).
      test.truthy(curves[1][2] < 0.5, name .. " starts above the centre")
    end
  end)

  test.it("morphs from one shape to the next", function()
    test.load("../sdf/m3shapes.lua")
    test.settle()
    test.snapshot("m3shapes-rest.png")
    test.advance(1000)
    test.advance(64)
    test.snapshot("m3shapes-mid.png")
    test.advance(700)
    test.snapshot("m3shapes-next.png")
  end)
end)
