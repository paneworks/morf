local test = morf.test
local SOURCE = [[
  local steps = {}
  require("services").workspace.step = function(delta) steps[#steps + 1] = delta end
  require("init")
  morf.surface.width=tonumber(morf.env("TEST_W"))
  morf.surface.height=tonumber(morf.env("TEST_H"))
  morf.ipc.swipe_test = function(edge) require("phone_gestures").swipe(edge) end
  morf.ipc.gesture_state = function()
    local sidebar = require("sidebar")
    return { dashboard = require("dashboard").drawer.open:get(),
      dashboard_y = require("dashboard").drawer.panel.translate_y,
      sidebar_y = sidebar.drawer.panel.translate_y,
      dashboard_tab = require("dashboard").tab:get(), sidebar_tab = sidebar.tab:get(),
      sidebar = sidebar.drawer.open:get(), notifications = sidebar.showing("notifications"),
      settings = sidebar.showing("settings"), steps = steps }
  end
]]
local function load(style, width, height, workspaces)
  test.stub_run("task", { code = 0, stdout = "[]" })
  test.load("../shell/init.lua", { source = SOURCE, size = { width or 1116, height or 2484 },
    env = { CAELESTIA_STYLE = style, CAELESTIA_DRY_RUN = "1", CAELESTIA_WALLPAPER = "",
      TEST_W=tostring(width or 1116), TEST_H=tostring(height or 2484), CAELESTIA_WORKSPACE_GESTURES=workspaces or "" } })
  test.advance(500)
end
local function swipe(edge) test.ipc("swipe_test", edge) test.advance(600) end
local function state() return test.ipc("gesture_state") end
for _, style in ipairs { "material", "tsugumori" } do
  test.it(style .. " leaves side edges to compositor workspace previews", function()
    load(style, nil, nil, "compositor")
    test.eq(#test.find_all("phone-gesture-left"), 0)
    test.eq(#test.find_all("phone-gesture-right"), 0)
    swipe("left") swipe("right") test.eq(state().steps, {})
    test.touch("down", 1, 558, 2480)
    test.touch("move", 1, 558, 2100) test.advance(500)
    test.truthy(state().dashboard)
    test.truthy(state().dashboard_y > 0)
    test.touch("cancel", 1) test.advance(500)
    test.eq(#test.logs("error"), 0)
  end)
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
    test.swipe({x,y}, {x,y+240}, {duration=160}) test.advance(700)
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

for _, style in ipairs { "material", "tsugumori" } do
  test.it(style .. " bottom drawer follows, holds and reverses under a finger", function()
    load(style)
    local hidden = state().dashboard_y
    test.touch("down",0,558,2480)
    test.advance(40) test.touch("move",0,558,1980)
    local half = state().dashboard_y
    test.near(half,hidden-500,1)
    test.advance(1200)
    test.near(state().dashboard_y,half,1)
    if morf.env("MORF_GESTURE_SNAPSHOTS")=="1" then
      test.truthy(test.snapshot(style.."-bottom-partial.png"))
    end
    test.touch("move",0,558,2180)
    test.near(state().dashboard_y,hidden-300,1)
    test.touch("cancel",0,558,2180) test.advance(500)
    test.falsy(state().dashboard)
    test.falsy(test.get("drawer-dashboard").visible)
    test.eq(#test.logs("error"),0)
  end)
  test.it(style .. " top drawer slow pull settles by position and can be dragged shut", function()
    load(style)
    test.touch("down",0,558,4)
    test.advance(400) test.touch("move",0,558,1500)
    test.advance(500) test.touch("up",0,558,1500) test.advance(500)
    test.truthy(state().sidebar)
    test.near(state().sidebar_y,0,1)
    local pages=test.get("sidebar-pages")
    local x,y=pages.x+pages.width/2,pages.y+4
    test.touch("down",0,x,y)
    test.advance(40) test.touch("move",0,x,y-120)
    test.near(state().sidebar_y,-120,1)
    test.advance(1000)
    test.near(state().sidebar_y,-120,1)
    test.touch("move",0,x,y-1600)
    test.advance(500) test.touch("up",0,x,y-1600) test.advance(500)
    test.falsy(state().sidebar)
    test.eq(#test.logs("error"),0)
  end)
end

for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." horizontal pages follow and hold without committing a tab",function()
    load(style)
    swipe("bottom")
    local pages=test.get("dashboard-pages")
    local track=test.get("dashboard-page-track")
    local x,y=pages.x+pages.width/2,pages.y+4
    test.touch("down",0,x,y)
    test.advance(40) test.touch("move",0,x-240,y)
    test.near(test.get("dashboard-page-track").x,track.x-240,1)
    test.eq(state().dashboard_tab,1)
    test.advance(1000)
    test.near(test.get("dashboard-page-track").x,track.x-240,1)
    if morf.env("MORF_GESTURE_SNAPSHOTS")=="1" then
      test.truthy(test.snapshot(style.."-pages-partial.png"))
    end
    test.touch("move",0,x-100,y)
    test.near(test.get("dashboard-page-track").x,track.x-100,1)
    test.touch("cancel",0,x-100,y) test.advance(500)
    test.eq(state().dashboard_tab,1)
    test.near(test.get("dashboard-page-track").x,track.x,1)
    test.eq(#test.logs("error"),0)
  end)
  test.it(style.." can catch a drawer while it settles without jumping",function()
    load(style)
    test.swipe({558,2480},{558,1800},{duration=160})
    test.advance(60)
    local before=state().dashboard_y
    test.touch("down",0,558,2480)
    test.advance(1) test.touch("move",0,558,2460)
    local caught=state().dashboard_y
    test.truthy(caught>0)
    test.near(caught,before-20,45)
    test.advance(1000) test.near(state().dashboard_y,caught,1)
    test.touch("cancel",0,558,2460) test.advance(500)
    test.truthy(state().dashboard)
    test.near(state().dashboard_y,0,1)
  end)
end

test.it("outside touch dismisses only a completed tap, not the start of a pull",function()
  load("material") swipe("bottom")
  test.touch("down",0,550,220)
  test.truthy(state().dashboard)
  test.touch("move",0,550,320) test.touch("up",0,550,320)
  test.advance(500) test.truthy(state().dashboard)
  test.touch("down",0,550,220) test.touch("up",0,550,220)
  test.advance(500) test.falsy(state().dashboard)
end)
