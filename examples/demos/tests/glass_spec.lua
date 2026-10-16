-- `examples/demos/render/glass-cases.lua`: the four glass cases over one
-- busy backdrop.
--
--     morf test examples/demos/tests/glass_spec.lua

local test = morf.test

test.describe("glass cases", function()
  test.before_each(function()
    test.load("../render/glass-cases.lua", { size = { 1200, 420 } })
    test.settle()
  end)

  test.it("lays the four cases out in a row", function()
    local xs = {}
    for _, label in ipairs { "clear", "frosted", "darkened", "lightened" } do
      xs[#xs + 1] = test.get { id = label }.x
    end
    test.eq(xs, { 30, 320, 610, 900 })
  end)

  test.it("says nothing is wrong", function()
    test.eq(test.logs("warn"), {})
  end)

  test.it("draws itself, where there is a GPU", function()
    local drawn, where = test.snapshot("glass.png")
    if drawn then test.note("wrote " .. where) end
  end)
end)
