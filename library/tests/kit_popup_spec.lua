-- lib.kit.popup on the Popup archetype and the overlay layer, with a plain
-- skin: a menu beside its button, a modal dialog, a toast that times out,
-- and a panel tracked where it is.
--
--     morf test library/tests/kit_popup_spec.lua

local test = morf.test

local HOST = [[
  local ui = require("morf.ui")
  local skin = require("lib.kit.skin")
  local w = require("lib.kit.widgets")
  local popup = require("lib.kit.popup")
  morf.surface.height = 600
  skin.define("plain", { skins = {
    Popup = function() return { background = ui.Rect { anchors = { fill = true }, color = "#202020" } } end,
    Press = function(t, spec) return { background = ui.Rect { anchors = { fill = true }, color = "#404040" },
      label = ui.Text { x = 8, text = tostring(spec.label or ""), color = "#ffffff" } } end,
  } })
  skin.use("plain")
  local log = {}
  local function note(s) log[#log + 1] = s end
  local open_panel = morf.signal("test.panel", false)
  local menu = w.menu { id = "menu", width = 160,
    items = { { label = "Copy", on_clicked = function() note("copy") end },
              { label = "Paste", on_clicked = function() note("paste") end } },
    on_closed = function(reason) note("menu-closed:" .. reason) end }
  local dialog = w.dialog { id = "dialog", width = 320, title = "Discard?",
    buttons = { { label = "Cancel" }, { label = "Discard", on_clicked = function() note("discard") end } },
    on_closed = function(reason) note("dialog-closed:" .. reason) end }
  local opener
  opener = w.push { id = "opener", x = 20, y = 20, width = 100, height = 30, label = "Menu",
    on_clicked = function() menu.toggle(opener) end }
  local panel = ui.Item { id = "panel", x = 400, y = 100, width = 200, height = 200,
    visible = function() return open_panel:get() end,
    ui.MouseArea { anchors = { fill = true } } }
  local root = ui.Item { width = 800, height = 600, opener, panel }
  popup.track(panel, { open = function() return open_panel:get() end,
    on_close = function(reason) note("panel:" .. reason) open_panel:set(false) end })
  morf.ipc.log = function() local s = table.concat(log, ","); log = {}; return s end
  morf.ipc.dialog = function() dialog.open(opener) end
  morf.ipc.menu_open = function() return menu.is_open() end
  morf.ipc.toast = function() popup.toast { id = "toast", text = "Copied", timeout = 1000, anchor = opener } end
  morf.ipc.panel = function() open_panel:set(true) end
  morf.ipc.panel_open = function() return open_panel:get() end
]]

local function load() test.load { source = HOST, size = { 800, 600 } } test.settle(100) end

local function focused()
  for _, node in ipairs(test.nodes()) do if node.focused then return node.id end end
end

test.it("a menu opens beside its button, runs an item and closes", function()
  load()
  test.click("opener") test.settle(50)
  test.truthy(test.ipc("menu_open"))
  local menu, opener = test.get("menu-item-1"), test.get("opener")
  test.truthy(menu.y >= opener.y + opener.height, "the menu is not below its button")
  test.click("menu-item-2") test.settle(50)
  test.eq(test.ipc("log"), "paste,menu-closed:closed")
  test.falsy(test.ipc("menu_open"))
end)

test.it("Escape closes the menu and focus goes back to its button", function()
  load()
  test.key("Tab") test.settle(20)
  test.eq(focused(), "opener")
  test.key("Return") test.settle(50)
  test.truthy(test.ipc("menu_open"))
  test.eq(focused(), "menu-item-1")
  test.key("Escape") test.settle(50)
  test.falsy(test.ipc("menu_open"))
  test.eq(test.ipc("log"), "menu-closed:escape")
  test.eq(focused(), "opener")
end)

test.it("a dialog is modal and its buttons close it", function()
  load()
  test.ipc("dialog") test.settle(50)
  -- Tab stays inside it.
  for _ = 1, 4 do test.key("Tab") test.settle(10) end
  test.truthy(focused() == "dialog-button-1" or focused() == "dialog-button-2", tostring(focused()))
  -- A press outside does nothing to a dialog.
  test.click(700, 550) test.settle(50)
  test.eq(test.ipc("log"), "")
  test.click("dialog-button-2") test.settle(50)
  test.eq(test.ipc("log"), "discard,dialog-closed:closed")
end)

test.it("a toast goes by itself", function()
  load()
  test.ipc("toast") test.settle(50)
  test.truthy(test.find { text = "Copied", visible = true })
  test.advance(1200) test.settle(50)
  test.falsy(test.find { text = "Copied", visible = true })
end)

test.it("a tracked panel shuts on a press outside it and on Escape", function()
  load()
  test.ipc("panel") test.settle(50)
  test.click(450, 150) test.settle(30)
  test.truthy(test.ipc("panel_open"), "a press inside shut it")
  test.click(100, 500) test.settle(30)
  test.falsy(test.ipc("panel_open"))
  test.eq(test.ipc("log"), "panel:outside")
  test.ipc("panel") test.settle(50)
  test.key("Escape") test.settle(30)
  test.falsy(test.ipc("panel_open"))
  test.eq(#test.logs("error"), 0)
end)
