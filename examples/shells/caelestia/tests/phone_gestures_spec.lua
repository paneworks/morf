local test = morf.test
local SOURCE = [[
  local steps = {}
  require("services").workspace.step = function(delta) steps[#steps + 1] = delta end
  require("init")
  morf.ipc.swipe_test = function(edge) require("phone_gestures").swipe(edge) end
  morf.ipc.gesture_state = function()
    local sidebar = require("sidebar")
    return { dashboard = require("dashboard").drawer.open:get(),
      dashboard_tab = require("dashboard").tab:get(), sidebar_tab = sidebar.tab:get(),
      sidebar = sidebar.drawer.open:get(), notifications = sidebar.showing("notifications"),
      settings = sidebar.showing("settings"), steps = steps }
  end
]]
local function load(style, width, height)
  test.stub_run("task", { code = 0, stdout = "[]" })
  test.load("../shell/init.lua", { source = SOURCE, size = { width or 1116, height or 2484 },
    env = { CAELESTIA_STYLE = style, CAELESTIA_DRY_RUN = "1", CAELESTIA_WALLPAPER = "" } })
  test.advance(500)
end
local function swipe(edge) test.ipc("swipe_test", edge) test.advance(600) end
local function state() return test.ipc("gesture_state") end
for _, style in ipairs { "material", "tsugumori" } do
  test.it(style .. " phone pulls open dashboard, notifications and quick settings", function()
    load(style)
    swipe("bottom") test.truthy(state().dashboard)
    swipe("top") test.truthy(state().sidebar) test.truthy(state().notifications)
    test.falsy(state().dashboard)
    swipe("top") test.truthy(state().settings)
    swipe("bottom") test.truthy(state().dashboard) test.falsy(state().sidebar)
    for _, edge in ipairs { "top", "bottom", "left", "right" } do
      local strip = test.get("phone-gesture-" .. edge)
      test.truthy(strip.width > 0 and strip.height > 0)
      test.truthy(strip.width == 20 or strip.height == 20)
    end
    test.eq(#test.logs("error"), 0)
  end)
  test.it(style .. " side swipes close panels and respect authentication dialogs", function()
    load(style)
    swipe("bottom") swipe("right")
    test.falsy(state().dashboard) test.eq(state().steps, { 1 })
    swipe("left") test.eq(state().steps, { 1, -1 })
    test.ipc("session", "open") test.advance(600)
    swipe("right") swipe("top") swipe("bottom")
    test.eq(state().steps, { 1, -1 })
    test.falsy(state().sidebar) test.falsy(state().dashboard)
    test.eq(#test.logs("error"), 0)
  end)
end
test.it("landscape desktop does not acquire phone gesture regions or actions", function()
  load("material", 1280, 800)
  swipe("bottom") swipe("top") swipe("right")
  test.falsy(state().dashboard) test.falsy(state().sidebar) test.eq(state().steps, {})
  test.eq(#test.find_all("phone-gesture-edges"), 0)
  test.eq(#test.logs("error"), 0)
end)

for _, style in ipairs { "material", "tsugumori" } do
  test.it(style .. " real touch switches dashboard tabs and pulls the drawer closed", function()
    load(style)
    test.swipe({558,2480}, {558,2240}) test.advance(700)
    test.truthy(state().dashboard)
    local pages = test.get("dashboard-pages")
    local x,y=pages.x+pages.width/2,pages.y+4
    test.swipe({x+180,y}, {x-180,y}, {duration=500}) test.advance(700)
    test.eq(state().dashboard_tab,2)
    test.swipe({x-180,y}, {x+180,y}) test.advance(700)
    test.eq(state().dashboard_tab,1)
    test.swipe({x,y}, {x,y+240}, {duration=500}) test.advance(700)
    test.falsy(state().dashboard)
    test.eq(#test.logs("error"),0)
  end)
  test.it(style .. " real touch switches top-panel tabs and pushes it closed", function()
    load(style)
    test.swipe({558,4}, {558,240}) test.advance(700)
    test.truthy(state().sidebar)
    local pages=test.get("sidebar-pages")
    local x,y=pages.x+pages.width/2,pages.y+4
    local before=state().sidebar_tab
    test.swipe({x-180,y}, {x+180,y}) test.advance(700)
    test.eq(state().sidebar_tab,math.max(1,before-1))
    test.swipe({x,y+180}, {x,y-60}) test.advance(700)
    test.falsy(state().sidebar)
    test.eq(#test.logs("error"),0)
  end)
end
