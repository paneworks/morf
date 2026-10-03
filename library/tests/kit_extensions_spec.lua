-- The widgets built on behaviour the archetypes gained late, with a plain
-- skin: a hold button that counts only once held, slide to confirm, a
-- radial menu picked by direction and by a flick, a pie menu opened round
-- the pointer, a tumbler that snaps and goes round, and a shortcut recorder.
--
--     morf test --no-dbus library/tests/kit_extensions_spec.lua

local test = morf.test

local HOST = [[
  local ui = require("morf.ui")
  local skin = require("lib.kit.skin")
  local w = require("lib.kit.widgets")
  morf.surface.width, morf.surface.height = 900, 600
  skin.define("plain", { skins = {
    Selection = function(t)
      return { item = function(index, value) return ui.Text { id = "entry-" .. tostring(value), text = tostring(value) } end }
    end,
  } })
  skin.use("plain")
  local log = {}
  local function note(s) log[#log + 1] = s end
  local hold = w.hold_button { id = "hold", x = 10, y = 10, width = 160, height = 40, label = "Delete",
    on_clicked = function() note("clicked") end, on_hold_canceled = function() note("canceled") end }
  local slide, slide_t = w.slide_to_confirm { id = "slide", x = 10, y = 80, width = 280, height = 52,
    on_confirmed = function() note("confirmed") end }
  local radial = w.radial_menu { id = "radial", x = 340, y = 10, size = 220,
    items = { "Copy", "Paste", "Cut", "Delete" },
    on_current_changed = function(i) note("current:" .. i) end,
    on_activated = function(i) note("activated:" .. i) end }
  local hours = {}
  for i = 1, 12 do hours[i] = tostring(i) end
  local hour = 1
  local tumbler = w.tumbler { id = "tumbler", x = 600, y = 10, items = hours, current = 1,
    on_current_changed = function(i) hour = i end }
  local recorder, _, recorder_t = w.shortcut_recorder { id = "recorder", x = 10, y = 200, width = 260, height = 44,
    text = "ctrl+q", on_captured = function(chord) note("captured:" .. chord) end,
    conflicts = function(chord) return chord == "ctrl+c" end }
  local target = ui.MouseArea { id = "pie-target", x = 300, y = 300, width = 300, height = 280 }
  local pie, pie_menu = w.pie_menu { id = "pie", size = 200, items = { "Play", "Next", "Queue", "Back" },
    on_activated = function(i) note("pie:" .. i) end }
  pie_menu.attach(target)
  ui.Item { width = 900, height = 600, hold, slide, radial, tumbler, recorder, target }
  morf.ipc.pie_open = function() return pie_menu.is_open() end
  morf.ipc.open_pie = function() pie_menu.open_at(450, 440) end
  morf.ipc.log = function() local s = table.concat(log, ","); log = {}; return s end
  morf.ipc.slide = function() return slide_t.value end
  morf.ipc.hour = function() return hour end
  morf.ipc.chord = function() return recorder_t.text end
  morf.ipc.conflict = function() return recorder_t.conflict end
]]

local function load() test.load { source = HOST, size = { 900, 600 } } test.settle(200) end
local function wait(ms) for _ = 1, math.ceil(ms / 16) do test.advance(16) end end

test.it("a hold button fires only after its hold and cancels when let go early", function()
  load()
  test.press("hold") wait(400)
  test.release("hold") test.settle(50)
  test.eq(test.ipc("log"), "canceled")
  test.press("hold") wait(900)
  test.eq(test.ipc("log"), "clicked")
  test.release("hold") test.settle(50)
  test.eq(test.ipc("log"), "")
  -- From the keyboard it acts at once.
  test.key("Tab") test.settle(20)
  test.key("Return") test.settle(20)
  test.eq(test.ipc("log"), "clicked")
end)

test.it("slide to confirm confirms only slid all the way and springs back", function()
  load()
  test.drag({ 36, 106 }, { 180, 106 }) test.settle(100)
  test.eq(test.ipc("log"), "")
  test.eq(test.ipc("slide"), 0)
  test.drag({ 36, 106 }, { 320, 106 }) test.settle(100)
  test.eq(test.ipc("log"), "confirmed")
  test.eq(test.ipc("slide"), 0)
end)

test.it("a radial menu picks by direction and a flick activates", function()
  load()
  local cx, cy = 340 + 110, 10 + 110
  test.move(cx, cy - 70) test.settle(20)
  test.eq(test.ipc("log"), "current:1")
  test.move(cx + 70, cy) test.settle(20)
  test.eq(test.ipc("log"), "current:2")
  -- Inside the hub nothing is pointed at.
  test.move(cx + 3, cy + 2) test.settle(20)
  test.eq(test.ipc("log"), "")
  test.drag({ cx, cy }, { cx, cy + 90 }) test.settle(20)
  test.eq(test.ipc("log"), "current:3,activated:3")
  -- A release in the hub picks nothing.
  test.drag({ cx, cy }, { cx - 4, cy }) test.settle(20)
  test.eq(test.ipc("log"), "")
end)

test.it("a tumbler's drag snaps to an entry and goes round", function()
  load()
  local x, y = 600 + 36, 10 + 90
  test.drag({ x, y }, { x, y - 72 }, { steps = 12 }) test.settle(800)
  test.eq(test.ipc("hour"), 3)
  test.drag({ x, y }, { x, y + 4 * 36 + 10 }, { steps = 12 }) test.settle(800)
  test.eq(test.ipc("hour"), 11)
  -- A press on the row under the centre turns to it; the wheel steps.
  test.click(x, y + 36) test.settle(800)
  test.eq(test.ipc("hour"), 12)
  test.move(x, y)
  test.wheel(0, 1, { x = x, y = y }) test.settle(800)
  test.eq(test.ipc("hour"), 1)
end)

test.it("a shortcut recorder captures a chord, Escape gives it back and BackSpace clears", function()
  load()
  test.click("recorder") test.settle(20)
  test.key("Control_L", "ctrl") test.settle(20)
  test.eq(test.ipc("log"), "")
  test.key("k", "ctrl+shift") test.settle(20)
  test.eq(test.ipc("log"), "captured:ctrl+shift+k")
  test.eq(test.ipc("chord"), "ctrl+shift+k")
  test.key("Escape") test.settle(20)
  test.eq(test.ipc("log"), "")
  test.eq(test.ipc("chord"), "ctrl+shift+k")
  test.key("c", { "ctrl" }) test.settle(20)
  test.eq(test.ipc("conflict"), "In use")
  test.ipc("log")
  test.key("BackSpace") test.settle(20)
  test.eq(test.ipc("log"), "captured:")
  test.eq(test.ipc("chord"), "")
  test.eq(test.ipc("conflict"), "")
end)

test.it("a pie menu opens round a right press and a flick picks from it", function()
  load()
  test.drag({ 450, 440 }, { 450 + 80, 440 }, { button = "right" }) test.settle(100)
  test.eq(test.ipc("log"), "pie:2")
  test.eq(test.ipc("pie_open"), false)
  -- Opened at a point and let go in the hub it stays open; a click picks.
  test.ipc("open_pie") test.settle(100)
  test.eq(test.ipc("pie_open"), true)
  test.click(450, 440 + 70) test.settle(100)
  test.eq(test.ipc("log"), "pie:3")
  test.eq(test.ipc("pie_open"), false)
end)

test.it("a shortcut recorder lets Tab alone go on", function()
  load()
  test.click("recorder") test.settle(20)
  test.key("Tab") test.settle(20)
  test.eq(test.ipc("log"), "")
  test.eq(test.ipc("chord"), "ctrl+q")
  local focused
  for _, node in ipairs(test.nodes()) do if node.focused then focused = node.id end end
  test.ne(focused, "recorder")
end)
