local test = morf.test
local SOURCE = [[
  local steps, switches = {}, {}
  require("services").workspace.step = function(delta) steps[#steps + 1] = delta end
  local go=require("services").workspace.go
  require("services").workspace.go=function(id) switches[#switches+1]=id go(id) end
  require("init")
  morf.surface.width=tonumber(morf.env("TEST_W"))
  morf.surface.height=tonumber(morf.env("TEST_H"))
  morf.ipc.swipe_test = function(edge,x) require("phone_gestures").swipe(edge,tonumber(x)) end
  morf.ipc.launcher_query_test = function(query) require("launcher").set_query(query) end
  morf.ipc.gesture_state = function()
    local sidebar = require("sidebar")
    return { dashboard = require("dashboard").drawer.open:get(),
      dashboard_y = require("dashboard").drawer.panel.translate_y,
      sidebar_y = sidebar.drawer.panel.translate_y,
      dashboard_tab = require("dashboard").tab:get(), sidebar_tab = sidebar.tab:get(),
      sidebar = sidebar.drawer.open:get(), notifications = sidebar.showing("notifications"),
      settings = sidebar.showing("settings"), steps = steps, switches=switches,
      launcher=require("launcher").drawer.open:get(), menu=require("menus").source:get(),
      query=require("launcher").query:get(),
      keyboard=require("keyboard").active(), keyboard_mode=require("keyboard").keys.mode:get(),
      active_workspace=require("services").workspace.active(),
      preview=require("phone_gestures").workspace_preview and {
        active=require("phone_gestures").workspace_preview.active,
        offset=require("phone_gestures").workspace_preview.offset,
        target=require("phone_gestures").workspace_preview.target,
      } }
  end
]]
local function load(style, width, height, workspaces, driver)
  test.stub_run("task", { code = 0, stdout = "[]" })
  test.load("../shell/init.lua", { source = SOURCE, size = { width or 1116, height or 2484 },
    env = { CAELESTIA_STYLE = style, CAELESTIA_DRY_RUN = "1", CAELESTIA_WALLPAPER = "",
      TEST_W=tostring(width or 1116), TEST_H=tostring(height or 2484),
      CAELESTIA_WORKSPACE_GESTURES=workspaces or "", CAELESTIA_GESTURE_DRIVER=driver or "" } })
  test.advance(500)
end
local function state() return test.ipc("gesture_state") end

for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." lisgd owns global swipes while Morf selects panels from the starting half",function()
    load(style,nil,nil,nil,"lisgd")
    test.falsy(state().preview)
    test.touch("down",0,850,2480) test.touch("move",0,350,2480) test.touch("up",0,350,2480)
    test.eq(state().steps,{}) test.eq(state().switches,{})
    test.truthy(test.ipc("phone-gesture","workspace-next"))
    test.eq(state().steps,{1}) test.falsy(test.ipc("phone-gesture","workspace-next"))
    test.swipe({400,2480},{400,2200}) test.advance(100)
    test.falsy(state().dashboard)
    test.truthy(test.ipc("phone-gesture","dashboard")) test.advance(600)
    test.truthy(state().dashboard)
    for _,x in ipairs {300,800} do
      test.swipe({x,4},{x,300}) test.advance(100)
      test.truthy(test.ipc("phone-gesture","top")) test.advance(600)
      test.truthy(state().sidebar) test.falsy(state().dashboard)
      test.eq(state().notifications,x<558) test.eq(state().settings,x>=558)
    end
    test.eq(test.logs("error"),{})
  end)
  test.it(style.." lisgd commands reject missing, stale, cancelled and authentication-blocked origins",function()
    load(style,nil,nil,nil,"lisgd")
    test.falsy(test.ipc("phone-gesture","workspace-next"))
    test.touch("down",0,850,2480) test.touch("move",0,350,2480) test.touch("cancel",0)
    test.falsy(test.ipc("phone-gesture","workspace-next"))
    test.swipe({850,2480},{350,2480}) test.advance(2600)
    test.falsy(test.ipc("phone-gesture","workspace-next"))
    test.swipe({850,2480},{350,2480}) test.ipc("session","open")
    test.falsy(test.ipc("phone-gesture","workspace-next"))
    test.eq(state().steps,{}) test.eq(state().switches,{})
    test.eq(test.logs("error"),{})
  end)
  test.it(style.." lisgd shows the keyboard once while Morf still switches layout and hides it",function()
    load(style,nil,nil,nil,"lisgd")
    local function pair(x,y,dy)
      test.touch("down",0,x,y) test.touch("down",1,x+80,y)
      test.touch("move",0,x,y+dy) test.touch("move",1,x+80,y+dy)
      test.touch("up",0,x,y+dy) test.touch("up",1,x+80,y+dy)
    end
    pair(400,2480,-200)
    test.falsy(state().keyboard)
    test.truthy(test.ipc("phone-gesture","keyboard")) test.advance(600)
    test.truthy(state().keyboard) test.eq(state().keyboard_mode,"full")
    test.falsy(test.ipc("phone-gesture","keyboard"))
    local key=test.get("caelestia.osk.key.full.letters.q")
    pair(key.x+10,key.y+20,-100) test.advance(600)
    test.eq(state().keyboard_mode,"dev")
    key=test.get("caelestia.osk.key.dev.letters.q")
    pair(key.x+10,key.y+20,100) test.advance(600)
    test.falsy(state().keyboard) test.eq(state().steps,{})
    test.eq(test.logs("error"),{})
  end)
end

for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." top and bottom sheets keep identical side margins at every phone scale",function()
    for _,scale in ipairs {.75,1,1.5,1.8,2} do
      load(style,math.floor(1116/scale+.5),math.floor(2484/scale+.5))
      local top=test.get("drawer-sidebar")
      local bottom=test.get("drawer-dashboard")
      test.near(bottom.width,top.width,.5)
      test.near(bottom.x,top.x,.5)
      test.truthy(bottom.x>0)
    end
  end)
