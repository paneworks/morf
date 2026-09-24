-- impasto driven by input the way a person drives it: clicks, drags, the
-- wheel and keys go through the shell's own pointer and keyboard paths.
-- The scenarios first driven with a real pointer and keyboard in a nested
-- compositor, kept here as regressions.
--
--     morf test --private-bus examples/tests/impasto_input_spec.lua

local test = morf.test

local function load(env)
  local vars = { IMPASTO_DRY_RUN = "1", IMPASTO_INLINE_WALLPAPER = "1" }
  for name, value in pairs(env or {}) do vars[name] = value end
  test.load("../impasto/init.lua", { size = { 1280, 720 }, env = vars })
  test.settle(3000)
end

test.describe("impasto input", function()
  test.it("a click beside an open island closes it", function()
    load()
    test.eq(test.ipc("wifi"), "wifi")
    test.settle(2000)
    test.click(1100, 400)
    test.settle(2000)
    test.ne(test.ipc("wifi"), "", "the island stayed open")
  end)

  test.it("the launcher closes on Escape after the actions mode was typed and erased", function()
    load()
    test.eq(test.ipc("launcher"), "launcher")
    test.settle(2000)
    test.type(">")
    test.settle(500)
    test.key("BackSpace")
    test.settle(500)
    test.key("Escape")
    test.settle(2000)
    test.eq(test.ipc("launcher"), "launcher", "the launcher stayed open")
  end)

  test.it("arranging the control centre: a block dragged onto the tray is removed", function()
    load()
    test.eq(test.ipc("controls"), "controls")
    test.settle(2000)
    test.eq(test.ipc("controls_edit"), "editing")
    test.settle(2000)
    local airplane = test.get { text = "Airplane", visible = true }
    local tray = test.get { text_contains = "drop here to remove", visible = true }
    test.drag({ airplane.x + 10, airplane.y + 5 }, { tray.x + 40, tray.y - 300 }, { steps = 20 })
    test.settle(2000)
    test.falsy(test.find(function(n) return n.text == "Airplane" and n.visible and n.x < tray.x end), "the toggles block is still on the grid")
  end)

  test.it("the arcade: Escape leaves a game for the shelf, and the shelf for nothing", function()
    load()
    test.eq(test.ipc("games"), "games")
    test.settle(2000)
    for _ = 1, 8 do test.key("Right") test.settle(100) end
    test.key("Return")
    test.settle(1500)
    test.key("space")
    test.settle(300)
    test.key("Escape")
    test.settle(1000)
    test.key("Escape")
    test.settle(1500)
    test.eq(test.ipc("games"), "games", "the arcade stayed open")
  end)

  test.it("keyboard layouts are picked from the xkb list, searched, and never all let go", function()
    load()
    test.ipc("settings", "input")
    test.settle(2000)
    test.eq(test.ipc("get", "keyboardLayouts"), '"us"')
    test.click(test.get { text = "Edit", visible = true })
    test.settle(500)
    test.type("germ")
    test.settle(500)
    local german = test.get { id = "layout-row-de", visible = true }
    test.falsy(test.find { id = "layout-row-fr", visible = true }, "the search leaves French out")
    test.click(german)
    test.settle(500)
    test.eq(test.ipc("get", "keyboardLayouts"), '"us,de"')
    -- A chip lets its layout go; the last one stays.
    test.click(test.get { id = "layout-chip-us", visible = true })
    test.settle(500)
    test.eq(test.ipc("get", "keyboardLayouts"), '"de"')
    test.click(test.get { id = "layout-chip-de", visible = true })
    test.settle(500)
    test.eq(test.ipc("get", "keyboardLayouts"), '"de"')
  end)
end)
