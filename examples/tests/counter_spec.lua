-- The counter's spec: `morf test examples/tests/counter_spec.lua`.
--
-- Hermetic: `uname` is stubbed, time is virtual, and nothing reaches the
-- session the tests were started in.

local test = morf.test

test.describe("counter", function()
  test.before_each(function()
    test.stub_run("uname", { stdout = "testbox\n" })
    test.load("counter.lua", { size = { 1280, 720 } })
  end)

  test.it("lays out a panel in the middle of the screen", function()
    local surface = test.surfaces()[1]
    test.eq(surface.kind, "primary")
    test.eq({ surface.width, surface.height }, { 320, 200 })
    test.eq({ surface.x, surface.y }, { 480, 260 })
    local panel = test.get { id = "panel" }
    test.eq({ panel.width, panel.height }, { 320, 200 })
  end)

  test.it("starts at zero", function()
    test.eq(test.get({ id = "count" }).text, "count: 0")
    test.eq(test.ipc("count"), 0)
  end)

  test.it("counts clicks on the button", function()
    test.click { id = "increment" }
    test.click { id = "increment" }
    test.eq(test.get({ id = "count" }).text, "count: 2")
    test.eq(test.ipc("count"), 2)
  end)

  test.it("clicks by position, and misses beside the button", function()
    local button = test.get { id = "button" }
    test.click(button.x + 5, button.y + 5)
    test.click(button.x + button.width + 40, button.y + 5)
    test.eq(test.ipc("count"), 1)
  end)

  test.it("turns with the wheel", function()
    test.move { id = "increment" }
    test.wheel(0, 15)
    test.wheel(0, 15)
    test.wheel(0, -15)
    test.eq(test.ipc("count"), 1)
  end)

  test.it("resets over IPC", function()
    test.click { id = "increment" }
    test.eq(test.ipc("reset"), 0)
    test.eq(test.get({ id = "count" }).text, "count: 0")
  end)

  test.it("is ready after a second of its own time", function()
    test.eq(test.get({ id = "status" }).text, "starting")
    test.advance(999)
    test.eq(test.get({ id = "status" }).text, "starting")
    test.advance(1)
    test.eq(test.get({ id = "status" }).text, "ready")
  end)

  test.it("shows what the stubbed command answered", function()
    test.settle()
    test.eq(test.ipc("host"), "testbox")
    test.eq(test.get({ id = "host" }).text, "on testbox")
    test.contains(test.runs(), { "uname", "-n" })
  end)

  test.it("greets whoever types their name", function()
    test.click { id = "name" }
    test.type("Ada")
    test.key("Return")
    test.eq(test.get({ id = "name" }).text, "Ada")
    test.eq(test.get({ id = "greeting" }).text, "hello, Ada")
    test.key("Escape")
    test.eq(test.get({ id = "greeting" }).text, "")
  end)

  test.it("finds nodes by text and by predicate", function()
    test.truthy(test.find { text = "add one" })
    local texts = test.find_all(function(node) return node.element == "Text" end)
    test.eq(#texts, 5)
    test.contains(test.text_of(test.get { id = "panel" }), "add one")
  end)

  test.it("says nothing is wrong", function()
    test.settle()
    test.eq(test.logs("warn"), {})
  end)

  test.it("draws itself, where there is a GPU", function()
    local drawn, where = test.snapshot("counter.png")
    if drawn then test.note("wrote " .. where) end
  end)
end)
