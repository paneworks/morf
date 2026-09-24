-- examples/terminal.lua, headless: a real program on a real pseudo-terminal,
-- with nothing on screen.
--
--     morf test examples/tests/terminal_spec.lua
--
-- Not hermetic the way counter_spec.lua is: the terminal runs btop (or top)
-- for real, so this waits on the wall clock with `test.wait` rather than
-- only advancing the virtual one.

local test = morf.test

test.describe("terminal", function()
  test.before_each(function()
    test.load("../terminal.lua", { size = { 1280, 800 } })
  end)

  test.it("is a floating panel of the size it asks for", function()
    local surface = test.surfaces()[1]
    test.eq({ surface.width, surface.height }, { 1000, 640 })
    test.eq({ surface.x, surface.y }, { 140, 80 })
  end)

  test.it("shows the program's screen", function()
    local screen = test.wait(function()
      local text = test.ipc("text")
      return text:find("%S") and text
    end, 5000, "the program drew nothing")
    test.truthy(#screen > 0)
  end)

  test.it("titles itself with the grid's size", function()
    local size = test.wait(function()
      return test.find(function(node)
        return node.text and node.text:find("^%d+ × %d+$")
      end)
    end, 5000)
    local columns, rows = size.text:match("^(%d+) × (%d+)$")
    test.truthy(tonumber(columns) > 80, "columns")
    test.truthy(tonumber(rows) > 20, "rows")
  end)
end)
