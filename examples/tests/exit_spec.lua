-- `examples/exit.lua`: notifications and a panel that animate out before
-- they are removed, and back in when they are put back while leaving.
--
--     morf test examples/tests/exit_spec.lua
--     nixVulkanIntel morf test examples/tests/exit_spec.lua   # with snapshots

local test = morf.test

local function note(n)
  return test.find { id = "note-" .. n }
end

test.describe("exit", function()
  test.before_each(function()
    test.load("../exit.lua")
    test.settle()
  end)

  test.it("starts with four notifications and the panel open", function()
    test.eq(test.ipc("count"), 4)
    for n = 1, 4 do
      test.truthy(note(n), "note " .. n)
      test.falsy(note(n).exiting)
    end
    test.truthy(test.find { id = "panel" })
    test.eq(test.logs("warn"), {})
  end)

  test.it("keeps a dismissed notification drawn, out of the flow, until it has left", function()
    local second, third = note(2), note(3)
    test.ipc("dismiss", 2)
    test.eq(test.ipc("count"), 3)
    local leaving = note(2)
    test.truthy(leaving, "still in the tree")
    test.truthy(leaving.exiting)
    test.eq(leaving.handle, second.handle)
    test.eq(leaving.y, second.y, "where it was")
    test.eq(note(3).y, second.y, "the one below closed up at once")
    test.ne(note(3).y, third.y)
    test.advance(120)
    local midway = note(2)
    test.truthy(midway.opacity > 0.05 and midway.opacity < 0.95, "fading: " .. midway.opacity)
    test.truthy(midway.y > second.y, "and dropping: " .. midway.y)
    test.settle()
    test.falsy(note(2), "removed once its exit ended")
  end)

  test.it("takes a notification put back while leaving and animates it home", function()
    local second = note(2)
    test.ipc("dismiss", 2)
    test.advance(96)
    test.ipc("restore")
    local back = note(2)
    test.eq(back.handle, second.handle, "the same node")
    test.falsy(back.exiting)
    test.settle()
    test.near(note(2).opacity, 1, 1e-6)
    test.eq(note(2).y, second.y)
    test.eq(note(3).y, second.y + second.height + 10)
  end)

  test.it("plays the panel's exit when its Loader lets go, and takes it back when asked again", function()
    local panel = test.get { id = "panel" }
    test.ipc("toggle")
    test.advance(64)
    local leaving = test.get { id = "panel" }
    test.truthy(leaving.exiting)
    test.ipc("toggle")
    test.eq(test.get({ id = "panel" }).handle, panel.handle, "not built again")
    test.falsy(test.get({ id = "panel" }).exiting)
    test.settle()
    test.ipc("toggle")
    test.settle()
    test.falsy(test.find { id = "panel" }, "gone once it has left")
  end)

  test.it("draws the way out, frame by frame, where there is a GPU", function()
    test.ipc("dismiss", 2)
    test.ipc("toggle")
    local drawn, where
    local at = 0
    for _, ms in ipairs { 0, 64, 128, 192, 300 } do
      test.advance(ms - at)
      at = ms
      drawn, where = test.snapshot(("exit_%03d.png"):format(ms))
    end
    if drawn then test.note("wrote " .. where) end
  end)
end)
