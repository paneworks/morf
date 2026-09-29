local test=morf.test
local HOST=[[
  local ui=require("morf.ui")
  local W,H=tonumber(morf.env("TEST_WIDTH")),tonumber(morf.env("TEST_HEIGHT"))
  morf.surface.height=H
  local page=morf.env("TEST_PAGE")
  local shown=morf.signal("powerbar.fixture.shown",false)
  local revision=morf.signal("powerbar.fixture.revision",0)
  local reads,profile_reads,handoffs=0,0,0
  local calls,writes={},{}
  local fail=false
  local battery={ac=false,time_left=8100,time_to_full=3600,batteries={{name="BAT0",capacity=64,
    status="Discharging",power=12.4,health=92,charge_start=40,charge_limit=80,charge_mode="Standard",cycles=185,temperature=31.5}}}
  package.loaded["lib.sysinfo"]={history_size=60,sources={battery={interval=5000}},
    battery=function() revision:get() reads=reads+1 return battery end}
  local profiles=morf.state {available=true,active="balanced",degraded="",list={
    {name="power-saver"},{name="balanced"},{name="performance"}}}
  package.loaded.services={upower={state=setmetatable({available=true},{__index=function(_,key)
    if key=="profiles" then profile_reads=profile_reads+1 return profiles end
  end}),set_profile=function(value)
    calls[#calls+1]=value
    if fail then return nil,"Profile permission denied" end
    profiles.active=value return true
  end}}
  local prefs=morf.state {enabled="auto",side="top",titles="on"}
  local config=require("config")
  local function get(key)
    if key=="edgebar.enabled" or key=="edgebar.side" or key=="edgebar.titles" then return prefs[key:gsub("^edgebar%.","")] end
    return config.get(key)
  end
  local function set(key,value) writes[#writes+1]={key,value} prefs[key:gsub("^edgebar%.","")]=value end
  package.loaded.config={get=get,set=set}
  package.loaded.bar={side=function() return prefs.side end,set_side=function(value) set("edgebar.side",value) end}
  package.loaded.dashboard_battery={show=function() handoffs=handoffs+1 end}
  local model
  local ok,models=pcall(require,page=="power" and "power_model" or "bar_settings_model")
  if ok then
    local create=models.new
    models.new=function(...) model=create(...) return model end
  end
  local content=require(page.."_page").page(W,function() return H end)
  ui.Item {width=W,height=H,visible=function() return shown:get() end,content}
  morf.ipc.show=function(on)
    shown:set(on=="yes") require("presentation").set("settings."..page,shown:get())
  end
  morf.ipc.state=function() return {reads=reads,profile_reads=profile_reads,calls=calls,writes=writes,handoffs=handoffs,
    profile=profiles.active,message=model and model.message and model.message:get() or "",
    mode=prefs.enabled,side=prefs.side,titles=prefs.titles} end
  morf.ipc.change=function(kind)
    if kind=="missing" then battery={batteries={}}
    elseif kind=="unavailable" then profiles.available=false
    elseif kind=="unsupported" then profiles.list:replace({{name="balanced"},{name="power-saver"}},"name")
    elseif kind=="degraded" then profiles.degraded="High temperature"
    elseif kind=="failure" then fail=true
    elseif kind=="battery-daemon-off" then package.loaded.services.upower.state.available=false
    elseif kind=="charging" then battery.batteries[1].status="Charging" battery.ac=true end
    revision:set(revision:get()+1)
  end
  morf.ipc.action=function(kind,value)
    if page=="power" then
      if kind=="graphs" then return model.open_battery() end
      return model.select(value)
    end
    return ({mode=model.set_mode,side=model.set_side,titles=model.set_titles})[kind](value)
  end
]]
local function load(style,page,w,h,dry)
  w,h=w or 408,h or 1000
  test.load("../shell/init.lua",{source=HOST,size={w,h},env={CAELESTIA_STYLE=style,
    TEST_PAGE=page,TEST_WIDTH=tostring(w),TEST_HEIGHT=tostring(h),CAELESTIA_DRY_RUN=dry and "1" or "0"}})
end
local function open() test.ipc("show","yes") test.advance(2500) end
local function shot(name) if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(name..".png") end end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." Power preserves readings, profiles and graph handoff",function()
    load(style,"power") open()
    test.eq(test.get("power-percent").text,"64%")
    test.truthy(test.find {text="Discharging, 2h 15m left  ·  12.4 W"})
    shot(style.."-power")
    -- Profiles can be offered even when UPower has no battery service.
    test.ipc("change","battery-daemon-off")
    test.click("power-profile-performance") test.advance(100)
    test.eq(test.ipc("state").calls,{"performance"})
    test.eq(test.ipc("state").profile,"performance")
    test.click("power-open-battery") test.eq(test.ipc("state").handoffs,1)
    test.ipc("change","charging") test.advance(100)
    test.truthy(test.find {text="Charging, full in 1h 00m  ·  12.4 W"})
    test.ipc("change","missing") test.advance(100)
    test.eq(test.get("power-percent").text,style=="material" and "0%" or "--")
    test.truthy(test.find {text="No battery"})
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
  test.it(style.." Bar settings dispatch validated preference changes",function()
    load(style,"bar") open() shot(style.."-bar-settings")
    test.click("bar-show-on") test.advance(100)
    test.eq(test.ipc("state").mode,"on")
    test.click("bar-side-right") test.advance(400)
    test.eq(test.ipc("state").side,"right")
    if style=="tsugumori" then test.near(test.get("bar-preview-right").opacity,1,.001) end
    test.click("bar-titles-off") test.eq(test.ipc("state").titles,"off")
    local count=#test.ipc("state").writes
    test.falsy(test.ipc("action","side","diagonal"))
    test.falsy(test.ipc("action","mode","unknown"))
    test.ipc("show","no") test.ipc("action","titles","on")
    test.eq(#test.ipc("state").writes,count)
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
  test.it(style.." Power suppresses hidden reads and dry-run service actions",function()
    load(style,"power",nil,nil,true)
    test.advance(100)
    test.eq(test.ipc("state").reads,0) test.eq(test.ipc("state").profile_reads,0)
    open()
    test.click("power-profile-performance") test.eq(test.ipc("state").calls,{})
    test.ipc("show","no") test.advance(100)
    local before=test.ipc("state")
    test.ipc("change","charging") test.ipc("change","degraded") test.advance(100)
    test.ipc("action","select","power-saver") test.ipc("action","graphs")
    local after=test.ipc("state")
    test.eq(after.reads,before.reads) test.eq(after.profile_reads,before.profile_reads)
    test.eq(after.calls,{}) test.eq(after.handoffs,0)
    open() test.truthy(test.ipc("state").reads>after.reads)
    test.eq(#test.logs("error"),0)
  end)
end
test.it("Power rejects removed modes and exposes service errors without inventing a profile",function()
  load("tsugumori","power") open()
  test.ipc("change","failure") test.click("power-profile-performance") test.advance(100)
  test.eq(test.ipc("state").message,"Profile permission denied")
  test.eq(test.ipc("state").profile,"balanced")
  test.ipc("change","unsupported") test.click("power-profile-performance")
  test.eq(#test.ipc("state").calls,1)
  test.ipc("change","degraded") test.advance(100)
  test.eq(test.get("power-profile-note").text,"Performance limited: High temperature")
  test.ipc("change","unavailable") test.click("power-profile-power-saver")
  test.eq(#test.ipc("state").calls,1)
  test.advance(100)
  test.eq(test.get("power-profile-note").text,"Power profile service is unavailable.")
  test.ipc("show","no") test.advance(100)
  test.eq(test.ipc("state").message,"")
  test.eq(#test.logs("error"),0)
end)
for _,page in ipairs {"power","bar"} do
  test.it("Tsugumori compact "..page.." reaches the last control and replays section titles",function()
    load("tsugumori",page,360,420) open()
    shot(page.."-compact-top")
    local viewport=test.get(page.."-scroll")
    test.wheel(0,3000,{x=viewport.x+viewport.width-2,y=viewport.y+220}) test.advance(400)
    local id=page=="power" and "power-open-battery" or "bar-titles-off"
    local last=test.get(id)
    test.truthy(last.y>=viewport.y and last.y+last.height<=viewport.y+viewport.height)
    local title=page=="power" and "power-heading-battery-text" or "bar-heading-window-titles-text"
    local expected=page=="power" and "BATTERY" or "WINDOW TITLES"
    test.truthy(test.get(title).text~=expected)
    test.advance(2200) test.eq(test.get(title).text,expected)
    shot(page.."-compact-bottom")
    test.click(id)
    if page=="power" then test.eq(test.ipc("state").handoffs,1) else test.eq(test.ipc("state").titles,"off") end
    test.ipc("show","no") test.advance(50) test.ipc("show","yes") test.advance(500)
    local first=test.get(page=="power" and "power-summary" or "bar-show")
    test.near(first.y,viewport.y,1)
    test.advance(2200)
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
end
