local test=morf.test
local function recognizer()
  test.load("../lock/init.lua",{source=[[
    local clock,wakes,allowed=0,0,true
    local g=require("themes.double_tap").new {now=function() return clock end,
      allowed=function() return allowed end,wake=function() wakes=wakes+1 end}
    morf.ipc.event=function(raw)
      local args=morf.json.decode(raw) local op=table.remove(args,1)
      if op=="advance" then clock=clock+args[1]
      elseif op=="allowed" then allowed=args[1]
      else g[op](table.unpack(args)) end
      return wakes
    end
    morf.ipc.wakes=function() return wakes end
  ]],size={20,20}})
  local function event(op,...) return test.ipc("event",morf.json.encode({op,...})) end
  local g={}
  for _,method in ipairs {"down","move","up","cancel","reset"} do
    g[method]=function(...) return event(method,...) end
  end
  local function advance(ms) event("advance",ms) end
  local function tap(x,y,duration)
    g.down(1,x or 100,y or 100) advance(duration or 80) g.up(1,x or 100,y or 100)
  end
  return g,advance,tap,function() return test.ipc("wakes") end,function(value) event("allowed",value) end
end
test.it("two released nearby taps wake once; a single tap never wakes",function()
  local g,advance,tap,wakes=recognizer()
  tap() test.eq(wakes(),0) advance(120) tap(110,100) test.eq(wakes(),1)
  advance(50) tap() advance(80) tap() test.eq(wakes(),1)
end)
test.it("rejects long, distant, delayed and bouncing taps",function()
  for _,kind in ipairs {"held","distant","late","bounce","too-short"} do
    local g,advance,tap,wakes=recognizer()
    tap() advance(kind=="late" and 500 or kind=="bounce" and 10 or 100)
    tap(kind=="distant" and 250 or 100,100,kind=="held" and 700 or kind=="too-short" and 5 or 80)
    test.eq(wakes(),0,kind)
  end
end)
test.it("rejects swipes even if the finger returns to its original position",function()
  local g,advance,tap,wakes=recognizer()
  tap() advance(100) g.down(1,100,100) advance(60)
  g.move(1,100,50) g.move(1,100,100) g.up(1,100,100)
  test.eq(wakes(),0) advance(100) tap() test.eq(wakes(),0)
end)
test.it("rejects multiple fingers, cancellation and input held across screen power changes",function()
  for _,kind in ipairs {"multi","cancel","power"} do
    local g,advance,tap,wakes=recognizer()
    tap() advance(100) g.down(1,100,100) advance(60)
    if kind=="multi" then g.down(2,120,100) g.up(2,120,100)
    elseif kind=="cancel" then g.cancel(1)
    else g.reset() advance(500) end
    g.up(1,100,100) test.eq(wakes(),0,kind)
    advance(500) tap() test.eq(wakes(),0,kind.." left a stale first tap")
    advance(100) tap() test.eq(wakes(),1,kind.." did not recover")
  end
end)
test.it("only recognises taps while asleep and after the blanking cooldown",function()
  local g,advance,tap,wakes,set_allowed=recognizer()
  set_allowed(false) tap() advance(100) tap() test.eq(wakes(),0)
  set_allowed(true) g.reset() tap() advance(100) tap() test.eq(wakes(),0)
  advance(400) tap() advance(100) tap() test.eq(wakes(),1)
end)

