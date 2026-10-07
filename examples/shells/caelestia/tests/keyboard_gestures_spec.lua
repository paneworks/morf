local test=morf.test
local HOST=[[
  local ui=require("morf.ui")
  local W,H=tonumber(morf.env("TEST_WIDTH")),tonumber(morf.env("TEST_HEIGHT"))
  morf.surface.width,morf.surface.height=W,H
  morf.surface.keyboard_focus="none"
  package.loaded.bar={desk=function() return 10,10,W-20,H-20 end}
  local attached=false
  package.loaded["lib.services.keyboards"]={attached=function() return attached end}
  local events,ime_callback={},nil
  morf.input_method={subscribe=function(fn) ime_callback=fn end,
    commit=function(value) events[#events+1]={text=value} end}
  morf.virtual_keyboard={key=function(code,on) events[#events+1]={code=code,on=on} end,
    modifiers=function(mask) events[#events+1]={mask=mask} end}
  require("config").set("keyboard.auto",true)
  local keyboard=require("keyboard")
  local C=require("theme").color
  local root=ui.Item {width=W,height=H,
    ui.Rect {anchors={fill=true},color=function() return C.surface end},
    ui.Item {x=10,y=10,width=W-20,height=H-20,
      ui.Sdf {anchors={fill=true},fill_color=function() return C.surfaceContainer end,keyboard.drawer.shape},keyboard.drawer.panel}}
  require("phone_gestures").attach(root)
  morf.ipc.show=keyboard.show
  morf.ipc.close=function() keyboard.drawer.set(false) end
  morf.ipc.manual=function() if keyboard.set then keyboard.set(true) else keyboard.drawer.set(true) end end
  morf.ipc.ime=function(on) ime_callback(on=="yes") end
  morf.ipc.attached=function(on) attached=on=="yes" end
  morf.ipc.auto=function(on) require("config").set("keyboard.auto",on=="yes") end
  morf.ipc.numbers=function(on) keyboard.keys.numbers:set(on=="yes") end
  morf.ipc.clear=function() events={} end
  morf.ipc.state=function() return {open=keyboard.drawer.open:get(),mode=keyboard.keys.mode:get(),
    shift=keyboard.keys.shift:get(),page=keyboard.keys.page:get(),events=events,focus=morf.surface.keyboard_focus} end
]]

local function load(style)
  test.load("../shell/init.lua",{source=HOST,size={600,1000},env={
    CAELESTIA_STYLE=style,CAELESTIA_DRY_RUN="0",TEST_WIDTH="600",TEST_HEIGHT="1000"}})
  test.advance(2400)
end
local function state() return test.ipc("state") end
local function pair(x,y,dx,dy,dx2,dy2)
  test.touch("down",10,x,y) test.touch("down",11,x+60,y)
  test.advance(50)
  test.touch("move",10,x+dx,y+dy) test.touch("move",11,x+60+(dx2 or dx),y+(dy2 or dy))
  test.touch("up",10,x+dx,y+dy) test.touch("up",11,x+60+(dx2 or dx),y+(dy2 or dy))
  test.advance(1000)
end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." two fingers show from bottom, alternate anywhere on keys, and hide downward",function()
    load(style)
    pair(250,995,0,-110) test.truthy(state().open) test.eq(state().mode,"full")
    for _,mode in ipairs {"full","dev"} do
      local r=test.get("caelestia.osk.key."..mode..".letters.q")
      pair(r.x+10,r.y+r.height/2,0,-100)
      test.eq(state().mode,mode=="full" and "dev" or "full")
      test.eq(state().events,{})
    end
    local r=test.get("caelestia.osk.key.full.letters.q")
    pair(r.x+10,r.y+10,0,100) test.falsy(state().open) test.eq(state().events,{})
    test.eq(#test.logs("error"),0)
  end)
  test.it(style.." only bottom origins show, and short/pinching/cancelled contacts do nothing",function()
    load(style)
    pair(250,700,0,-120) test.falsy(state().open)
    pair(250,995,0,-20) test.falsy(state().open)
    pair(250,995,0,-100,0,0) test.falsy(state().open)
    test.touch("down",10,250,995) test.touch("down",11,310,995)
    test.touch("move",10,250,890) test.touch("move",11,310,890)
    test.touch("down",12,400,995)
    test.touch("up",10,250,890) test.touch("up",11,310,890) test.touch("up",12,400,995)
    test.advance(1000) test.falsy(state().open)
    pair(250,995,0,-110) test.truthy(state().open)
    local r=test.get("caelestia.osk.key.full.letters.q")
    pair(r.x+10,r.y+10,0,80,0,-80) test.eq(state().mode,"full") test.truthy(state().open)
    test.eq(state().events,{})
  end)
  test.it(style.." keyboard gestures cancel backspace and normal touch taps still type",function()
    load(style) test.ipc("show","full") test.advance(2400)
    local r=test.get("caelestia.osk.key.full.letters.backspace")
    test.touch("down",1,r.x+r.width/2,r.y+r.height/2)
    test.advance(100)
    test.touch("down",2,r.x-30,r.y+r.height/2)
    test.touch("move",1,r.x+r.width/2,r.y-100)
    test.touch("move",2,r.x-30,r.y-100)
    test.touch("up",1,r.x+r.width/2,r.y-100) test.touch("up",2,r.x-30,r.y-100)
    test.advance(1000) test.eq(state().events,{}) test.eq(state().mode,"dev")
    test.ipc("show","full") test.advance(1000)
    local q=test.get("caelestia.osk.key.full.letters.q")
    test.touch("down",0,q.x+10,q.y+10) test.touch("up",0,q.x+10,q.y+10)
    test.eq(state().events,{{code=16,on=true},{code=16,on=false}})
  end)
end
