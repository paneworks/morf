-- The example Settings application (examples/apps/settings/app.lua): it
-- opens, its pages change from the sidebar by pointer and keyboard, its
-- rows change settings, its dialogs open and close, it adapts from desktop
-- to phone width, and a screen reader can name everything.
--
--     morf test --no-dbus examples/apps/settings/tests/settings_app_spec.lua

local test = morf.test

local function load(size)
  test.load("../app.lua", { size = size or { 1600, 1000 } })
  test.settle(1500)
end

test.it("opens on Appearance with a header, sidebar and page", function()
  load()
  test.truthy(test.find { id = "settings-header", visible = true }, "no header bar")
  test.truthy(test.find { id = "settings-sidebar", visible = true }, "no sidebar")
  test.eq(test.ipc("settings-state").page, "appearance")
  test.eq(#test.logs("error"), 0)
end)

test.it("changes page from the sidebar by pointer and by keyboard", function()
  load()
  test.click("settings-sidebar-item-network") test.settle(800)
  test.eq(test.ipc("settings-state").page, "network")
  test.truthy(test.find { id = "wifi", visible = true }, "the network page did not show")
  test.key("Down") test.settle(400)
  test.key("Return") test.settle(800)
  test.eq(test.ipc("settings-state").page, "power")
end)

test.it("changes settings from its rows", function()
  load()
  test.click("dark") test.settle(400)
  test.truthy(test.ipc("settings-state").dark, "the dark switch did not toggle")
  test.click("settings-sidebar-item-network") test.settle(800)
  test.click("wifi") test.settle(400)
  test.falsy(test.ipc("settings-state").wifi, "the Wi-Fi row did not toggle")
end)

test.it("opens and dismisses the reset dialog and the about dialog", function()
  load()
  test.click("settings-sidebar-item-power") test.settle(800)
  test.click("reset") test.settle(600)
  test.truthy(test.find { id = "settings-reset-confirm", visible = true }, "the reset dialog did not open")
  test.key("Escape") test.settle(600)
  test.falsy(test.find { id = "settings-reset-confirm", visible = true })
  test.click("settings-sidebar-item-about") test.settle(800)
  test.click("about-app") test.settle(600)
  test.truthy(test.find { id = "settings-about-close", visible = true }, "the about dialog did not open")
end)

test.it("folds the sidebar into a drawer at phone width", function()
  load()
  test.falsy(test.ipc("settings-collapsed"), "the sidebar starts folded on a desktop")
  test.resize_window("Settings", 380, 760) test.settle(1200)
  test.truthy(test.ipc("settings-collapsed"), "the sidebar did not fold at phone width")
  test.truthy(test.find { id = "settings-show-sections", visible = true }, "no button to open the drawer")
  test.click("settings-show-sections") test.settle(800)
  local item = test.get("settings-sidebar-item-sound")
  test.truthy(item.x >= -1, "the drawer did not open")
  test.click("settings-sidebar-item-sound") test.settle(900)
  test.eq(test.ipc("settings-state").page, "sound")
  test.resize_window("Settings", 1100, 700) test.settle(1200)
  test.falsy(test.ipc("settings-collapsed"), "the sidebar did not come back when wide")
  test.eq(#test.logs("error"), 0)
end)

test.it("names every control for a screen reader", function()
  load()
  local unnamed = {}
  for _, row in ipairs(test.accessible()) do
    if row.name == "" and row.focusable then unnamed[#unnamed + 1] = row.role .. " " .. row.id end
  end
  test.eq(#unnamed, 0, table.concat(unnamed, "\n"))
end)
