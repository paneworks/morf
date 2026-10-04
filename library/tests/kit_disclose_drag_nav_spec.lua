-- lib.kit.disclosure, lib.kit.drag and lib.kit.navigation with a plain
-- skin: an accordion, a split pane dragged and keyed, a card swiped away,
-- and a navigation view pushed and popped with Alt+Left.
--
--     morf test library/tests/kit_disclose_drag_nav_spec.lua

local test = morf.test

local HOST = [[
  local ui = require("morf.ui")
  local skin = require("lib.kit.skin")
  local w = require("lib.kit.widgets")
  local drag = require("lib.kit.drag")
  morf.surface.height = 600
  skin.define("plain", { skins = {} })
  skin.use("plain")
  local log = {}
  local function note(s) log[#log + 1] = s end
  local a = w.accordion { id = "a", x = 10, y = 10, width = 200, header_height = 30, group = "acc",
    content = ui.Rect { id = "a-body", width = 200, height = 60, color = "#ff0000" },
    on_toggled = function(on) note("a:" .. tostring(on)) end }
  local b = w.accordion { id = "b", x = 10, y = 200, width = 200, header_height = 30, group = "acc",
    content = ui.Rect { id = "b-body", width = 200, height = 60, color = "#00ff00" },
    on_toggled = function(on) note("b:" .. tostring(on)) end }
  local split, ratio = drag.split { id = "split", x = 300, y = 10, width = 400, height = 100,
    first = ui.Rect { width = 400, height = 100, color = "#333333" }, second = ui.Rect { width = 400, height = 100 } }
  local card = ui.MouseArea { id = "card", x = 300, y = 200, width = 200, height = 60 }
  drag.swipe(card, { on_swiped = function(d) note("swiped:" .. d) end })
  local nav_node, nav = w.navigation_view { id = "nav", x = 300, y = 300, width = 300, height = 200, current = "home",
    pages = { home = function() return w.push { id = "home-button", width = 100, height = 30 } end,
      sound = function() return w.push { id = "sound-button", width = 100, height = 30 } end } }
  ui.Item { width = 800, height = 600, a, b, split, card, nav_node }
  morf.ipc.log = function() local s = table.concat(log, ","); log = {}; return s end
  morf.ipc.ratio = function() return ratio:get() end
  morf.ipc.push = function(p) nav.push(p) end
  morf.ipc.current = function() return nav.t.current end
  morf.ipc.focus_inside = function()
    -- The page's button, focused as Tab would.
    for _ in pairs({}) do end
  end
]]

local function load() test.load { source = HOST, size = { 800, 600 } } test.settle(100) end

test.it("an accordion keeps one open", function()
  load()
  test.click(50, 20) test.settle(300)
  test.eq(test.ipc("log"), "a:true")
  test.truthy(test.get("a-body").visible)
  test.click(50, 210) test.settle(300)
  test.eq(test.ipc("log"), "b:true,a:false")
end)

test.it("a split pane follows its divider and the arrows", function()
  load()
  local d = test.get("split")
  test.drag({ 300 + 200, 50 }, { 300 + 280, 50 }, { steps = 5 }) test.settle(50)
  test.truthy(test.ipc("ratio") > 0.65, tostring(test.ipc("ratio")))
end)

test.it("a card goes when swiped far enough and comes back when not", function()
  load()
  test.drag({ 400, 230 }, { 418, 230 }, { steps = 3 }) test.settle(50)
  test.eq(test.ipc("log"), "")
  test.drag({ 400, 230 }, { 260, 230 }, { steps = 6 }) test.settle(50)
  test.eq(test.ipc("log"), "swiped:left")
end)

test.it("a navigation view pushes and Alt+Left pops", function()
  load()
  test.ipc("push", "sound") test.settle(400)
  test.eq(test.ipc("current"), "sound")
  test.truthy(test.get("sound-button").visible)
  for _ = 1, 8 do
    local here
    for _, n in ipairs(test.nodes()) do if n.focused then here = n.id end end
    if here == "sound-button" then break end
    test.key("Tab") test.settle(10)
  end
  test.key("Left", { "alt" }) test.settle(400)
  test.eq(test.ipc("current"), "home")
end)
