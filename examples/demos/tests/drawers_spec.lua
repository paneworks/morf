-- `examples/demos/motion/drawers.lua`: drawers that slide out of a screen-edge frame,
-- stretching as they go.
--
--     morf test examples/demos/tests/drawers_spec.lua
--     nixVulkanIntel morf test examples/demos/tests/drawers_spec.lua   # with snapshots

local test = morf.test

local W, H, THICK = 1280, 720, 10

local function box(edge)
  return test.get { id = "drawer-" .. edge }
end

test.describe("drawers", function()
  test.before_each(function()
    test.load("../motion/drawers.lua", { size = { W, H } })
    test.settle()
  end)

  test.it("covers the screen and lets the pointer through", function()
    local surface = test.surfaces()[1]
    test.eq({ surface.width, surface.height }, { W, H })
    test.eq(test.ipc("opened"), "")
  end)

  test.it("keeps every drawer out of sight until it is asked for", function()
    test.truthy(box("top").y + box("top").height < 0, "top is above the screen")
    test.truthy(box("bottom").y > H, "bottom is below it")
    test.truthy(box("left").x + box("left").width < 0, "left is off the left")
    test.truthy(box("right").x > W, "right is off the right")
  end)

  test.it("slides a drawer out against its edge of the frame", function()
    test.ipc("open", "top")
    test.settle()
    local top = box("top")
    test.near(top.y, THICK, 1e-6)
    test.near(top.x + top.width / 2, W / 2, 1e-6)
    test.eq({ top.width, top.height }, { 420, 150 })
    test.eq(test.ipc("opened"), "top")
  end)

  test.it("stretches along the slide and comes back to square", function()
    test.ipc("open", "left")
    local tallest, widest = 0, 0
    for _ = 1, 12 do
      test.advance(16)
      local left = box("left")
      widest = math.max(widest, left.width)
      tallest = math.max(tallest, left.height)
    end
    -- Sliding right: wider than itself, and narrower across the motion.
    test.truthy(widest > 262, "stretched along the motion: " .. widest)
    test.settle()
    local left = box("left")
    test.near(left.width, 260, 1e-6)
    test.near(left.height, 360, 1e-6)
    test.near(left.x, THICK, 1e-6)
  end)

  test.it("opens and closes every drawer", function()
    test.ipc("open", "all")
    test.settle()
    test.eq(test.ipc("opened"), "top right bottom left")
    test.near(box("bottom").y + box("bottom").height, H - THICK, 1e-6)
    test.near(box("right").x + box("right").width, W - THICK, 1e-6)
    test.ipc("close", "all")
    test.settle()
    test.eq(test.ipc("opened"), "")
    test.truthy(box("right").x > W)
  end)

  test.it("refuses an edge it does not have", function()
    test.raises(function() test.ipc("open", "middle") end, "no drawer")
  end)

  test.it("says nothing is wrong", function()
    test.ipc("toggle", "all")
    test.settle()
    test.eq(test.logs("warn"), {})
  end)

  test.it("draws itself, where there is a GPU", function()
    test.ipc("open", "all")
    test.settle()
    local drawn, where = test.snapshot("drawers.png")
    if drawn then test.note("wrote " .. where) end
  end)
end)