end

for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." bottom workspace preview follows and reverses without switching until release",function()
    load(style)
    test.touch("down",0,900,2480)
    test.advance(40) test.touch("move",0,400,2480)
    test.truthy(state().preview.active)
    test.near(state().preview.offset,-500,1)
    test.eq(state().active_workspace,1) test.eq(state().switches,{})
    local x=test.get("phone-workspace-page-0").x
    test.advance(1200) test.near(test.get("phone-workspace-page-0").x,x,1)
    test.touch("move",0,820,2480) test.near(state().preview.offset,-80,1)
    test.touch("up",0,820,2480) test.advance(400)
    test.eq(state().switches,{}) test.falsy(state().preview.active)
    test.eq(#test.find_all("phone-workspace-page-0"),0)
    test.touch("down",0,900,2480) test.touch("move",0,300,2480)
    test.truthy(state().preview.active)
    test.eq(state().preview.target,2)
    test.near(state().preview.offset,-600,1)
    test.eq(state().switches,{})
    test.touch("up",0,300,2480) test.advance(400)
    test.eq(test.logs("error"),{})
    test.falsy(state().preview.active)
    test.eq(state().switches,{2}) test.eq(state().active_workspace,2)
    test.touch("down",0,200,2480) test.touch("move",0,800,2480)
    test.touch("up",0,800,2480) test.advance(400)
    test.eq(state().switches,{2,1})
    test.eq(#test.logs("error"),0)
  end)
  test.it(style.." workspace drag cancels on second finger, touch cancel and authentication",function()
    load(style)
    test.touch("down",0,900,2480) test.touch("move",0,300,2480)
    test.touch("down",1,700,2480)
    test.falsy(state().preview.active)
    test.touch("up",0,300,2480) test.touch("up",1,700,2480)
    test.advance(400) test.eq(state().switches,{})
    test.touch("down",0,900,2480) test.touch("move",0,300,2480)
    test.touch("cancel",0) test.advance(400)
    test.falsy(state().preview.active) test.eq(state().switches,{})
    test.touch("down",0,900,2480) test.touch("move",0,300,2480)
    test.ipc("session","open")
    test.touch("up",0,300,2480) test.advance(400)
    test.falsy(state().preview.active) test.eq(state().switches,{})
    test.ipc("session","close") test.advance(400)
    test.swipe({1110,1200},{400,1200}) test.advance(400)
    test.eq(state().switches,{}) test.falsy(state().preview.active)
    test.eq(#test.logs("error"),0)
  end)
end
local function swipe(edge,x) test.ipc("swipe_test", edge,tostring(x or 0)) test.advance(600) end
for _, style in ipairs { "material", "tsugumori" } do
  test.it(style .. " side pulls open Apps from the left and Web from the right on release", function()
    load(style, nil, nil, "compositor")
    for _,edge in ipairs {"left","right"} do
      local strip=test.get("phone-gesture-"..edge)
      test.eq(strip.width,20) test.eq(strip.y,20)
      test.eq(strip.height,2444)
    end
    test.touch("down",0,4,1200)
    test.advance(200) test.touch("move",0,204,1200)
    test.falsy(state().launcher)
    test.advance(500) test.touch("up",0,204,1200) test.advance(600)
    test.truthy(state().launcher) test.eq(state().menu,"apps")
    test.ipc("launcher_query_test","previous query")
    test.swipe({1112,1200},{912,1200},{duration=500}) test.advance(600)
    test.truthy(state().launcher) test.eq(state().menu,"web") test.eq(state().query,"")
    test.eq(state().steps,{}) test.eq(state().switches,{})
    test.ipc("launcher","close") test.advance(600)
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
    swipe("top",1000) test.truthy(state().settings)
    swipe("bottom") test.truthy(state().dashboard) test.falsy(state().sidebar)
    for _, edge in ipairs { "top", "bottom" } do
      local strip = test.get("phone-gesture-" .. edge)
      test.truthy(strip.width > 0 and strip.height > 0)
      test.truthy(strip.width == 20 or strip.height == 20)
    end
    test.eq(#test.logs("error"), 0)
  end)
  test.it(style .. " side launchers replace panels and authentication blocks every edge", function()
    load(style)
    swipe("bottom") swipe("right")
    test.falsy(state().dashboard) test.truthy(state().launcher) test.eq(state().menu,"web")
    swipe("left") test.eq(state().menu,"apps") test.eq(state().steps,{})
    test.ipc("launcher","close") swipe("bottom")
    test.ipc("session", "open") test.advance(600)
    swipe("left") swipe("right") swipe("top") swipe("bottom")
    test.eq(state().steps, {})
    test.falsy(state().launcher)
    test.falsy(state().sidebar) test.truthy(state().dashboard)
    test.eq(#test.logs("error"), 0)
  end)
  test.it(style .. " side pulls ignore taps, vertical motion, short drags and cancellations", function()
    load(style)
    test.touch("down",0,4,1200) test.touch("up",0,4,1200)
    test.swipe({4,1200},{4,1400})
    test.swipe({4,1200},{34,1200},{duration=500})
    test.touch("down",0,4,1200) test.touch("move",0,204,1200) test.touch("cancel",0)
    test.touch("down",0,4,1200) test.touch("move",0,204,1200)
    test.touch("down",1,400,1200)
    test.touch("up",0,204,1200) test.touch("up",1,400,1200)
    test.touch("down",0,4,1200)
    test.advance(40) test.touch("move",0,204,1200)
    test.advance(40) test.touch("move",0,24,1200)
    test.touch("up",0,24,1200) test.advance(600)
    test.falsy(state().launcher) test.eq(state().switches,{})
    test.eq(test.logs("error"),{})
  end)
  test.it(style .. " a short fast inward fling opens a launcher but authentication cancels a pull", function()
    load(style)
    test.swipe({4,1200},{44,1200},{duration=40}) test.advance(600)
    test.truthy(state().launcher) test.eq(state().menu,"apps")
    test.ipc("launcher","close") test.advance(600)
    test.touch("down",0,1112,1200) test.touch("move",0,912,1200)
    test.ipc("session","open")
    test.touch("up",0,912,1200) test.advance(600)
    test.falsy(state().launcher) test.eq(test.logs("error"),{})
  end)
end
test.it("landscape desktop does not acquire phone gesture regions or actions", function()
  load("material", 1280, 800)
  swipe("bottom") swipe("top") swipe("left") swipe("right")
  test.falsy(state().dashboard) test.falsy(state().sidebar) test.eq(state().steps, {})
  test.falsy(state().launcher)
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
