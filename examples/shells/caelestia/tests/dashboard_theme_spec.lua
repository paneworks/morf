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
  package.loaded["lib.services.sysinfo"]={sources=sources,history_size=60,
    restore_history=function() end,snapshot_history=function() return {} end,
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
    test.truthy(test.ipc("state").reads.cpu>(reads.cpu or 0),"the overview did not read again on reopening")
    test.eq(#test.logs("warn"),0) test.eq(#test.logs("error"),0)
  end)
  test.it(style.." dashboard tab changes present the latest page and survive closing mid-transition",function()
    load(style)
    test.ipc("show","yes") test.advance(2400)
    test.click("dashboard-tab-performance")
    test.advance(2400)
    test.eq(test.ipc("state").displayed,3)
    test.click("test-action-3") test.eq(test.ipc("state").actions,{"page-3"})
    shot(style.."-dashboard-performance")
    test.ipc("select","4") test.advance(80)
    test.ipc("select","5") test.advance(80)
    test.ipc("select","6") test.advance(2400)
    test.eq(test.ipc("state").displayed,6) test.truthy(test.ipc("state").lule)
    test.click("dashboard-tab-dashboard") test.advance(2400)
    test.eq(test.ipc("state").displayed,1)
    test.truthy(test.get("dashboard-calendar").visible)
    test.ipc("select","2") test.advance(80)
    test.ipc("show","no") test.advance(90)
    test.ipc("select","5") test.ipc("show","yes") test.advance(2400)
    test.eq(test.ipc("state").displayed,5)
    test.truthy(test.get("drawer-dashboard").visible)
    test.falsy(test.ipc("state").lule)
    local curtain=test.find {id="dashboard-page-curtain"}
    if curtain then test.falsy(curtain.visible) end
    test.truthy(test.settle(500)<100)
    test.eq(#test.logs("error"),0)
  end)
end
-- The shared layout: one tab row and one page track in every theme.
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." dashboard tabs stay evenly apart as the drawer resizes between pages",function()
    load(style)
    test.ipc("show","yes") test.advance(2400)
    for _,index in ipairs {3,6,1} do
      test.ipc("select",tostring(index)) test.advance(2400)
      local panel,previous=test.get("drawer-dashboard"),nil
      for _,key in ipairs {"dashboard","media","performance","battery","weather","lule"} do
        local button=test.get("dashboard-tab-"..key)
        test.truthy(button.x>=panel.x and button.x+button.width<=panel.x+panel.width+.5,"tab outside the drawer: "..key)
        if previous then
          test.near(button.width,previous.width,.5)
          test.truthy(button.x>=previous.x+previous.width-.5,"tabs overlap: "..key)
        end
        previous=button
      end
      shot(style.."-dashboard-tabs-"..index)
    end
    test.eq(#test.logs("error"),0)
  end)
end

test.it("Tsugumori dashboard weather keeps the forecast under the readings and decodes its title on entry",function()
  load("tsugumori",1920,1080,true)
  test.ipc("show","yes") test.ipc("select","5") test.advance(400)
  local title=test.get("weather-forecast-title-text")
  test.truthy(title.text~="7-DAY FORECAST","forecast title did not decode on entry")
  test.advance(2400)
  test.eq(test.ipc("state").displayed,5)
  local page=test.get("dashboard-weather-tab")
  test.truthy(test.get("weather-forecast").y>test.get("weather-humidity").y)
  local final=test.get("weather-day-7")
  test.truthy(final.visible)
  test.truthy(final.x>=page.x and final.x+final.width<=page.x+page.width+.5)
  test.truthy(final.y+final.height<=page.y+page.height+.5)
  shot("tsugumori-dashboard-weather")
  test.ipc("select","1") test.advance(2400)
  test.eq(test.ipc("state").displayed,1)
  test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
end)

test.it("Tsugumori dashboard battery keeps its graphs and every device fact on the page",function()
  load("tsugumori",1920,1080,false,true)
  test.ipc("show","yes") test.ipc("select","4") test.advance(2500)
  local page=test.get("dashboard-battery")
  for _,id in ipairs {"battery-graph-charge","battery-graph-power","battery-graph-voltage","battery-graph-temperature",
    "battery-facts"} do
    local node=test.get(id)
    test.truthy(node.visible,id)
    test.truthy(node.x>=page.x and node.x+node.width<=page.x+page.width+.5,id.." leaves the page")
    test.truthy(node.y>=page.y and node.y+node.height<=page.y+page.height+.5,id.." leaves the page")
  end
  shot("tsugumori-dashboard-battery")
  test.ipc("select","1") test.advance(2400)
  test.eq(test.ipc("state").displayed,1)
  test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
end)
