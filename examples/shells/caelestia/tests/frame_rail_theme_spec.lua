local test=morf.test
local HOST=[[
  local ui=require("morf.ui")
  local paths={}
  local path=ui.Path
  ui.Path=function(props)
    local node=path(props)
    if props.id then paths[props.id]=node end
    return node
  end
  local W,H=tonumber(morf.env("TEST_WIDTH")),tonumber(morf.env("TEST_HEIGHT"))
  morf.surface.height=H
  local active=morf.signal("test.workspace",1)
  local enabled=morf.signal("test.rail.enabled",true)
  local top=morf.signal("test.bar.top",0)
  local config=require("config")
  local get=config.get
  config.get=function(key)
    if key=="rail.enabled" then return enabled:get() end
    if key=="rail.hold" then return 800 end
    return get(key)
  end
  local function desk() return 0,top:get(),W,H-top:get() end
  package.loaded.bar={desk=desk}
  package.loaded.services={workspace={active=function() return active:get() end,
    occupied=function(id) return id==1 or id==4 or id==15 end}}
  local clicks,triggers=0,0
  local left=require("drawer").new {name="leftbar",edge="left",width=430,height=360,
    content=ui.MouseArea {id="frame-panel-content",anchors={fill=true},on_clicked=function() clicks=clicks+1 end}}
  package.loaded.leftbar={drawer=left}
  local rail=require("rail")
  local node=rail.build()
  local frame=require("themes").view("frame")
  local root=frame.build {desk=desk,bar=ui.Item {},drawers={left},rail={node=node,shape=rail.shape},
    levels={node=ui.Item {},shape=ui.SdfShape {shape="box",width=0,height=0,opacity=0}},
    overlays={ui.MouseArea {id="frame-catcher",anchors={fill=true},visible=function() return left.open:get() end,
      on_clicked=function() if not left.panel.contains_pointer then left.set(false) end end}},
    triggers={ui.MouseArea {id="frame-trigger",width=40,height=10,on_entered=function() triggers=triggers+1 end}}}
  ui.Item {width=W,height=H,ui.Rect {width=W,height=H,color="#101a24"},root}
  morf.ipc.workspace=function(id) active:set(tonumber(id)) end
  morf.ipc.enabled=function(on) enabled:set(on=="yes") end
  morf.ipc.left=function(on) left.set(on=="yes") end
  morf.ipc.top=function(height) top:set(tonumber(height)) end
  morf.ipc.ruler=function(edge,kind) return paths["tsugumori-frame-ruler-"..edge.."-"..kind].d end
  morf.ipc.state=function() return {workspace=active:get(),open=left.open:get(),clicks=clicks,triggers=triggers,
    geometry=rail.geometry(),insets=frame.insets {left=0,top=top:get(),right=0,bottom=0}} end
]]
local function load(style,w,h)
  test.load("../shell/init.lua",{source=HOST,size={w or 1280,h or 800},env={CAELESTIA_STYLE=style,
    TEST_WIDTH=tostring(w or 1280),TEST_HEIGHT=tostring(h or 800)}})
  test.advance(400)
end
local function shot(name)
  if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(name..".png") end
