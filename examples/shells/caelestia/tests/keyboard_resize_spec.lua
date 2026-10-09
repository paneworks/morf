local test=morf.test
local SOURCE=[[
  local keys={}
  local osk=require("lib.util.osk")
  local build=osk.new
  osk.new=function(options)
    local send=options.send
    options.send=function(event) keys[#keys+1]=event if send then send(event) end end
    return build(options)
  end
  require("init")
  morf.surface.width=tonumber(morf.env("TEST_W"))
  morf.surface.height=tonumber(morf.env("TEST_H"))
  morf.ipc.layout=function()
    return {reserved=morf.surface.reserve,inset=require("themes.keyboard").inset:get(),
      dashboard=require("dashboard").drawer.open:get(),sidebar=require("sidebar").drawer.open:get(),
      keyboard=require("keyboard").active(),focus=morf.surface.keyboard_focus}
  end
  morf.ipc.numbers=function(value) require("keyboard").keys.numbers:set(value=="yes") end
  morf.ipc.terminal=function()
    require("dashboard").tab:set(6)
    require("dashboard").drawer.set(true)
  end
  morf.ipc.editor=function()
    local studio=require("lule_studio")
    studio.set_source("files") studio.folder_draft:set("")
  end
  morf.ipc.draft=function() return require("lule_studio").folder_draft:get() end
  morf.ipc.keys=function() return keys end
]]
local function load(style,w,h)
  test.stub_run("task",{code=0,stdout="[]"})
  test.load("../shell/init.lua",{source=SOURCE,size={w,h},env={CAELESTIA_STYLE=style,
    CAELESTIA_DRY_RUN="1",CAELESTIA_SCALE_MODE="compositor",CAELESTIA_WALLPAPER="",
    TEST_W=tostring(w),TEST_H=tostring(h)}})
  test.advance(700)
end
local function state() return test.ipc("layout") end
local function pair(y,dy)
  test.touch("down",10,230,y) test.touch("down",11,310,y)
  test.touch("move",10,230,y+dy) test.touch("move",11,310,y+dy)
  test.touch("up",10,230,y+dy) test.touch("up",11,310,y+dy)
  test.advance(1200)
end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." keyboard resizes open panels and lifts workspace pills without closing them",function()
    load(style,620,1380)
    test.ipc("dashboard","open") test.ipc("sidebar","open") test.advance(1000)
    local before=state()
    local rail_y=test.get("rail-pill-1").y
    local dashboard_h=test.get("drawer-dashboard").height
    local sidebar_h=test.get("drawer-sidebar").height
    local scroll_h=test.get("dashboard-scroll-1").height
    pair(1375,-140)
    local open=state()
    test.truthy(open.keyboard) test.truthy(open.dashboard) test.truthy(open.sidebar)
    test.truthy(open.inset>200)
    test.eq(open.reserved.bottom-before.reserved.bottom,open.inset)
    test.near(test.get("rail-pill-1").y,rail_y-open.inset,1)
    local board=test.get("drawer-keyboard")
    test.truthy(test.get("rail-pill-1").y+6<board.y)
    test.truthy(test.get("drawer-dashboard").height<dashboard_h)
    test.truthy(test.get("drawer-sidebar").height<sidebar_h)
    test.truthy(test.get("dashboard-scroll-1").height<scroll_h)
    for _,id in ipairs {"drawer-dashboard","drawer-sidebar"} do
      local p=test.get(id)
      test.truthy(p.y>=0 and p.y+p.height<=board.y,id.." overlaps keyboard")
    end
    local edge=test.get("phone-gesture-bottom")
    test.truthy(edge.y+edge.height<=board.y)
    local bottom=board.y+board.height
    test.ipc("numbers","yes") test.advance(1200)
    local taller=state()
    test.truthy(taller.inset>open.inset)
    board=test.get("drawer-keyboard")
    test.near(board.y+board.height,bottom,1)
    test.near(test.get("rail-pill-1").y,rail_y-taller.inset,1)
    test.ipc("keyboard","close") test.advance(1200)
    test.truthy(state().dashboard) test.truthy(state().sidebar)
    test.eq(state().inset,0) test.eq(state().reserved,before.reserved)
    test.near(test.get("rail-pill-1").y,rail_y,1)
    test.near(test.get("drawer-dashboard").height,dashboard_h,1)
    test.near(test.get("dashboard-scroll-1").height,scroll_h,1)
    test.eq(test.logs("error"),{})
  end)
  test.it(style.." keyboard opening preserves the dashboard terminal and its typing focus",function()
    load(style,744,1656)
    test.ipc("terminal") test.advance(900)
    local before=state()
    test.truthy(before.dashboard)
    test.eq(before.focus,"on_demand")
    local terminal_height=test.get("dashboard-terminal").height
    pair(1651,-140)
    test.truthy(state().dashboard) test.truthy(state().keyboard)
    test.eq(state().focus,before.focus)
    test.truthy(test.get("dashboard-terminal").height<terminal_height)
    local panel=test.get("drawer-dashboard")
    test.truthy(panel.y+panel.height<=test.get("drawer-keyboard").y)
    test.eq(test.logs("error"),{})
  end)
  test.it(style.." showing the keyboard retains a settings field's draft and text focus",function()
    load(style,744,1656)
    test.ipc("settings","theme/lule") test.ipc("editor") test.advance(900)
    test.click("lule-folder") test.type("draft")
    test.eq(test.ipc("draft"),"draft")
    pair(1651,-140)
    test.truthy(state().sidebar) test.truthy(state().keyboard)
    test.eq(state().focus,"on_demand")
    test.type("x") test.eq(test.ipc("draft"),"draftx")
    test.ipc("keyboard","close") test.advance(700)
    test.type("y") test.eq(test.ipc("draft"),"draftxy")
    test.truthy(state().sidebar)
    test.eq(test.logs("error"),{})
  end)
  test.it(style.." tapping keyboard keys does not dismiss panels or steal their text focus",function()
    load(style,744,1656)
    test.ipc("settings","theme/lule") test.ipc("editor") test.advance(900)
    test.click("lule-folder") test.type("draft")
    pair(1651,-140)
    local q=test.get("caelestia.osk.key.full.letters.q")
    test.touch("down",0,q.x+10,q.y+10) test.touch("up",0,q.x+10,q.y+10)
    test.advance(200)
    test.truthy(state().sidebar,"keyboard tap dismissed settings")
    test.truthy(state().keyboard)
    test.eq(#test.ipc("keys"),1)
    test.type("x") test.eq(test.ipc("draft"),"draftx")
    test.ipc("sidebar","close") test.ipc("terminal") test.advance(900)
    q=test.get("caelestia.osk.key.full.letters.q")
    test.touch("down",0,q.x+10,q.y+10) test.touch("up",0,q.x+10,q.y+10)
    test.advance(200)
    test.truthy(state().dashboard,"keyboard tap dismissed dashboard")
    test.eq(state().focus,"on_demand")
    -- Genuine outside taps still dismiss the panel while the keyboard stays.
    test.touch("down",0,370,180) test.touch("up",0,370,180) test.advance(900)
    test.falsy(state().dashboard) test.truthy(state().keyboard)
    test.eq(test.logs("error"),{})
  end)
end
