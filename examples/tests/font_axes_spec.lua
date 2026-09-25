-- `examples/font_axes.lua`: icons whose FILL and weight axes animate when
-- they are selected.
--
--     morf test examples/tests/font_axes_spec.lua
--     nixVulkanIntel morf test examples/tests/font_axes_spec.lua   # with snapshots

local test = morf.test

test.describe("font axes", function()
  test.before_each(function()
    test.load("../font_axes.lua")
    test.settle()
  end)

  test.it("starts with the first icon filled and the rest hollow", function()
    test.eq(test.ipc("selected"), 1)
    test.near(test.ipc("fill", 1), 1, 1e-6)
    test.near(test.ipc("fill", 3), 0, 1e-6)
    test.eq(test.logs("warn"), {})
  end)

  test.it("moves the fill through the values between, as any number moves", function()
    test.click { id = "tab-favorite" }
    test.eq(test.ipc("selected"), 3)
    test.advance(64)
    local rising, falling = test.ipc("fill", 3), test.ipc("fill", 1)
    test.truthy(rising > 0.05 and rising < 0.95, "part way in: " .. rising)
    test.truthy(falling > 0.05 and falling < 0.95, "part way out: " .. falling)
    test.settle()
    test.near(test.ipc("fill", 3), 1, 1e-6)
    test.near(test.ipc("fill", 1), 0, 1e-6)
  end)

  test.it("lays nothing out again for a fill: the icon keeps its box", function()
    local before = test.get { id = "icon-favorite" }
    test.ipc("select", 3)
    test.advance(96)
    local during = test.get { id = "icon-favorite" }
    test.eq({ during.width, during.height }, { before.width, before.height })
  end)

  test.it("draws the selection filling in, where there is a GPU", function()
    test.ipc("select", 3)
    local drawn, where
    local at = 0
    for _, ms in ipairs { 0, 48, 96, 300 } do
      test.advance(ms - at)
      at = ms
      drawn, where = test.snapshot("font_axes_" .. ms .. ".png")
    end
    if drawn then test.note("wrote " .. where) end
  end)
end)