local HOST=[[
  local ui=require("morf.ui")
  local file=morf.env("XDG_RUNTIME_DIR").."/phone-screen-"..morf.env("HYPRLAND_INSTANCE_SIGNATURE")..".status"
  local status,changed,wakes,clicks,drags,rests="on",nil,0,0,0,0
  local clock=0
  morf.time.now_ms=function() return clock end
  morf.ipc.tick=function(ms) clock=clock+tonumber(ms) end
  local read,watch,run=morf.fs.read,morf.fs.watch,morf.run
  morf.fs.read=function(path)
    if path=="/fixture-phone-screen.json" then return '{"command":"/fixture/phone-screen","doubleTap":true}' end
    if path==file then return status.."\n" end
    return read(path)
  end
  morf.fs.watch=function(path,callback,...)
    if path==file then changed=callback return {} end
    return watch(path,callback,...)
  end
  morf.run=function(argv,options,callback)
    if argv[1]=="/fixture/phone-screen" then
      assert(argv[2]=="wake") wakes=wakes+1 callback({ok=true}) return
    end
    return run(argv,options,callback)
  end
  morf.surface.width,morf.surface.height=400,900
  local root=ui.Rect {width=400,height=900,color="#202020",
    ui.MouseArea {anchors={fill=true},on_clicked=function() clicks=clicks+1 end,
      on_dragged=function() drags=drags+1 end}}
  require("themes.phone_wake").attach(root,{prefix="fixture",width=400,height=900,
    rest=function() rests=rests+1 end})
  morf.ipc.power=function(value) status=value changed() end
  morf.ipc.audit=function() return {wakes=wakes,clicks=clicks,drags=drags,rests=rests} end
]]
test.it("sleeping shield consumes touches and releases normal input after waking",function()
  local function advance(ms) test.ipc("tick",tostring(ms)) test.advance(ms) end
  test.load("../lock/init.lua",{source=HOST,size={400,900},env={
    MORF_PHONE_SCREEN_CONFIG="/fixture-phone-screen.json",XDG_RUNTIME_DIR="/tmp",
    HYPRLAND_INSTANCE_SIGNATURE="fixture",CAELESTIA_DRY_RUN="0"}})
  advance(100)
  test.click(200,450) test.eq(test.ipc("audit").clicks,1)
  test.ipc("power","off") advance(500)
  test.truthy(test.get("fixture-wake-shield").visible)
  test.swipe({200,600},{200,300},{duration=300})
  test.click(200,450) test.eq(test.ipc("audit"),{wakes=0,clicks=1,drags=0,rests=1})
  test.touch("down",1,200,450) advance(80) test.touch("up",1,200,450)
  test.eq(test.ipc("audit").wakes,0)
  advance(120)
  test.touch("down",1,204,453) advance(80) test.touch("up",1,204,453)
  test.eq(test.ipc("audit"),{wakes=1,clicks=1,drags=0,rests=1})
  test.ipc("power","on") advance(100)
  test.falsy(test.get("fixture-wake-shield").visible)
  test.click(200,450) test.eq(test.ipc("audit").clicks,2)
  test.eq(test.logs("error"),{})
end)

for _,role in ipairs {"lock","greet"} do
  test.it(role.." sleeps and wakes at the clock with no hidden keyboard input",function()
    local setup=HOST:match("^(.-)  morf.surface.width")
    local source=setup..[[
      local person={name="fixture",label="Fixture",initial="F"}
      package.loaded["lib.services.accounts"]={me=function() return person end,list=function() return {person} end}
      package.loaded["lib.services.keyboards"]={attached=function() return false end}
      local role=morf.env("TEST_ROLE")
      local theme=require("themes").current[role]
      local build,ctx=require(theme),nil
      package.loaded[theme]=function(context) ctx=context return build(context) end
      require("init")
      morf.ipc.power=function(value) status=value changed() end
      morf.ipc.draft=function() ctx.open_sheet() ctx.method:set("password") ctx.type_text("draft") end
      morf.ipc.audit=function() return {stage=ctx.stage:get(),typed=ctx.typed:get(),wakes=wakes} end
    ]]
    test.load("../"..role.."/init.lua",{source=source,size={400,900},args={"window","preview"},env={
      TEST_ROLE=role,CAELESTIA_STYLE="tsugumori",CAELESTIA_DRY_RUN="0",
      MORF_PHONE_SCREEN_CONFIG="/fixture-phone-screen.json",XDG_RUNTIME_DIR="/tmp",
      HYPRLAND_INSTANCE_SIGNATURE="fixture"}})
    test.advance(900)
    test.ipc("draft") test.advance(700)
    test.truthy(test.get(role.."-keyboard").visible)
    test.ipc("power","off") test.advance(700)
    test.eq(test.ipc("audit"),{stage="rest",typed=0,wakes=0})
    test.truthy(test.get(role.."-wake-shield").visible)
    test.swipe({200,650},{200,300},{duration=300})
    test.touch("down",1,200,600) test.touch("up",1,200,600)
    test.eq(test.ipc("audit"),{stage="rest",typed=0,wakes=0})
    test.ipc("power","on") test.advance(700)
    test.falsy(test.get(role.."-wake-shield").visible)
    test.falsy(test.get(role.."-keyboard").visible)
    test.eq(test.ipc("audit"),{stage="rest",typed=0,wakes=0})
    test.swipe({200,650},{200,300},{duration=300}) test.advance(700)
    test.eq(test.ipc("audit").stage,"sheet")
    test.eq(test.logs("error"),{})
  end)
end
