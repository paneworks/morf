local test=morf.test
local HOST=[[
  local ui=require("morf.ui")
  local W,H=tonumber(morf.env("TEST_WIDTH")),tonumber(morf.env("TEST_HEIGHT"))
  morf.surface.height=H
  local volume,muted,brightness=morf.signal("test.volume",.42),morf.signal("test.muted",false),morf.signal("test.brightness",.68)
  morf.audio={available=function() return true end,
    default_sink=function() return {id=1,volume=volume:get(),muted=muted:get()} end,
    set_volume=function(_,value) volume:set(value) end}
  package.loaded["lib.services.sysinfo"]={backlight=function() return {percent=brightness:get()*100} end,
    set_brightness=function(value) brightness:set(value/100) end}
  package.loaded.bar={desk=function() return 0,0,W,H end}
  package.loaded.rail={geometry=function() return {w=W,h=H,item=60,gap=12} end}
  local here=morf.signal("test.levels.here",true)
  package.loaded.services={here=function() return here:get() end}
  local theme=require("theme")
  local sidebar={drawer=require("drawer").new {name="sidebar",edge="right",width=430,height=360,content=ui.Item {}},
    showing=function(page) return page=="settings" end}
  package.loaded.sidebar=sidebar
  local levels=require("levels")
  local root=levels.build()
  local osd=require("osd")
  ui.Item {width=W,height=H,
    ui.Rect {width=W,height=H,color=function() return theme.color.surface end},
    ui.Sdf {anchors={fill=true},fill_color=function() return theme.color.surfaceContainer end,levels.shape,sidebar.drawer.shape},
    root,sidebar.drawer.panel}
  morf.ipc.pop=levels.pop
  morf.ipc.volume=function(v) osd.set_volume(tonumber(v)) end
  morf.ipc.brightness=function(v) osd.set_brightness(tonumber(v)) end
  morf.ipc.mute=function(on) muted:set(on=="yes") end
  morf.ipc.here=function(on) here:set(on=="yes") end
  morf.ipc.sidebar=function(on) sidebar.drawer.set(on=="yes") end
  morf.ipc.state=function() return {shown=levels.shown:get(),active=levels.active:get(),volume=osd.volume(),brightness=osd.brightness()} end
]]
local function load(style,w,h)
  test.load("../shell/init.lua",{source=HOST,size={w or 1100,h or 750},env={
    CAELESTIA_STYLE=style,TEST_WIDTH=tostring(w or 1100),TEST_HEIGHT=tostring(h or 750)}})
  test.advance(400)
end
local function snapshot(name)
  if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(name..".png") end
end
for _,style in ipairs {"material","tsugumori"} do
  local hold=2200
  test.it(style.." OSD follows readings and ignores initial, settings and other-output changes",function()
    load(style)
    test.eq(test.ipc("state").shown,"")
    test.ipc("volume",".65") test.advance(700)
    test.eq(test.ipc("state").shown,"volume")
    test.truthy(test.get("levels-swell").width>0)
    snapshot(style.."-volume")
    test.ipc("brightness",".25") test.advance(700)
    test.eq(test.ipc("state").shown,"brightness")
    snapshot(style.."-brightness")
    test.advance(hold+500)
    test.eq(test.ipc("state").shown,"")
    test.falsy(test.get("levels-swell").visible)
    test.ipc("sidebar","yes") test.advance(900)
    test.ipc("volume",".75") test.advance(700)
    test.eq(test.ipc("state").shown,"")
    test.ipc("sidebar","no") test.advance(900)
    test.ipc("here","no") test.ipc("brightness",".35") test.advance(700)
    test.eq(test.ipc("state").shown,"")
    test.eq(#test.logs("error"),0)
  end)
  test.it(style.." OSD refreshes its timeout and reverses an interrupted exit",function()
    load(style) test.ipc("pop","volume") test.advance(hold-100)
    test.ipc("pop","volume") test.advance(500)
    test.truthy(test.ipc("state").active)
    test.advance(hold-450) -- Fifty milliseconds into the closing animation.
    test.falsy(test.ipc("state").active)
    test.ipc("pop","brightness") test.advance(600)
    test.eq(test.ipc("state").shown,"brightness")
    test.truthy(test.get("levels-swell").width>0)
    test.near(test.get("levels-bud").opacity,1,.001)
    test.advance(hold+600)
    test.eq(test.ipc("state").shown,"")
    test.truthy(test.settle(500)<100)
    test.eq(#test.logs("error"),0)
    test.eq(#test.logs("warn"),0)
  end)
end
test.it("Tsugumori OSD shows mute state, clamps readings and fits a compact output",function()
  load("tsugumori",800,480)
  test.ipc("volume","2") test.ipc("mute","yes") test.advance(1500)
  test.eq(test.ipc("state").volume,1)
  test.eq(test.get("levels-title-text").text,"MUTED")
  test.eq(test.get("levels-value").text,"100%")
  local panel=test.get("levels-swell")
  test.truthy(panel.x>=0 and panel.x+panel.width<=800)
  test.truthy(panel.y>=0 and panel.y+panel.height<=480)
  snapshot("tsugumori-osd-muted-compact")
  test.ipc("brightness","-1") test.advance(1500)
  test.near(test.ipc("state").brightness,.01,.0001)
  test.eq(test.get("levels-title-text").text,"BRIGHTNESS")
  test.eq(test.get("levels-value").text,"1%")
end)
