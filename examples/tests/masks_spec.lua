-- `examples/masks.lua`: cards seen through node and gradient masks, and a
-- list that fades at its edges while it scrolls.
--
--     morf test examples/tests/masks_spec.lua
--     nixVulkanIntel morf test examples/tests/masks_spec.lua   # with snapshots

local test = morf.test

local W, H = 980, 470

local function row(index)
  return test.get { text = string.format("Notification %02d", index) }
end

test.describe("masks", function()
  test.before_each(function()
    test.load("../masks.lua", { size = { 1280, 720 } })
    test.settle()
  end)

  test.it("lays out at its own size", function()
    local surface = test.surfaces()[1]
    test.eq({ surface.width, surface.height }, { W, H })
  end)

  test.it("lays a mask out in the box of the node it masks", function()
    -- The text stencil is centred in its 180-pixel plate.
    local stencil = test.get { text = "morf" }
    local plate = test.get(function(node)
      return node.element == "Rect" and node.width == 180 and node.height == 180
        and node.y > 200 and node.x < 100
    end)
    test.near(stencil.x + stencil.width / 2, plate.x + plate.width / 2, 1)
    test.near(stencil.y + stencil.height / 2, plate.y + plate.height / 2, 1)
  end)

  test.it("keeps the list's fade where its edges are while it scrolls", function()
    local list = test.get { id = "list" }
    local before = row(8).y
    test.ipc("scroll", 200)
    test.eq(row(8).y, before - 80, "the rows moved by the difference")
    -- The list itself, and so its mask's box, did not move.
    local after = test.get { id = "list" }
    test.eq({ after.x, after.y, after.width, after.height },
      { list.x, list.y, list.width, list.height })
  end)

  test.it("says nothing is wrong", function()
    test.ipc("scroll", 400)
    test.settle()
    test.eq(test.logs("warn"), {})
  end)

  test.it("draws itself, where there is a GPU", function()
    local drawn, where = test.snapshot("masks.png")
    if drawn then test.note("wrote " .. where) end
  end)
end)
