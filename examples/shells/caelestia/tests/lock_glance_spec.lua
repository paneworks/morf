-- Clock-first locking: incidental input must not reveal account controls.
local test=morf.test
local SOURCE=[[
  package.loaded["lib.services.accounts"]={me=function() return {name="fixture",label="Fixture",initial="F"} end}
  package.loaded["lib.services.keyboards"]={attached=function() return morf.env("LOCK_DESKTOP")=="1" end}
  package.loaded["lib.services.mpris"]={connect=function() return {state={active={}}} end}
  local weather=morf.signal("fixture.weather",{temperature=18.2,code=0,is_day=true})
  package.loaded["lib.integrations.weather"]={new=function() return {get=function() return weather:get() end} end,
    material_symbol=function() return "sunny" end}
  local ctx
  local keyboard=require("themes.auth_keyboard") local make=keyboard.new local board,options
  keyboard.new=function(config) options=config board=make(config) return board end
  local theme=require("themes").current.lock local build=require(theme)
  package.loaded[theme]=function(context) ctx=context return build(context) end
  require("init")
  morf.ipc.audit=function(action)
    if action=="password" then ctx.method:set("password") end
    if action=="no-weather" then weather:set({}) end
    if action=="hide-keyboard" then board.hide() end
    return {stage=ctx.stage:get(),typed=ctx.typed:get()}
  end
  morf.ipc.key_id=function(key) return options.prefix..".osk."..options.output..".key.full.letters."..key end
]]
local function load(style,w,h,desktop)
  test.load("../lock/init.lua",{source=SOURCE,size={w,h},args={"window","preview"},
    env={CAELESTIA_STYLE=style,CAELESTIA_SCALE_MODE="compositor",CAELESTIA_DRY_RUN="1",
      LOCK_DESKTOP=desktop and "1" or "0"}})
  test.advance(900)
end
local function at_rest()
  test.eq(test.ipc("audit"),{stage="rest",typed=0})
  test.truthy(test.get("lock-clock").visible)
  for _,id in ipairs {"lock-sheet","lock-name","lock-field","lock-keyboard"} do
    test.falsy(test.get(id).visible,id.." appeared before the upward gesture")
  end
