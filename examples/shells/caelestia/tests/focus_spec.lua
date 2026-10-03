-- Focus through the kit: Tab walks the buttons a theme made focusable,
-- Shift+Tab walks back, Return clicks the one with focus, and the theme's
-- ring shows for the keyboard only -- never for a click.
--
--     morf test --no-dbus examples/shells/caelestia/tests/focus_spec.lua

local test = morf.test

local HOST = [[
  local ui, kit = require("morf.ui"), require("kit")
  local clicks = {}
  local function button(name, x)
    return kit.action { id = "focus-" .. name, x = x, y = 20, width = 80, height = 36, cursor = "pointer",
      on_clicked = function() clicks[#clicks + 1] = name end,
      kit.label { text = name, anchors = { center_in = true } } }
  end
  local field = ui.TextInput { id = "focus-field", x = 20, y = 80, width = 200, height = 30 }
  ui.Item { width = 400, height = 200,
    button("one", 20), button("two", 120), button("three", 220), field }
  morf.ipc.clicks = function() return table.concat(clicks, ",") end
]]

local function load(style)
  test.load("../shell/init.lua", { size = { 400, 200 }, env = { CAELESTIA_STYLE = style }, source = HOST })
  test.settle(500)
end

local function focused()
  for _, node in ipairs(test.nodes()) do
    if node.focused then return node.id ~= "" and node.id or node.element, node end
  end
end

for _, style in ipairs { "material", "tsugumori" } do
  test.it(style .. ": Tab walks the buttons, Shift+Tab walks back", function()
    load(style)
    test.key("Tab") test.advance(50)
    test.eq(focused(), "focus-one")
    test.key("Tab") test.advance(50)
    test.eq(focused(), "focus-two")
    test.truthy(test.get("focus-two").visual_focus)
    test.key("Tab") test.key("Tab") test.advance(50)
    test.eq(focused(), "focus-field")
    test.key("Tab", { "shift" }) test.advance(50)
    test.eq(focused(), "focus-three")
    test.eq(#test.logs("error"), 0)
  end)

  test.it(style .. ": Return clicks the focused button and the ring is the keyboard's", function()
    load(style)
    local plain = 0
    for _, node in ipairs(test.nodes()) do if node.visible then plain = plain + 1 end end
    test.key("Tab") test.advance(50)
    test.key("Return") test.advance(50)
    test.eq(test.ipc("clicks"), "one")
    local ringed = 0
    for _, node in ipairs(test.nodes()) do if node.visible then ringed = ringed + 1 end end
    test.truthy(ringed > plain, "a ring shows on keyboard focus")
    -- A click on a button clicks it and leaves no ring anywhere.
    test.click("focus-three") test.advance(50)
    test.eq(test.ipc("clicks"), "one,three")
    -- The field takes focus by click, and the ring goes with the keyboard.
    test.click("focus-field") test.advance(50)
    test.eq(focused(), "focus-field")
    for _, node in ipairs(test.nodes()) do test.falsy(node.visual_focus, "no ring after a click") end
  end)
end