end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." frame preserves drawer layering, edge input and bar offsets",function()
    load(style)
    local opening=test.get("frame-opening")
    test.eq(opening.x,10) test.eq(opening.y,10)
    test.eq(opening.width,1260) test.eq(opening.height,780)
    test.ipc("left","yes") test.advance(1200)
    test.click("frame-panel-content") test.eq(test.ipc("state").clicks,1)
    test.truthy(test.ipc("state").open)
    test.click(1100,400) test.advance(1100)
    test.falsy(test.ipc("state").open)
    test.move(20,2) test.truthy(test.ipc("state").triggers>0)
    test.ipc("top","64") test.advance(400)
    test.eq(test.get("desk").y,64)
    test.eq(test.get("frame-opening").y,74)
    test.eq(test.get("frame-opening").height,716)
    test.eq(test.ipc("state").insets,{left=10,top=74,right=10,bottom=10})
    shot(style.."-frame-bar-inset")
    test.eq(#test.logs("error"),0)
    test.eq(#test.logs("warn"),0)
  end)
  test.it(style.." workspace rail handles rapid changes, groups and disabling mid-animation",function()
    load(style)
    test.truthy(test.find("rail-pill-9"))
    test.falsy(test.find("rail-pill-10"))
    for _,id in ipairs {1,11,21} do
      test.ipc("workspace",tostring(id)) test.advance(1200)
      local g=test.ipc("state").geometry
      if style=="tsugumori" then test.near(test.get("rail-selector").y,g.top,.001) end
      test.near(test.get("rail-pill-1").y,g.top,.001)
      test.near(test.get("rail-pill-9").y,g.top+8*(g.item+g.gap),.001)
    end
    test.ipc("workspace","4") test.advance(450)
    shot(style.."-rail-moving")
    test.ipc("workspace","7") test.advance(160)
    test.ipc("workspace","15") test.advance(500)
    if style=="tsugumori" then
      test.eq(test.get("rail-value").text,"15")
      test.eq(test.get("rail-title-text").text,"WORKSPACE")
    end
    test.advance(2000)
    if style=="material" then
      test.near(test.get("rail-field").opacity,0,.001)
      test.near(test.get("rail-pill-5").opacity,1,.001)
    else test.falsy(test.get("rail-swell").visible) end
    test.ipc("workspace","16") test.advance(200)
    test.ipc("enabled","no") test.advance(20)
    test.falsy(test.get("rail").visible)
    test.near(test.get("rail-swell-background").opacity,0,.001)
    test.ipc("enabled","yes") test.advance(500)
    test.near(test.get("rail-swell-background").opacity,0,.001)
    test.ipc("workspace","17") test.advance(400)
    test.truthy(test.get("rail-swell-background").opacity>.9)
    test.advance(2500)
    test.truthy(test.settle(500)<100)
    test.eq(#test.logs("error"),0)
  end)
end
test.it("Tsugumori rulers bracket three workspaces and stay continuous behind pills",function()
  for _,size in ipairs {{800,480},{1280,801}} do
    load("tsugumori",size[1],size[2])
    local g=test.ipc("state").geometry
    local origin=g.top-g.gap/2
    local majors={}
    for y in test.ipc("ruler","left","major"):gmatch("M[%d.]+ ([%d.]+) H") do
      majors[#majors+1]=tonumber(y)
    end
    for group=0,3 do
      local expected=origin+group*3*(g.item+g.gap)+.5
      local found=false
      for _,y in ipairs(majors) do if y==expected then found=true end end
      test.truthy(found,"missing workspace group boundary at "..expected)
    end
    for _,edge in ipairs {"left","right"} do
      local ticks={}
      for _,kind in ipairs {"minor","major"} do
        for y in test.ipc("ruler",edge,kind):gmatch("M[%d.]+ ([%d.]+) H") do
          ticks[#ticks+1]=tonumber(y)
        end
      end
      table.sort(ticks)
      test.truthy(#ticks>0)
      test.truthy(ticks[1]<=22.5)
      test.truthy(ticks[#ticks]>=g.h-22.5)
      for i=2,#ticks do test.eq(ticks[i]-ticks[i-1],8,"ruler has a missing tick") end
    end
    shot("tsugumori-clean-frame-"..size[2])
    test.eq(#test.logs("error"),0)
  end
end)
test.it("Tsugumori rail stays in a compact viewport and follows the left drawer",function()
  load("tsugumori",800,480)
  test.ipc("workspace","9") test.advance(450)
  local card=test.get("rail-swell")
  test.truthy(card.x>=0 and card.x+card.width<=800)
  test.truthy(card.y>=0 and card.y+card.height<=480)
  shot("tsugumori-rail-compact")
  local before=test.get("rail-selector").x
  test.ipc("left","yes") test.advance(1200)
  test.truthy(test.get("rail-selector").x>before+400)
  test.eq(#test.logs("warn"),0)
end)
