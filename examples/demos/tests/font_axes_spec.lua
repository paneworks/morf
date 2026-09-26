-- `examples/demos/text/font_axes.lua`: icons whose FILL and weight axes animate when
-- they are selected.
--
--     morf test examples/demos/tests/font_axes_spec.lua
--     nixVulkanIntel morf test examples/demos/tests/font_axes_spec.lua   # with snapshots

local test = morf.test

test.describe("font axes", function()
  test.before_each(function()
    test.load("../text/font_axes.lua")
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

  test.it("sets a wider wdth wider, and small text at its own optical size", function()
    local narrow = test.find { id = "wdth-50" }
    if not narrow then
      test.note("no face with a wdth axis here (FONT_AXES_FLEX=path names one)")
      return
    end
    local normal, wide = test.get { id = "wdth-100" }, test.get { id = "wdth-151" }
    test.truthy(narrow.width < normal.width and normal.width < wide.width,
      ("widths %.1f, %.1f, %.1f"):format(narrow.width, normal.width, wide.width))
    local auto, none = test.get { id = "optical-auto" }, test.get { id = "optical-none" }
    test.truthy(math.abs(auto.width - none.width) > 1,
      ("optical sizing moves the line: %.1f against %.1f"):format(auto.width, none.width))
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
