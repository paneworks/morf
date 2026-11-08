-- The entries of lib.kit.archetypes.selection and .collection are chosen by
-- a click or a tap, never by the press: a mouse drag or a finger's pan that
-- starts on one chooses nothing, while the press still shows it down; the
-- keys and a reorder drag still choose.
--
--     morf test library/tests/kit_entry_click_spec.lua

local test = morf.test

local HOST = [[
  local ui = require("morf.ui")
  local skin = require("lib.kit.skin")
  local w = require("lib.kit.widgets")
  local selection = require("lib.kit.archetypes.selection")
  skin.define("plain", { skins = {
    Selection = function()
      return { item = function(_, value, s)
        return ui.Text { id = "item-" .. tostring(value),
          text = function() return tostring(value) .. (s.down() and " down" or "") end }
      end }
    end,
    Collection = { row = function() return function(row, s)
      local text = ui.Text { text = tostring(row.label) }
      return text, function(next_row) text.text = tostring(next_row.label) end
    end end },
  } })
  skin.use("plain")
  local log = {}
  local function note(text) log[#log + 1] = text end
  local tab = morf.signal("test.tab", 1)
  ui.MouseArea { width = 600, height = 400,
    on_panned = function(phase) if phase == "begin" then note("pan") end return true end,
    w.view_switcher { id = "tabs", x = 10, y = 10, items = { "One", "Two", "Three" }, item_width = 60, item_height = 30,
      current = function() return tab:get() end,
      on_current_changed = function(i) tab:set(i) note("tab:" .. i) end },
    (selection.make("swatch_grid", { id = "swatches", x = 10, y = 60, columns = 3, item_width = 40, item_height = 40,
      items = { "a", "b", "c" }, press_activates = true,
      on_current_changed = function(i) note("swatch:" .. i) end,
      on_activated = function(i) note("pick:" .. i) end })),
    (selection.make("view_switcher", { id = "strip", x = 10, y = 120, items = { "P", "Q", "R" }, item_width = 60,
      item_height = 30, reorderable = true, current = 1,
      on_current_changed = function(i) note("strip:" .. i) end,
      on_reorder = function(i, step) note("move:" .. i .. ":" .. step) end })),
    (w.list_view { id = "rows", x = 300, y = 10, width = 200, height = 150, row_height = 30,
      rows = { { key = "a", label = "Alpha" }, { key = "b", label = "Beta" }, { key = "c", label = "Gamma" } },
      on_current_changed = function(i) note("row:" .. i) end }),
  }
  morf.ipc.log = function() local s = table.concat(log, ","); log = {}; return s end
]]

local function load() test.load { source = HOST, size = { 600, 400 } } test.settle(200) end

local function centre(query)
  local node = test.get(query)
  return node.x + node.width / 2, node.y + node.height / 2
end

local function finger_swipe(x, y, dx, dy)
  test.touch("down", 0, x, y) test.advance(16)
  for i = 1, 8 do test.touch("move", 0, x + dx * i, y + dy * i) test.advance(16) end
  test.touch("up", 0, x + dx * 8, y + dy * 8) test.advance(300)
end

test.it("a tab is chosen when the click ends, not when it starts", function()
  load()
  local x, y = centre("item-Two")
  test.move(x, y) test.press(x, y) test.settle(30)
  test.eq(test.ipc("log"), "", "the press alone chose the tab")
  test.eq(test.get("item-Two").text, "Two down", "the press does not show the tab down")
  test.release(x, y) test.settle(30)
  test.eq(test.ipc("log"), "tab:2")
end)

test.it("a mouse drag that starts on a tab chooses nothing", function()
  load()
  local x, y = centre("item-Three")
  test.drag({ x, y }, { x, y + 200 }, { steps = 10 }) test.settle(30)
  test.eq(test.ipc("log"), "")
end)

test.it("a finger's pan that starts on an entry chooses and activates nothing", function()
  load()
  local x, y = centre("item-Two")
  finger_swipe(x, y, 0, 25)
  test.eq(test.ipc("log"), "pan")
  x, y = centre("item-b")
  finger_swipe(x, y, 0, 25)
  test.eq(test.ipc("log"), "pan")
  x, y = centre { text = "Beta" }
  finger_swipe(x, y, 0, 25)
  test.eq(test.ipc("log"), "pan")
end)

test.it("a tap still chooses, and a tap on a press_activates entry picks it", function()
  load()
  local x, y = centre("item-Three")
  test.touch("down", 0, x, y) test.advance(40) test.touch("up", 0, x, y) test.advance(200)
  test.eq(test.ipc("log"), "tab:3")
  x, y = centre("item-c")
  test.touch("down", 0, x, y) test.advance(40) test.touch("up", 0, x, y) test.advance(200)
  test.eq(test.ipc("log"), "swatch:3,pick:3")
end)

test.it("a row is chosen by its click, not by its press", function()
  load()
  local x, y = centre { text = "Gamma" }
  test.move(x, y) test.press(x, y) test.settle(30)
  test.eq(test.ipc("log"), "")
  test.release(x, y) test.settle(30)
  test.eq(test.ipc("log"), "row:3")
  x, y = centre { text = "Beta" }
  test.drag({ x, y }, { x + 150, y }, { steps = 10 }) test.settle(30)
  test.eq(test.ipc("log"), "")
end)

test.it("a reorder drag chooses the entry it carries, and the keys still choose", function()
  load()
  local x, y = centre("item-Q")
  test.drag({ x, y }, { x + 90, y }, { steps = 12 }) test.settle(30)
  test.eq(test.ipc("log"), "strip:2,move:2:1")
  for _ = 1, 12 do
    local focused = false
    for _, node in ipairs(test.nodes()) do if node.focused and node.id == "tabs" then focused = true end end
    if focused then break end
    test.key("Tab") test.settle(10)
  end
  test.key("Right") test.settle(20)
  test.eq(test.ipc("log"), "tab:2")
end)
