local test=morf.test
local HOST=[[
  local ui=require("morf.ui")
  local W,H=tonumber(morf.env("TEST_WIDTH")),tonumber(morf.env("TEST_HEIGHT"))
  morf.surface.height=H
  morf.time.now=function() return 1790584200 end
  local prefs=morf.state {enabled="off",side="top",titles="on"}
  local original=require("config")
  package.loaded.config={get=function(key)
    local name=key:match("^edgebar%.(.+)$")
    if name then return prefs[name] end
    return original.get(key)
  end,set=function(key,value) prefs[key:gsub("^edgebar%.","")]=value end}
  local rev=morf.signal("bar.fixture.revision",0)
  local reads,calls=0,{}
  local data={available=true,wired=false,wifi_enabled=true,present=true,charging=false,percentage=64,
    locked=false,registered=true,signal=70,technology="5G",mode="vibrate",tor=true}
  local clients={
    {address="0xa",class="editor",initial_class="editor",title="Research notes",workspace=1,hidden=false},
    {address="0xb",class="terminal",initial_class="terminal",title="Build",workspace=1,hidden=false},
    {address="0xc",class="browser",initial_class="browser",title="Another workspace",workspace=2,hidden=false},
    {address="0xd",class="hidden",initial_class="hidden",title="Hidden",workspace=1,hidden=true}}
  local lists=morf.state {clients=clients,aps={{in_use=true,strength=65}}}
  local active=morf.state {address="0xa"}
  local workspace=morf.signal("bar.fixture.workspace",1)
  local service_state=setmetatable({},{__index=function(_,key)
    rev:get() reads=reads+1
    if key=="access_points" then return lists.aps end
    if key=="display" then return data end
    return data[key]
  end})
  local hypr={state={clients=lists.clients,active_window=active},dispatch=function(command,value)
    calls[#calls+1]={command,value} active.address=value:gsub("^address:","") return true
  end}
  package.loaded.services={net={state=service_state},modem={state=service_state},upower={state=service_state},
    ringer={state=service_state},tor={on=function() rev:get() reads=reads+1 return data.tor end},
    hyprland=hypr,workspace={active=function() return workspace:get() end}}
  package.loaded.apps={icon=function() return nil end}
  local function drawer(key)
    local open=morf.signal("bar.fixture."..key,false)
    return {open=open,set=function(on) open:set(on) end}
  end
  package.loaded.launcher={drawer=drawer("launcher")}
  package.loaded.dashboard={drawer=drawer("dashboard")}
  local tab=morf.signal("bar.fixture.tab","notifications")
  package.loaded.sidebar={drawer=drawer("sidebar"),select=function(key) tab:set(key) end}
  local detail=morf.signal("bar.fixture.detail","network")
  package.loaded.utilities={detail=detail}
  local model
  local ok,models=pcall(require,"bar_model")
  if ok then local create=models.new models.new=function(...) model=create(...) return model end end
  local bar=require("bar")
  ui.Item {width=W,height=H,ui.Rect {anchors={fill=true},color=function() return require("theme").color.surface end},bar.build()}
  morf.ipc.show=function(value) prefs.enabled=value end
  morf.ipc.side=function(value) bar.set_side(value) end
  morf.ipc.titles=function(value) prefs.titles=value end
  morf.ipc.color=function(value)
    require("theme").follow(value)
    return require("theme").color.primary:hex()
  end
  morf.ipc.workspace=function(value) workspace:set(tonumber(value)) end
  morf.ipc.focus=function(address) return model and model.focus(address) end
  morf.ipc.change=function(kind)
    if kind=="offline" then data.available=false data.registered=false data.present=false data.mode="sound" data.tor=false
    elseif kind=="charging" then data.charging=true data.percentage=84
    elseif kind=="remove" then table.remove(clients,1) lists.clients:replace(clients,"address")
    elseif kind=="many" then
      clients={} for i=1,24 do clients[i]={address="0x"..i,class="editor",initial_class="editor",title="Window "..i,workspace=1,hidden=false} end
      lists.clients:replace(clients,"address") active.address="0x24"
    elseif kind=="hidden-update" then data.percentage=31 data.signal=10
    end
    rev:set(rev:get()+1)
  end
  morf.ipc.state=function()
    return {reads=reads,calls=calls,on=bar.on(),side=bar.side(),insets=bar.insets(),desk={bar.desk()},
      reading=model and model.reading:get() or {},count=model and model.rows:len() or 0,
      launcher=package.loaded.launcher.drawer.open:get(),dashboard=package.loaded.dashboard.drawer.open:get(),
      sidebar=package.loaded.sidebar.drawer.open:get(),tab=tab:get(),detail=detail:get()}
  end
]]
local function load(style,w,h,dry)
  test.load("../shell/init.lua",{source=HOST,size={w or 1280,h or 800},env={CAELESTIA_STYLE=style,
    TEST_WIDTH=tostring(w or 1280),TEST_HEIGHT=tostring(h or 800),CAELESTIA_DRY_RUN=dry and "1" or "0"}})
  test.advance(100)
end
local function state() return test.ipc("state") end
local function show() test.ipc("show","on") test.advance(800) end
local function shot(name) if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(name..".png") end end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." bar preserves all four edges and routes its controls",function()
    load(style) show()
    local thick=style=="material" and 40 or 44
    local wide=style=="material" and 48 or 52
    for _,edge in ipairs {"top","bottom","left","right"} do
      test.ipc("side",edge) test.advance(800)
      local expected={left=0,top=0,right=0,bottom=0}
      expected[edge]=(edge=="top" or edge=="bottom") and thick or wide
      test.eq(state().insets,expected)
      local b=test.get("bar")
      test.truthy(b.x>=0 and b.y>=0 and b.x+b.width<=1280 and b.y+b.height<=800)
      shot(style.."-bar-"..edge)
    end
    test.ipc("side","top") test.advance(800)
    test.click("bar-window-0xb") test.eq(state().calls,{{"focuswindow","address:0xb"}})
    test.click("bar-logo") test.truthy(state().launcher)
    test.click("bar-clock") test.truthy(state().dashboard)
    test.click("bar-status") test.truthy(state().sidebar) test.eq(state().detail,"")
    test.ipc("show","off") test.advance(600)
    test.falsy(test.get("bar").visible)
    test.eq(state().desk,{0,0,1280,800})
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
  test.it(style.." bar caches hidden readings and rejects stale window actions",function()
    load(style)
    test.eq(state().reads,0)
    show() test.eq(state().count,2)
    test.eq(state().reading.percentage,"64%") test.eq(state().reading.network_icon,"network_wifi_3_bar")
    test.ipc("change","charging") test.advance(100)
    test.eq(state().reading.percentage,"84%") test.eq(state().reading.battery_icon,"battery_charging_full")
    test.ipc("workspace","2") test.advance(100) test.eq(state().count,1)
    test.falsy(test.ipc("focus","0xa")) test.eq(#state().calls,0)
    test.ipc("workspace","1") test.ipc("change","remove") test.advance(100)
    test.falsy(test.ipc("focus","0xa")) test.eq(#state().calls,0)
    test.ipc("show","off") test.advance(100)
    local reads=state().reads
    test.ipc("change","hidden-update") test.advance(65000)
    test.eq(state().reads,reads)
    test.falsy(test.ipc("focus","0xb"))
    show() test.eq(state().reading.percentage,"31%")
    test.ipc("change","offline") test.advance(100)
    test.eq(state().reading.network_icon,"wifi") test.eq(state().reading.battery_icon,"power")
    test.falsy(state().reading.tor) test.falsy(state().reading.ring)
    test.click("bar-status") test.eq(state().tab,"settings")
    test.eq(#test.logs("error"),0)
  end)
end
test.it("bar preview suppresses window focus commands",function()
  load("tsugumori",1280,800,true) show()
  test.click("bar-window-0xb") test.eq(#state().calls,0)
  test.eq(#test.logs("error"),0)
end)
test.it("bar auto mode and wallpaper colors remain independent of its visual theme",function()
  local accents={}
  for _,style in ipairs {"material","tsugumori"} do
    load(style,500,720)
    test.ipc("show","auto") test.advance(500) test.truthy(state().on)
    accents[style]=test.ipc("color","#00c5ae") test.advance(500)
    shot(style.."-bar-teal")
    load(style,1280,800)
    test.ipc("show","auto") test.advance(500) test.falsy(state().on)
  end
  test.eq(accents.material,accents.tsugumori)
end)
test.it("Tsugumori bar cancels interrupted entry and settles after reopening",function()
  load("tsugumori",500,720)
  test.ipc("show","on") test.advance(350)
  test.truthy(test.get("bar-horizontal-content").opacity<1)
  shot("tsugumori-bar-entry")
  test.ipc("side","left") test.advance(100)
  test.ipc("show","off") test.advance(1000)
  test.falsy(test.get("bar").visible)
  test.near(test.get("bar-vertical-content").opacity,1,.001)
  test.ipc("show","on") test.advance(1200)
  test.near(test.get("bar-vertical-content").opacity,1,.001)
  test.near(test.get("bar-vertical-content").y,12,.001)
  shot("tsugumori-bar-reopened")
  test.eq(#test.logs("error"),0)
end)
test.it("Tsugumori compact bar keeps the final window reachable without overlapping controls",function()
  load("tsugumori",500,720) show()
  test.ipc("change","many") test.advance(900)
  local lane=test.get("bar-windows")
  local clock=test.get("bar-clock") local status=test.get("bar-status")
  test.truthy(lane.x+lane.width<=clock.x)
  test.truthy(clock.x+clock.width<=status.x)
  test.truthy(status.x+status.width<=500)
  local last=test.get("bar-window-0x24")
  test.truthy(last.x>=lane.x and last.x+last.width<=lane.x+lane.width+.1)
  test.click("bar-window-0x24") test.eq(state().calls[1],{"focuswindow","address:0x24"})
  shot("tsugumori-bar-compact")
  test.wheel(0,-1000,{x=lane.x+lane.width-2,y=lane.y+18}) test.advance(300)
  local first=test.get("bar-window-0x1")
  test.truthy(first.x>=lane.x and first.x+first.width<=lane.x+lane.width+.1)
  test.ipc("side","left") test.advance(900)
  lane=test.get("bar-windows-v") clock=test.get("bar-clock-v") status=test.get("bar-status-v")
  test.truthy(lane.y+lane.height<=clock.y)
  test.truthy(clock.y+clock.height<=status.y)
  test.truthy(status.y+status.height<=720)
  last=test.get("bar-window-0x24-v")
  test.truthy(last.y>=lane.y and last.y+last.height<=lane.y+lane.height+.1)
  test.click("bar-window-0x24-v") test.eq(#state().calls,2)
  shot("tsugumori-bar-compact-vertical")
  test.wheel(0,-2000,{x=lane.x+lane.width-2,y=lane.y+20}) test.advance(300)
  first=test.get("bar-window-0x1-v")
  test.truthy(first.y>=lane.y and first.y+first.height<=lane.y+lane.height+.1)
  test.eq(#test.logs("error"),0)
end)
