local test=morf.test
local HOST=[[
  local ui=require("morf.ui")
  local W,H=tonumber(morf.env("TEST_WIDTH")),tonumber(morf.env("TEST_HEIGHT"))
  morf.surface.height=H
  package.loaded.bar={desk=function() return 0,0,W,H end}
  local sample=morf.signal("test.dashboard.sample",42)
  local reads,actions={},{}
  local function read(name,value)
    reads[name]=(reads[name] or 0)+1
    sample:get()
    return value
  end
  local sources={}
  for _,name in ipairs {"cpu","memory","drives","network","gpu","fans","battery"} do
    sources[name]={pin=function() end,running=function() return true end,samples=0,interval=3000}
  end
  package.loaded["lib.sysinfo"]={sources=sources,history_size=60,
    cpu=function() return read("cpu",{usage=sample:get()}) end,
    memory=function() return read("memory",{percent=58}) end,
    disks=function() return read("disks",{{mount="/",percent=34}}) end,
    battery=function() return read("battery",{batteries={{name="BAT1",capacity=76,status="Discharging"}}}) end,
    history=function(key) return read("history",key:match("voltage$") and {11.4,11.6,11.5,11.7} or {42,58,68,76}) end,
    system=function() return read("system",{hostname="workstation",uptime=4386}) end}
  package.loaded.services={hyprland={available=function() return false end},
    weather=function() return read("weather",{available=true,temperature=21,condition="Clear",code=0,is_day=true,
      daily={{low=10,high=20},{low=11,high=21},{low=12,high=22},{low=13,high=23},
        {low=14,high=24},{low=15,high=25},{low=16,high=26}}}) end,
    weather_symbol=function() return "sunny" end,
    player=function() return read("player",{title="Morning signal",artist="Preview",album="Desktop",art_url="",playing=false}) end,
    media={play_pause=function() actions[#actions+1]="play_pause" end,
      previous=function() actions[#actions+1]="previous" end,next=function() actions[#actions+1]="next" end}}
  package.loaded.lule_studio={active=morf.signal("test.dashboard.lule",false)}
  local kit=require("kit")
  local C=require("theme").color
  local model=require("dashboard_model")
  model.username,model.face="preview",""
  model.clock=function(format) return format=="%H:%M" and "08:24" or format=="%I" and "08" or format=="%M" and "24" or "MONDAY, 28 SEPTEMBER" end
  model.calendar=function() return model.month(model.month_offset:get(),{year=2026,month=9,day=28}) end
  local page_sizes={{840,439},{1000,350},{1400,760},{1000,680},{870,650},{960,489}}
  local page_cache={}
  model.page=function(index)
    if index==5 and morf.env("TEST_REAL_WEATHER")=="1" then return require("dashboard_weather") end
    if index==4 and morf.env("TEST_REAL_BATTERY")=="1" then return require("dashboard_battery") end
    if not page_cache[index] then
      local w,h=table.unpack(page_sizes[index])
      local ctx=require("dashboard_state").context(index)
      local node=kit.card {id="test-page-"..index,width=w,height=h,
        kit.heading {id="test-title-"..index,x=20,y=20,text=model.tabs[index].name,active=ctx.opened},
        kit.subtitle {x=20,y=58,text="The page keeps its original controller and actions."},
        kit.pill {id="test-action-"..index,x=w-170,y=h-52,width=150,height=36,label="Page action",
          on_clicked=function() actions[#actions+1]="page-"..index end},
      }
      page_cache[index]={WIDTH=w,HEIGHT=h,page=node}
    end
    return page_cache[index]
  end
  local dashboard=require("dashboard")
  ui.Item {width=W,height=H,
    ui.Rect {width=W,height=H,color=function() return C.surfaceContainerLowest end},
    ui.Item {x=10,y=10,width=W-20,height=H-20,
      ui.Sdf {anchors={fill=true},fill_color=function() return C.surface end,dashboard.drawer.shape},dashboard.drawer.panel},
  }
  morf.ipc.show=function(on) dashboard.drawer.set(on=="yes") end
  morf.ipc.select=function(index) model.select(tonumber(index)) end
  morf.ipc.sample=function(value) sample:set(tonumber(value)) end
  morf.ipc.state=function()
    return {tab=model.tab:get(),displayed=model.displayed:get(),opened=model.opened:get(),reads=reads,actions=actions,
      month=model.month_offset:get(),lule=require("lule_studio").active:get()}
  end
]]
local function load(style,w,h,real_weather,real_battery)
  test.load("../shell/init.lua",{source=HOST,size={w or 1920,h or 1080},env={CAELESTIA_STYLE=style,
    CAELESTIA_DRY_RUN="1",TEST_WIDTH=tostring(w or 1920),TEST_HEIGHT=tostring(h or 1080),
    TEST_REAL_WEATHER=real_weather and "1" or "0",TEST_REAL_BATTERY=real_battery and "1" or "0"}})
  test.advance(100)
end
local function shot(name)
  if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(name..".png") end
end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." dashboard keeps calendar and playback actions and stops hidden readings",function()
    load(style)
    test.eq(test.ipc("state").reads,{})
    test.ipc("show","yes") test.advance(2400)
    test.truthy(test.get(style=="material" and "calendar-title" or "calendar-title-text").visible)
    if style=="material" then test.eq(test.get("drawer-dashboard").width,872) end
    test.click("calendar-next") test.advance(2400)
    test.eq(test.ipc("state").month,1)
    test.click("calendar-previous") test.advance(2400)
    test.eq(test.ipc("state").month,0)
    test.click("media-play") test.eq(test.ipc("state").actions,{"play_pause"})
    test.eq(test.ipc("state").tab,1)
    shot(style.."-dashboard-overview")
    test.ipc("show","no") test.advance(1000)
    local reads=test.ipc("state").reads
    test.ipc("sample","66") test.advance(2500)
    test.eq(test.ipc("state").reads,reads)
    test.ipc("show","yes") test.advance(2400)
    if style=="tsugumori" then
      test.eq(test.get("dashboard-cpu-value").text,"66%")
      test.eq(test.get("dashboard-battery-value").text,"76%")
    end
    test.eq(#test.logs("warn"),0) test.eq(#test.logs("error"),0)
  end)
  test.it(style.." dashboard tab changes present the latest page and survive closing mid-transition",function()
    load(style)
    test.ipc("show","yes") test.advance(2400)
    if style=="tsugumori" then
      test.click("dashboard-card-performance") test.advance(160)
      local card=test.get("dashboard-card-performance")
      test.truthy(card.opacity>0 and card.opacity<1)
      test.truthy(test.get("dashboard-card-weather").opacity>0)
      test.truthy(test.get("dashboard-card-weather").opacity<1)
      shot("tsugumori-dashboard-branch-moving")
    else test.click("dashboard-tab-performance") end
    test.advance(2400)
    test.eq(test.ipc("state").displayed,3)
    if style=="tsugumori" then
      local detail,body=test.get("dashboard-detail"),test.get("dashboard-pages")
      test.falsy(test.get("dashboard-overview").visible)
      test.near(detail.x,body.x,.01) test.near(detail.y,body.y,.01)
      test.near(detail.width,body.width,.01) test.near(detail.height,body.height,.01)
    end
    test.click("test-action-3") test.eq(test.ipc("state").actions,{"page-3"})
    shot(style.."-dashboard-performance")
    test.ipc("select","4") test.advance(80)
    test.ipc("select","5") test.advance(80)
    test.ipc("select","6") test.advance(2400)
    test.eq(test.ipc("state").displayed,6) test.truthy(test.ipc("state").lule)
    if style=="tsugumori" then
      test.falsy(test.get("dashboard-page-weather").visible)
      test.falsy(test.get("dashboard-detail-page-curtain").visible)
      test.click("dashboard-tab-dashboard") test.advance(2400)
      test.eq(test.ipc("state").displayed,1)
      test.truthy(test.get("dashboard-calendar").visible)
    end
    test.ipc("select","2") test.advance(80)
    test.ipc("show","no") test.advance(90)
    test.ipc("select","5") test.ipc("show","yes") test.advance(2400)
    test.eq(test.ipc("state").displayed,5)
    test.truthy(test.get("drawer-dashboard").visible)
    test.falsy(test.ipc("state").lule)
    if style=="tsugumori" then
      test.near(test.get("dashboard-detail").opacity,1,.001)
      test.falsy(test.get("dashboard-detail-page-curtain").visible)
    end
    test.truthy(test.settle(500)<100)
    test.eq(#test.logs("error"),0)
  end)
end
test.it("Tsugumori compact overview and native-size pages scroll without losing controls",function()
  load("tsugumori",800,520)
  test.ipc("show","yes") test.advance(2400)
  local panel=test.get("drawer-dashboard")
  test.truthy(panel.x>=0 and panel.x+panel.width<=800)
  test.truthy(panel.y>=0 and panel.y+panel.height<=520)
  local body=test.get("dashboard-pages")
  test.wheel(0,1500,{x=body.x+40,y=body.y+40}) test.advance(500)
  test.click("calendar-next") test.advance(2400)
  test.eq(test.ipc("state").month,1)
  shot("tsugumori-dashboard-compact-calendar")
  test.ipc("select","3") test.advance(2400)
  test.falsy(test.get("dashboard-overview").visible)
  local viewport=test.get("dashboard-page-performance")
  test.wheel(2000,2000,{x=viewport.x+40,y=viewport.y+40}) test.advance(500)
  local action=test.get("test-action-3")
  test.truthy(action.x>=viewport.x and action.x+action.width<=viewport.x+viewport.width+1)
  test.truthy(action.y>=viewport.y and action.y+action.height<=viewport.y+viewport.height+1)
  test.click("test-action-3") test.eq(test.ipc("state").actions,{"page-3"})
  shot("tsugumori-dashboard-compact-branch")
  test.ipc("select","1") test.advance(2400)
  test.truthy(test.get("dashboard-card-media").y>=body.y)
  test.eq(#test.logs("warn"),0) test.eq(#test.logs("error"),0)
end)

test.it("Tsugumori resized tabs stay separate and interrupted detail entry settles",function()
  load("tsugumori")
  test.ipc("show","yes") test.advance(2400)
  test.ipc("select","3") test.advance(340)
  test.ipc("select","4") test.advance(2400)
  local detail=test.get("dashboard-detail")
  local body=test.get("dashboard-pages")
  test.near(detail.x,body.x,.01)
  test.near(detail.y,body.y,.01)
  for _,index in ipairs {3,6,1} do
    test.ipc("select",tostring(index)) test.advance(2400)
    local previous
    for _,key in ipairs {"dashboard","media","performance","battery","weather","lule"} do
      local button=test.get("dashboard-tab-"..key)
      if previous then
        test.near(button.width,previous.width,.1)
        test.near(button.x,previous.x+previous.width+8,.1)
      end
      previous=button
    end
    shot("tsugumori-dashboard-tabs-"..index)
  end
end)

test.it("Tsugumori dashboard fits responsive weather and scrolls the final forecast into view",function()
  load("tsugumori",500,720,true)
  test.ipc("show","yes") test.ipc("select","5") test.advance(2500)
  local viewport=test.get("dashboard-page-weather")
  local page=test.get("dashboard-weather-tab")
  local navigation=test.get("dashboard-navigation")
  local selected=test.get("dashboard-tab-weather")
  test.truthy(selected.x>=navigation.x and selected.x+selected.width<=navigation.x+navigation.width)
  test.near(page.width,viewport.width,.01)
  test.truthy(test.get("weather-forecast").y>test.get("weather-readings").y)
  local title=test.get("weather-forecast-title-text").text
  local forecast=test.get("weather-forecast-title")
  test.wheel(0,forecast.y-viewport.y-24,{x=viewport.x+40,y=viewport.y+40}) test.advance(200)
  test.truthy(test.get("weather-forecast-title-text").text~=title,"forecast title decoded before scrolling into view")
  test.advance(2200)
  test.eq(test.get("weather-forecast-title-text").text,title)
  test.wheel(0,2000,{x=viewport.x+40,y=viewport.y+40}) test.advance(500)
  local final=test.get("weather-day-7")
  test.truthy(final.x>=viewport.x and final.x+final.width<=viewport.x+viewport.width)
  test.truthy(final.y>=viewport.y and final.y+final.height<=viewport.y+viewport.height)
  shot("tsugumori-dashboard-weather-compact")
  test.ipc("select","1") test.advance(2400)
  test.eq(test.ipc("state").displayed,1)
  test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
end)

test.it("Tsugumori dashboard fits Battery graphs and retains access to the final device fact",function()
  load("tsugumori",500,720,false,true)
  test.ipc("show","yes") test.ipc("select","4") test.advance(2500)
  local viewport=test.get("dashboard-page-battery")
  test.near(test.get("dashboard-battery").width,viewport.width,.01)
  local nav=test.get("dashboard-navigation")
  local selected=test.get("dashboard-tab-battery")
  test.truthy(selected.x>=nav.x and selected.x+selected.width<=nav.x+nav.width)
  local graph_title="battery-graph-voltage-title"
  local caption=test.get(graph_title.."-text").text
  local graph=test.get(graph_title)
  test.wheel(0,graph.y-viewport.y-24,{x=viewport.x+40,y=viewport.y+40}) test.advance(200)
  test.truthy(test.get(graph_title.."-text").text~=caption,"graph title decoded before scrolling into view")
  test.advance(2200)
  test.eq(test.get(graph_title.."-text").text,caption)
  test.wheel(0,2600,{x=viewport.x+40,y=viewport.y+40}) test.advance(500)
  local last=test.get("battery-fact-limit")
  test.truthy(last.x>=viewport.x and last.x+last.width<=viewport.x+viewport.width)
  test.truthy(last.y>=viewport.y and last.y+last.height<=viewport.y+viewport.height)
  shot("tsugumori-dashboard-battery-compact")
  test.ipc("select","1") test.advance(2400)
  test.eq(test.ipc("state").displayed,1)
  test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
end)
