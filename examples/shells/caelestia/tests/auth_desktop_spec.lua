local test=morf.test
local HOST=[[
  local weather=morf.signal("fixture.weather",{temperature=18.2,code=0,is_day=true,condition="Clear"})
  local player=morf.signal("fixture.player",{title="Morning signal",artist="Preview ensemble",playing=false,art_url=""})
  local reads={weather=0,media=0,art=0,weather_new=0,media_new=0}
  local actions={}
  package.loaded["lib.weather"]={new=function()
    reads.weather_new=reads.weather_new+1
    return {get=function() reads.weather=reads.weather+1 return weather:get() end}
  end,material_symbol=function() return "sunny" end}
  package.loaded["lib.mpris"]={connect=function()
    reads.media_new=reads.media_new+1
    return {state=setmetatable({},{__index=function(_,key)
      if key=="active" then reads.media=reads.media+1 return player:get() end
    end}),previous=function() actions[#actions+1]="previous" end,
      play_pause=function() actions[#actions+1]="play_pause" end,next=function() actions[#actions+1]="next" end}
  end}
  package.loaded["lib.remote"]={file=function() reads.art=reads.art+1 return "" end}
  package.loaded["lib.keyboards"]={attached=function() return true end}
  local person={name="preview",label="Preview User",initial="P",face=""}
  package.loaded["lib.accounts"]={me=function() return person end,list=function() return {person} end}
  package.loaded["lib.sessions"]={list=function() return {{name="Desktop",command={"false"}}} end,default_index=function() return 1 end}
  local part=morf.env("TEST_AUTH_PART")
  local name=require("themes").current[part]
  local build=require(name)
  local state
  package.loaded[name]=function(ctx)
    state=ctx
    ctx.clock={get=function() return "08:42" end}
    ctx.day={get=function() return "Monday, 28 September" end}
    return build(ctx)
  end
  require("init")
  morf.ipc.feed=function()
    weather:set({temperature=24,code=0,is_day=true,condition="Sunny"})
    player:set({title="Afternoon signal",artist="Second ensemble",playing=false,art_url=""})
  end
  morf.ipc.source_stage=function(value) state.stage:set(value) end
  morf.ipc.audit=function() return {reads=reads,actions=actions,hostname=state.hostname} end
]]
local function load(style,part,w,h,dry)
  test.load("../"..part.."/init.lua",{source=HOST,size={w or 1920,h or 1080},
    args=part=="lock" and {"window","preview"} or {"preview"},env={CAELESTIA_STYLE=style,
      CAELESTIA_DRY_RUN=dry and "1" or "0",TEST_AUTH_PART=part,GREETD_SOCK=false,LULE_A="/nonexistent/lule"}})
  test.advance(2600)
end
local function shot(name) if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(name..".png") end end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." lock weather and media share controller data and playback actions",function()
    load(style,"lock")
    test.truthy(test.find {text=style=="material" and "Morning signal" or "MORNING SIGNAL",visible=true})
    test.truthy(test.find {text="18°",visible=true})
    shot(style.."-lock-desktop-rest")
    test.click("lock-media-previous") test.click("lock-media-play") test.click("lock-media-next")
    test.eq(test.ipc("audit").actions,{"previous","play_pause","next"})
    test.eq(test.ipc("audit").reads.weather_new,1) test.eq(test.ipc("audit").reads.media_new,1)
    test.ipc("feed") test.advance(2400)
    test.truthy(test.find {text="24°",visible=true})
    test.truthy(test.find {text=style=="material" and "Afternoon signal" or "AFTERNOON SIGNAL",visible=true})
    test.ipc("stage","sheet") test.advance(2600)
    shot(style.."-lock-desktop-sheet")
    test.ipc("source_stage","closed") test.advance(700)
    local before=test.ipc("audit").reads
    test.ipc("feed") test.advance(400)
    test.eq(test.ipc("audit").reads,before,"hidden lock kept reading sources")
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
  test.it(style.." greeter receives host identity without starting lock readers",function()
    load(style,"greet")
    shot(style.."-greeter-desktop-rest")
    test.ipc("stage","sheet") test.advance(2600)
    shot(style.."-greeter-desktop-sheet")
    test.eq(test.ipc("audit").reads.weather_new,0) test.eq(test.ipc("audit").reads.media_new,0)
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
end
test.it("lock desktop model shares readers across outputs and guards stale and dry actions",function()
  test.load("../lock/init.lua",{env={CAELESTIA_DRY_RUN="0"},source=[[
    local active=morf.signal("fixture.active",true)
    local primary=morf.signal("fixture.primary","A")
    local weather_new,media_new,commands,art=0,0,0,0
    package.loaded["lib.weather"]={new=function() weather_new=weather_new+1 return {get=function() return {temperature=8} end} end,
      material_symbol=function() return "cloud" end}
    package.loaded["lib.mpris"]={connect=function() media_new=media_new+1 return {state={active={title="Track",art_url="sample"}},
      next=function() commands=commands+1 end} end}
    package.loaded["lib.remote"]={file=function() art=art+1 return "cached.png" end}
    local model=require("models.lock_desktop").new {active=function() return active:get() end,primary=function(name) return primary:get()==name end}
    local a,b=model.for_output("A"),model.for_output("B")
    morf.ipc.read=function() a.weather() b.weather() a.player() b.player() a.artwork() b.artwork() end
    morf.ipc.control=function(output,action) return (output=="A" and a or b).control(action) end
    morf.ipc.hide=function() active:set(false) end
    morf.ipc.primary=function() primary:set("B") end
    morf.ipc.state=function() return {weather=weather_new,media=media_new,commands=commands,art=art} end
  ]]})
  test.eq(test.ipc("state"),{weather=0,media=0,commands=0,art=0})
  test.ipc("read") test.eq(test.ipc("state"),{weather=1,media=1,commands=0,art=1})
  test.falsy(test.ipc("control","B","next")) test.truthy(test.ipc("control","A","next"))
  test.falsy(test.ipc("control","A","connect"))
  test.ipc("primary") test.falsy(test.ipc("control","A","next")) test.truthy(test.ipc("control","B","next"))
  test.ipc("hide") test.ipc("read") test.falsy(test.ipc("control","B","next"))
  test.eq(test.ipc("state"),{weather=1,media=1,commands=2,art=1})
  test.eq(#test.logs("error"),0)
end)
test.it("Tsugumori compact lock keeps the weather and playback register within the output",function()
  load("tsugumori","lock",500,720,true)
  local panel=test.get("lock-desktop")
  test.truthy(panel.x>=0 and panel.x+panel.width<=500)
  test.truthy(panel.y>=0 and panel.y+panel.height<=720)
  test.click("lock-media-play") test.eq(test.ipc("audit").actions,{})
  shot("tsugumori-lock-desktop-compact")
  test.ipc("stage","sheet") test.advance(2600)
  test.falsy(test.get("lock-desktop").visible)
  test.ipc("stage","rest") test.advance(900)
  test.truthy(test.get("lock-media-title-text").text~="MORNING SIGNAL")
  test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
end)
test.it("unavailable lock readers remain optional and failed media connections are not repeated",function()
  test.load("../lock/init.lua",{source=[[
    local calls=0
    package.loaded["lib.weather"]={new=function() return {get=function() return nil end} end,
      material_symbol=function() return "cloud" end}
    package.loaded["lib.mpris"]={connect=function() calls=calls+1 error("No preview bus") end}
    local model=require("models.lock_desktop").new {active=function() return true end,primary=function() return true end}
    local page=model.for_output("A")
    morf.ipc.read=function()
      return {weather=page.weather(),player=page.player(),available=page.media_available(),
        sent=page.control("play_pause"),calls=calls}
    end
  ]]})
  for _=1,3 do test.eq(test.ipc("read"),{weather={},player={},available=false,sent=false,calls=1}) end
  test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
end)
