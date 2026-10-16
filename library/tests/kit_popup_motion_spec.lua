-- lib.kit.popup's motion, from a theme's `popup_motion`: the default kit's
-- menu, bottom sheet, toast and dialog coming in from the pose they rest in
-- while shut and going back to it as ghosts, sampled on the virtual clock --
-- opened, closed, turned back mid-flight and taken back while leaving --
-- with motion reduced, and a theme of the configuration's own whose targets
-- follow a signal.
--
--     morf test --no-dbus library/tests/kit_popup_motion_spec.lua

local test = morf.test

-- The verbs both configurations answer: a popup's pose as its node shows
-- it, the log, opening and closing.
local POSE = [[
  morf.ipc.pose = function(name)
    local n = popups[name].node
    return { s = n.scale, sx = n.scale_x, tx = n.translate_x, ty = n.translate_y, op = n.opacity,
      ox = n.transform_origin_x, oy = n.transform_origin_y }
  end
  morf.ipc.log = function() local s = table.concat(log, ","); log = {}; return s end
  morf.ipc.open = function(name) popups[name].open(name == "menu" and opener or nil) end
  morf.ipc.close = function(name) popups[name].close("closed") end
  morf.ipc.is_open = function(name) return popups[name].is_open() end
]]

local DEFAULT = [[
  local ui = require("morf.ui")
  local reduced = morf.env("POPUP_REDUCED") == "1"
  package.loaded.kit = require("lib.kit.skins.default").make { variant = "light", reduced_motion = reduced }
  local w = require("lib.kit.widgets")
  morf.surface.width, morf.surface.height = 800, 600
  local log, opener, popups = {}, nil, nil
  local function note(s) log[#log + 1] = s end
  local under = ui.MouseArea { id = "under", x = 0, y = 50, width = 800, height = 550,
    on_clicked = function() note("under") end }
  local root = ui.Item { width = 800, height = 600, under }
  local menu = w.menu { id = "menu", width = 160,
    items = { { label = "Copy", on_clicked = function() note("copy") end },
              { label = "Paste", on_clicked = function() note("paste") end } },
    on_opened = function() note("opened") end,
    on_closed = function(reason) note("closed:" .. reason) end }
  local sheet = w.bottom_sheet { id = "sheet", root = root, width = 400, height = 200, text = "Sheet" }
  local toast = w.toast { id = "toast", root = root, width = 200, height = 40, text = "Saved" }
  local dialog = w.dialog { id = "dialog", root = root, width = 320, height = 160, title = "Discard?",
    buttons = { { label = "Cancel" }, { label = "Discard" } },
    on_closed = function(reason) note("dialog:" .. reason) end }
  opener = w.push { id = "opener", x = 20, y = 20, width = 100, height = 30, label = "Menu",
    on_clicked = function() menu.toggle(opener) end }
  ui.reparent(opener, root)
  popups = { menu = menu, sheet = sheet, toast = toast, dialog = dialog }
]] .. POSE

-- A theme with only `popup_motion`: linear tweens, a 100 ms linger, and an
-- open pose that follows a signal.
local OWN = [[
  local ui = require("morf.ui")
  local skin = require("lib.kit.skin")
  skin.define("plain", { skins = {
    Popup = function() return { background = ui.Rect { anchors = { fill = true }, color = "#202020" } } end,
  } })
  skin.use("plain")
  local offset = morf.signal("own.offset", 0)
  local linear = { duration = 100, easing = "linear" }
  package.loaded.kit = { popup_motion = function(widget, spec, state)
    return { linger = 100, behavior = { translate_x = linear, opacity = linear },
      transform_origin_x = 0,
      translate_x = function() if state.open() then return offset:get() end return -40 end,
      opacity = function() return state.open() and 1 or 0 end }
  end }
  local popup = require("lib.kit.popup")
  local root = ui.Item { width = 400, height = 300 }
  local log, opener = {}, nil
  local popups = {
    own = popup.make("popover", { id = "own", root = root, width = 100, height = 50 }),
    still = popup.make("popover", { id = "still", root = root, width = 100, height = 50, behavior = {} }),
  }
  morf.ipc.offset = function(v) offset:set(v) end
]] .. POSE

local function load(source, env)
  test.load { source = source, size = { 800, 600 }, env = env }
  test.settle(100)
end

local function focused()
  for _, node in ipairs(test.nodes()) do if node.focused then return node.id end end
  return nil
end

--- Checks the popup's pose fields (and its box, `{ x, y, width, height }`).
local function expect(name, want, label)
  local pose = test.ipc("pose", name)
  for key, value in pairs(want) do
    if key == "box" then
      local node = test.get(name)
      test.near(node.x, value[1], 1e-3, label .. ": x")
      test.near(node.y, value[2], 1e-3, label .. ": y")
      test.near(node.width, value[3], 1e-3, label .. ": width")
      test.near(node.height, value[4], 1e-3, label .. ": height")
    else
      test.near(pose[key], value, 1e-4, ("%s: %s = %s"):format(label, key, tostring(pose[key])))
    end
  end
end

--- Advances to each time in `list` (ms from now) and checks the pose there.
local function samples(name, list)
  local now = 0
  for _, sample in ipairs(list) do
    test.advance(sample[1] - now)
    now = sample[1]
    expect(name, sample[2], ("%s at %d ms"):format(name, now))
  end
end

local function shown(name) return test.find { id = name, visible = true } ~= nil end

test.it("the default kit's menu grows out of its button and comes to rest", function()
  load(DEFAULT)
  test.falsy(test.find("menu"), "built before it was first opened")
  test.ipc("open", "menu")
  samples("menu", {
    { 0, { s = 0.94, sx = 1, tx = 0, ty = -6, op = 0, ox = 0, oy = 0, box = { 20, 48, 150.4, 67.68 } } },
    { 16, { s = 0.94523, ty = -5.47682, op = 0.31815 } },
    { 48, { s = 0.96201, ty = -3.79941, op = 0.66948 } },
    { 96, { s = 0.98182, ty = -1.81813, op = 0.92217, box = { 20, 52.1819, 157.0910, 70.6909 } } },
  })
  test.settle()
  expect("menu", { s = 1, sx = 1, tx = 0, ty = 0, op = 1, ox = 0, oy = 0, box = { 20, 54, 160, 72 } }, "at rest")
  test.eq(test.ipc("log"), "opened")
end)

test.it("closed, it gives focus back at once and leaves as a ghost that takes no input", function()
  load(DEFAULT)
  test.key("Tab") test.settle(20)
  test.eq(focused(), "opener")
  test.key("Return") test.settle()
  test.eq(focused(), "menu-item-1")
  test.key("Escape")
  test.falsy(test.ipc("is_open", "menu"))
  test.eq(focused(), "opener")
  test.eq(test.ipc("log"), "opened,closed:escape")
  test.truthy(shown("menu"), "gone before its exit played")
  samples("menu", {
    { 0, { s = 1, ty = 0, op = 1 } },
    { 16, { s = 0.99477, ty = -0.52318, op = 0.68185 } },
    { 48, { s = 0.97799, ty = -2.20059, op = 0.33052 } },
    { 96, { s = 0.95818, ty = -4.18187, op = 0.07783 } },
  })
  -- A press on the ghost goes to what is under it.
  test.click(60, 80)
  test.eq(test.ipc("log"), "under")
  test.eq(focused(), "opener")
  test.truthy(shown("menu"))
  test.advance(124)
  test.falsy(shown("menu"), "still in the layer after its linger")
  test.falsy(test.ipc("is_open", "menu"))
end)

test.it("closed mid-flight, it turns back from where it is at the speed it had", function()
  load(DEFAULT)
  test.ipc("open", "menu")
  test.advance(48)
  test.ipc("close", "menu")
  samples("menu", {
    { 0, { s = 0.96201, ty = -3.79941, op = 0.66948 } },
    { 16, { s = 0.96457, ty = -3.54265, op = 0.74971 } },
    { 48, { s = 0.95981, ty = -4.01872, op = 0.68405 } },
    { 96, { s = 0.95020, ty = -4.97967, op = 0.29668 } },
  })
  test.advance(124)
  test.falsy(shown("menu"))
  test.eq(test.ipc("log"), "opened,closed:closed")
end)

test.it("opened again while it leaves, it is taken back from where it was", function()
  load(DEFAULT)
  test.ipc("open", "menu") test.settle()
  test.ipc("close", "menu")
  test.advance(64)
  expect("menu", { s = 0.97019, ty = -2.98054, op = 0.22002 }, "leaving")
  test.ipc("open", "menu")
  test.truthy(test.ipc("is_open", "menu"))
  test.eq(focused(), "menu-item-1")
  samples("menu", {
    { 0, { s = 0.97019, ty = -2.98054, op = 0.22002 } },
    { 16, { s = 0.96881, ty = -3.11927, op = 0.16871 } },
    { 48, { s = 0.97590, ty = -2.40964, op = 0.27633 } },
  })
  test.settle()
  expect("menu", { s = 1, ty = 0, op = 1, box = { 20, 54, 160, 72 } }, "at rest again")
  -- Live again: its items take presses.
  test.click("menu-item-2") test.settle(50)
  test.eq(test.ipc("log"), "opened,closed:closed,opened,paste,closed:closed")
end)

test.it("a bottom sheet rises by its own height and keeps its opacity", function()
  load(DEFAULT)
  test.ipc("open", "sheet")
  samples("sheet", {
    { 0, { s = 1, ty = 248, op = 1, ox = 0.5, oy = 0.5, box = { 200, 448, 400, 200 } } },
    { 48, { ty = 174.50682, op = 1 } },
  })
  test.settle()
  expect("sheet", { ty = 0, op = 1, box = { 200, 200, 400, 200 } }, "at rest")
  test.ipc("close", "sheet")
  samples("sheet", { { 48, { ty = 73.49318, op = 1 } } })
  test.advance(200)
  test.falsy(shown("sheet"))
end)

test.it("a toast stretches in from a narrow strip", function()
  load(DEFAULT)
  test.ipc("open", "toast")
  samples("toast", {
    { 0, { s = 0.9, sx = 0.55, ty = 14, op = 0, ox = 0.5, oy = 0.5 } },
    { 48, { s = 0.93633, sx = 0.71349, ty = 9.85119, op = 0.66948 } },
  })
  test.settle()
  expect("toast", { s = 1, sx = 1, ty = 0, op = 1, box = { 300, 280, 200, 40 } }, "at rest")
  test.ipc("close", "toast")
  samples("toast", { { 48, { s = 0.96367, sx = 0.83651, ty = 4.14881, op = 0.33052 } } })
end)

test.it("a dialog grows from its middle, modal while open and not as it leaves", function()
  load(DEFAULT)
  test.ipc("open", "dialog")
  test.eq(focused(), "dialog-button-1")
  samples("dialog", {
    { 0, { s = 0.92, ty = 0, op = 0, ox = 0.5, oy = 0.5, box = { 252.8, 226.4, 294.4, 147.2 } } },
    { 48, { s = 0.94371, op = 0.66948 } },
  })
  test.settle()
  expect("dialog", { s = 1, op = 1, box = { 240, 220, 320, 160 } }, "at rest")
  test.click(700, 580)
  test.eq(test.ipc("log"), "", "a press outside reached past a modal dialog")
  test.ipc("close", "dialog")
  samples("dialog", { { 48, { s = 0.97629, op = 0.33052 } } })
  test.click(700, 580)
  test.eq(test.ipc("log"), "dialog:closed,under")
  test.advance(200)
  test.falsy(shown("dialog"))
end)

test.it("with motion reduced a popup is at rest at once and gone at once", function()
  load(DEFAULT, { POPUP_REDUCED = "1" })
  test.ipc("open", "menu")
  test.advance(16)
  expect("menu", { s = 1, ty = 0, op = 1, box = { 20, 54, 160, 72 } }, "opened")
  test.ipc("close", "menu")
  test.falsy(shown("menu"), "a ghost stayed with motion reduced")
  test.ipc("open", "menu")
  test.advance(16)
  expect("menu", { s = 1, ty = 0, op = 1 }, "opened again")
end)

test.it("a theme's own motion: its tweens, its linger, and targets that follow a signal", function()
  load(OWN)
  test.ipc("open", "own")
  samples("own", {
    { 0, { tx = -40, op = 0, ox = 0, s = 1, box = { 310, 8, 100, 50 } } },
    { 16, { tx = -33.6, op = 0.16 } },
    { 48, { tx = -20.8, op = 0.48 } },
    { 96, { tx = -1.6, op = 0.96 } },
    { 112, { tx = 0, op = 1 } },
  })
  -- The open pose moves with the signal it reads, by the same tween.
  test.ipc("offset", 20)
  samples("own", { { 0, { tx = 0 } }, { 48, { tx = 9.6 } }, { 112, { tx = 20, op = 1 } } })
  test.ipc("close", "own")
  samples("own", {
    { 16, { tx = 10.4, op = 0.84 } },
    { 48, { tx = -8.8, op = 0.52 } },
    { 96, { tx = -37.6, op = 0.04 } },
  })
  test.truthy(shown("own"))
  test.advance(16)
  expect("own", { tx = -40, op = 0 }, "shut")
  test.falsy(shown("own"), "still in the layer after its linger")
  -- While shut the signal moves nothing.
  test.ipc("offset", -10)
  test.advance(48)
  expect("own", { tx = -40, op = 0 }, "shut, the signal changed")
  test.ipc("open", "own")
  samples("own", { { 0, { tx = -40 } }, { 48, { tx = -25.6, op = 0.48 } }, { 112, { tx = -10, op = 1 } } })
end)

test.it("a popup given its own behaviour leaves the theme's motion out", function()
  load(OWN)
  test.ipc("open", "still")
  expect("still", { tx = 0, op = 1, ox = 0.5, oy = 0.5, box = { 350, 8, 100, 50 } }, "opened")
  test.ipc("close", "still")
  test.falsy(shown("still"))
  test.eq(test.logs("error"), {})
end)