end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." touch-only lock ignores taps and wake keys until swiped upward",function()
    for _,size in ipairs {{406,903},{1920,1080}} do
      local w,h=size[1],size[2]
      load(style,w,h,false) at_rest()
      test.click(w/2,h*.6)
      test.touch("down",1,w/2,h*.6) test.touch("up",1,w/2,h*.6)
      test.key("Super_L") test.key("Return") test.type("wake")
      test.advance(500) at_rest()
      test.swipe({w/2,h*.6},{w/2+80,h*.6},{duration=250})
      test.swipe({w/2,h*.6},{w/2,h*.6+80},{duration=250})
      test.touch("down",1,w/2,h*.6) test.touch("move",1,w/2,h*.6-80)
      test.touch("cancel",1,w/2,h*.6-80)
      test.advance(500) at_rest()
      test.swipe({w/2,h*.6},{w/2,h*.6-160},{duration=300})
      test.advance(700)
      test.eq(test.ipc("audit").stage,"sheet")
      test.truthy(test.get("lock-name").visible)
      test.ipc("audit","password") test.type("x")
      test.eq(test.ipc("audit").typed,1)
      local handle=test.get("lock-sheet-handle")
      test.swipe({handle.x+handle.width/2,handle.y+8},{handle.x+handle.width/2,handle.y+108},{duration=300})
      test.advance(700) at_rest()
      test.eq(test.logs("error"),{})
    end
  end)
  test.it(style.." laptop typing reveals the password form without losing the first character",function()
    load(style,1920,1080,true) at_rest()
    test.key("Super_L") test.key("Shift_L") test.key("BackSpace")
    test.advance(100) at_rest()
    test.type("w") test.advance(300)
    test.eq(test.ipc("audit"),{stage="sheet",typed=1})
    test.type("rong") test.key("Return") test.advance(900)
    test.eq(test.ipc("audit"),{stage="sheet",typed=0})
    test.truthy(test.get("lock-message").text:find("Wrong",1,true),"first character was lost before authentication")
    test.click(960,100) test.advance(700) at_rest()
    test.key("Return") test.advance(300)
    test.eq(test.ipc("audit"),{stage="sheet",typed=0})
    test.eq(test.logs("error"),{})
  end)
  test.it(style.." laptop lock reveals by upward drag or deliberate upward scroll",function()
    load(style,1920,1080,true)
    test.click(960,640) test.key("Super_L") test.advance(400) at_rest()
    test.drag({960,640},{1120,640},{steps=10}) test.advance(400) at_rest()
    test.drag({960,640},{960,480},{steps=10}) test.advance(700)
    test.eq(test.ipc("audit").stage,"sheet")
    test.click("lock-name") test.click("lock-field")
    local sheet=test.get("lock-sheet")
    test.click(sheet.x+4,sheet.y+sheet.height-4)
    test.eq(test.ipc("audit").stage,"sheet","click inside the card dismissed it")
    test.ipc("audit","password") test.type("private")
    test.click(960,100) test.advance(700) at_rest()
    test.wheel(0,120,{x=960,y=640}) at_rest()
    test.wheel(120,0,{x=960,y=640}) at_rest()
    test.wheel(0,-10,{x=960,y=640}) at_rest()
    test.advance(500)
    test.wheel(0,-10,{x=960,y=640}) at_rest()
    test.wheel(0,-80,{x=960,y=640}) test.advance(700)
    test.eq(test.ipc("audit").stage,"sheet")
    test.truthy(test.get("lock-name").visible)
    test.eq(test.logs("error"),{})
  end)
  test.it(style.." lock returns to its clock after an outside tap or hiding the keyboard",function()
    load(style,406,903,false)
    local function reveal()
      test.swipe({203,600},{203,440},{duration=300}) test.advance(700)
      test.ipc("audit","password") test.advance(100)
    end
    local function tap(id)
      local node=test.get(id) local x,y=node.x+node.width/2,node.y+node.height/2
      test.touch("down",1,x,y) test.touch("up",1,x,y)
    end
    reveal()
    tap("lock-name") tap("lock-field")
    tap(test.ipc("key_id","q"))
    test.eq(test.ipc("audit"),{stage="sheet",typed=1},"typing dismissed the card")
    test.touch("down",1,203,80) test.touch("up",1,203,80)
    test.advance(700) at_rest()
    reveal() tap(test.ipc("key_id","q"))
    local key=test.get(test.ipc("key_id","q")) local x,y=key.x+10,key.y+10
    test.touch("down",1,x,y) test.touch("down",2,x+60,y)
    test.touch("move",1,x,y+100) test.touch("move",2,x+60,y+100)
    test.touch("up",1,x,y+100) test.touch("up",2,x+60,y+100)
    test.advance(700) at_rest()
    reveal() tap(test.ipc("key_id","q"))
    test.ipc("audit","hide-keyboard") test.advance(700) at_rest()
    test.eq(test.logs("error"),{})
  end)
  test.it(style.." weather is centered above the clock at phone and laptop sizes",function()
    for _,size in ipairs {{360,800},{744,1656},{1920,1080}} do
      local w,h=size[1],size[2]
      load(style,w,h,w>h)
      local weather,clock,date=test.get("lock-weather"),test.get("lock-clock"),test.get("lock-date")
      test.truthy(weather.visible)
      test.truthy(math.abs(weather.x+weather.width/2-w/2)<=1,"weather drifted away from center")
      test.truthy(weather.y+weather.height<=clock.y,"weather overlaps the clock")
      test.truthy(clock.y+clock.height<=date.y,"date overlaps the clock")
      if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(style.."-lock-glance-"..w..".png") end
      test.ipc("audit","no-weather") test.advance(100)
      test.falsy(test.get("lock-weather").visible)
      test.truthy(test.get("lock-clock").visible)
      test.eq(test.logs("error"),{})
    end
  end)
end
