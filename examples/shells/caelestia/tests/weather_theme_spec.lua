local test=morf.test
local HOST=[[
  local ui=require("morf.ui")
  local W,H=tonumber(morf.env("TEST_WIDTH")),tonumber(morf.env("TEST_HEIGHT"))
  morf.surface.height=H
  local shown=morf.signal("test.weather.shown",false)
  local revision=morf.signal("test.weather.revision",0)
  local reading,reads={},0
  local function update(mode)
    local daily={}
    for i=1,mode=="short" and 3 or 7 do
      daily[i]={time=morf.time.time {year=2026,month=9,day=21+i,hour=12},low=i-5,high=i+8,code=61}
    end
    daily[1].sunrise=morf.time.time {year=2026,month=9,day=22,hour=7,minute=18}
    daily[1].sunset=morf.time.time {year=2026,month=9,day=22,hour=19,minute=21}
    reading={available=true,place="Windshire, Netherlands",temperature=-2.4,feels_like=-4.5,
      humidity=82,wind_speed=12.25,condition="Light rain",code=61,is_day=true,daily=daily,
      units={temperature="°C",wind="km/h"}}
    if mode=="imperial" then
      reading.temperature,reading.feels_like=72.4,74.5
      reading.units={temperature="°F",wind="mph"}
    elseif mode=="missing" then reading={available=false}
    elseif mode=="partial" then reading={available=true,place="Windshire"}
    elseif mode=="stale" then reading.stale=true end
    revision:set(revision:get()+1)
  end
  update("metric")
  local services=require("services")
  services.weather=function() reads=reads+1 revision:get() return reading end
  package.loaded.dashboard_state={context=function() return {opened=function() return shown:get() end} end}
  local view=require("dashboard_weather")
  if view.resize then view.resize(W-24,H-24) end
  local C=require("theme").color
  ui.Item {width=W,height=H,
    ui.Rect {anchors={fill=true},color=function() return C.surfaceContainerLowest end},
    ui.Flickable {id="weather-viewport",x=12,y=12,width=W-24,height=H-24,clip=true,
      visible=function() return shown:get() end,
      ui.Item {width=view.width or view.WIDTH,height=view.height or view.HEIGHT,view.page}},
  }
  morf.ipc.shown=function(value) shown:set(value=="yes") end
  morf.ipc.update=update
  morf.ipc.reads=function() return reads end
  morf.ipc.resize=function(value) if view.resize then view.resize(tonumber(value)) end end
]]
local function load(style,w,h)
  test.load("../shell/init.lua",{source=HOST,size={w or 1000,h or 630},env={
    CAELESTIA_STYLE=style,CAELESTIA_DRY_RUN="1",TEST_WIDTH=tostring(w or 1000),TEST_HEIGHT=tostring(h or 630)}})
  test.advance(100)
end
local function shot(name)
  if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(name..".png") end
end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." weather preserves units, forecast, daylight and hidden service gating",function()
    load(style)
    test.eq(test.ipc("reads"),0)
    test.ipc("shown","yes") test.advance(2500)
    test.eq(test.get("weather-temperature").text,"-2°C")
    test.eq(test.get("weather-condition").text,"Light rain")
    test.eq(test.get("weather-sunrise").text,"7:18 AM")
    test.eq(test.get("weather-sunset").text,"7:21 PM")
    test.truthy(test.get("weather-day-7").visible)
    shot(style.."-weather")
    test.ipc("update","imperial") test.advance(2500)
    test.eq(test.get("weather-temperature").text,"72°F")
    test.truthy(test.text_of(test.get("weather-wind")):find("12.2 mph",1,true))
    test.ipc("update","short") test.advance(2500)
    test.truthy(test.get("weather-day-3").visible)
    test.falsy(test.get("weather-day-4").visible)
    if style=="tsugumori" then test.eq(test.get("weather-forecast-title-text").text,"3-DAY FORECAST") end
    test.ipc("shown","no") test.advance(1000)
    local reads=test.ipc("reads")
    test.ipc("update","imperial") test.advance(3000)
    test.eq(test.ipc("reads"),reads)
    test.ipc("shown","yes") test.advance(900)
    if style=="tsugumori" then test.truthy(test.get("weather-place-text").text~="WINDSHIRE") end
    test.advance(1800)
    test.eq(test.get("weather-temperature").text,"72°F")
    test.eq(#test.logs("error"),0)
  end)
  test.it(style.." weather tolerates unavailable and partial readings",function()
    load(style)
    test.ipc("update","missing") test.ipc("shown","yes") test.advance(2500)
    test.eq(test.get("weather-temperature").text,"--")
    test.eq(test.get("weather-sunrise").text,"--")
    test.falsy(test.get("weather-day-7").visible)
    if style=="tsugumori" then
      test.truthy(test.get("weather-forecast-empty").visible)
      test.eq(test.get("weather-status").text,"WAITING FOR WEATHER")
    end
    test.ipc("update","partial") test.advance(2500)
    test.eq(test.get("weather-temperature").text,"--")
    test.ipc("update","stale") test.advance(2500)
    test.eq(test.get("weather-temperature").text,"-2°C")
    if style=="tsugumori" then test.eq(test.get("weather-status").text,"LAST KNOWN CONDITIONS") end
    test.ipc("shown","no") test.advance(1000)
    test.truthy(test.settle(500)<100)
    test.eq(#test.logs("error"),0)
  end)
end
test.it("Tsugumori weather stacks at narrow widths and keeps the final forecast reachable",function()
  load("tsugumori",380,520)
  test.ipc("shown","yes") test.advance(2500)
  local page=test.get("dashboard-weather-tab")
  local forecast=test.get("weather-forecast")
  test.truthy(forecast.y>test.get("weather-readings").y)
  test.near(page.width,356,.01)
  test.truthy(forecast.x>=page.x and forecast.x+forecast.width<=page.x+page.width)
  shot("tsugumori-weather-compact-current")
  test.wheel(0,1600,{x=200,y=300}) test.advance(600)
  local last=test.get("weather-day-7")
  local viewport=test.get("weather-viewport")
  test.truthy(last.y>=viewport.y and last.y+last.height<=viewport.y+viewport.height)
  for i=1,7 do
    local band=test.get("weather-day-"..i.."-range")
    test.truthy(band.width>0 and band.x>=page.x and band.x+band.width<=page.x+page.width)
  end
  shot("tsugumori-weather-compact-forecast")
  test.ipc("shown","no") test.advance(100)
  test.ipc("shown","yes") test.advance(60)
  test.ipc("shown","no") test.advance(70)
  test.ipc("shown","yes") test.advance(2500)
  test.near(test.get("weather-now").opacity,1,.001)
  test.ipc("resize","900") test.advance(500)
  test.truthy(test.get("weather-forecast").x>test.get("weather-now").x)
  test.eq(#test.logs("error"),0)
  test.eq(#test.logs("warn"),0)
end)
