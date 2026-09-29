local test=morf.test
local HOST=[[
  local ui=require("morf.ui")
  local kind=morf.env("TEST_KIND")
  local W,H=tonumber(morf.env("TEST_WIDTH")),tonumber(morf.env("TEST_HEIGHT"))
  morf.surface.height=H
  local shown=morf.signal("net.fixture.shown",false)
  local revision=morf.signal("net.fixture.revision",0)
  local available=true
  local reads,calls,pending,watches,releases=0,{},{},0,0
  local deferred=false
  local devices={
    {path="/device/1",interface="eth0",type="ethernet",state="activated",connection="Campus",ip4="10.20.30.4",speed=1000,hw_address="AA:BB:01",carrier=true},
    {path="/device/2",interface="enp1s0",type="ethernet",state="disconnected",connection="",ip4="",speed=100,hw_address="AA:BB:02",carrier=true},
    {path="/device/3",interface="dock0",type="ethernet",state="unavailable",connection="",ip4="",speed=0,hw_address="AA:BB:03",carrier=false},
  }
  local connections={
    {uuid="campus",id="Campus VPN",type="vpn",active=true},
    {uuid="personal",id="Personal",type="wireguard",active=false},
    {uuid="mesh-link",id="tailscale0",type="wireguard",active=true},
  }
  local lists=morf.state {devices={},vpn_connections={}}
  local function update()
    lists.devices:replace(devices,"path") lists.vpn_connections:replace(connections,"uuid") revision:set(revision:get()+1)
  end
  update()
  local network={state=setmetatable({},{__index=function(_,key)
    revision:get() reads=reads+1
    if key=="available" then return available end
    return lists[key]
  end}),snapshot=function() revision:get() reads=reads+1 return {devices=devices} end}
  local function record(name,target,on,done)
    calls[#calls+1]={name=name,target=target,on=on}
    if done then if deferred then pending[#pending+1]=done else done(true) end end
    return true
  end
  network.connect_device=function(id,done) return record("wired",id,true,done) end
  network.disconnect=function(id,done) return record("wired",id,false,done) end
  network.activate_vpn=function(id,done) return record("nm",id,true,done) end
  network.deactivate_vpn=function(id,done) return record("nm",id,false,done) end
  local mesh=morf.signal("net.fixture.mesh",{
    {id="netbird",up=true,detail="Connected",address="100.64.0.1",can_toggle=true},
    {id="tailscale",up=false,detail="Stopped",address="",can_toggle=true},
    {id="zerotier",up=true,detail="Up on zt0",address="",can_toggle=false}})
  local tunnel=morf.signal("net.fixture.tunnel",{
    {id="mullvad",up=false,detail="Off",address="",can_toggle=true},
    {id="protonvpn",up=true,detail="Connected",address="1.2.3.4",can_toggle=true}})
  local loaded=morf.signal("net.fixture.loaded",true)
  package.loaded["lib.vpns"]={rows={mesh={get=function() reads=reads+1 return mesh:get() end},tunnel={get=function() reads=reads+1 return tunnel:get() end}},
    loaded={mesh=loaded,tunnel=loaded},is_mesh_link=function(name) return name:match("^tailscale")~=nil end,
    watch=function() watches=watches+1 end,release=function() releases=releases+1 end,
    set=function(id,on,done) record("app",id,on,done) end}
  local phase=morf.signal("net.fixture.tor.phase","off")
  local progress=morf.signal("net.fixture.tor.progress","")
  package.loaded.services={net=network,tor={phase=phase,progress=progress,socks=9050,
    on=function() return phase:get()=="on" or phase:get()=="starting" end,
    set=function(on) record("tor","service",on) phase:set(on and "starting" or "off") end}}
  if morf.env("TEST_MISSING")=="1" then package.loaded.services.net=nil package.loaded.services.tor=nil end
  local model
  local ok,models=pcall(require,"net_pages_model")
  if ok then local new=models.new models.new=function(...) model=new(...) return model end end
  local pages=require("net_pages")
  local content
  if kind=="wired" then content=pages.wired_page(W,function() return H end)
  elseif kind=="tor" then content=pages.tor_page(W,function() return H end)
  else content=pages.vpn_page(kind,W,function() return H end) end
  ui.Item {width=W,height=H,visible=function() return shown:get() end,content}
  morf.ipc.show=function(on) shown:set(on=="yes") require("presentation").set("settings."..kind,shown:get()) end
  morf.ipc.state=function() return {reads=reads,calls=calls,watches=watches,releases=releases,
    message=model and model.message:get() or "",failed=model and model.failed:get() or false} end
  morf.ipc.defer=function() deferred=true end
  morf.ipc.reply=function(index,ok) pending[tonumber(index)](ok=="yes" and true or nil,"Permission denied") end
  local stale
  morf.ipc.capture=function(key) stale=model.row(key) end
  morf.ipc.choose=function() return model.toggle(stale) end
  morf.ipc.change=function(what)
    if what=="remove" then devices[2]=nil connections[2]=nil
    elseif what=="replace" then devices[2].path="/device/replacement"
    elseif what=="connected" then devices[2].state="activated" connections[2].active=true
    elseif what=="unavailable" then available=false
    elseif what=="empty" then devices={} connections={} mesh:set({}) tunnel:set({})
    elseif what=="many" then
      for i=1,24 do
        devices[#devices+1]={path="/extra/"..i,interface="eth-extra-"..i,type="ethernet",state="disconnected",connection="",ip4="",speed=1000,hw_address="",carrier=true}
        connections[#connections+1]={uuid="extra-"..i,id="VPN "..i,type="wireguard",active=false}
      end
    elseif what=="starting" then phase:set("starting") progress:set("42% · Connecting")
    elseif what=="running" then phase:set("on") progress:set("Connected")
    elseif what=="failed" then phase:set("failed") end
    update()
  end
]]
local function load(style,kind,w,h,dry,missing)
  w,h=w or 408,h or 1000
  test.load("../shell/init.lua",{source=HOST,size={w,h},env={CAELESTIA_STYLE=style,TEST_KIND=kind,
    TEST_WIDTH=tostring(w),TEST_HEIGHT=tostring(h),CAELESTIA_DRY_RUN=dry and "1" or "0",TEST_MISSING=missing and "1" or "0"}})
end
local function open() test.ipc("show","yes") test.advance(2500) end
local function shot(name) if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(name..".png") end end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." Wired reads ports and dispatches by current interface",function()
    load(style,"wired") open() shot(style.."-wired")
    test.truthy(test.find {text="Connected · Campus · 10.20.30.4 · 1000 Mb/s · AA:BB:01"})
    test.click("wired-eth0-switch") test.click("wired-enp1s0-switch")
    test.eq(test.ipc("state").calls,{{name="wired",target="eth0",on=false},{name="wired",target="enp1s0",on=true}})
    test.falsy(test.get("wired-dock0-switch").visible)
    test.ipc("capture",'['..'"/device/2","enp1s0"'..']')
    test.ipc("change","replace") test.falsy(test.ipc("choose"))
    test.eq(#test.ipc("state").calls,2)
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
  test.it(style.." Tunnel separates NetworkManager profiles from managed mesh links",function()
    load(style,"tunnel") open() shot(style.."-tunnel")
    test.falsy(test.find {id="vpn-nm-mesh-link"})
    test.click("vpn-nm-campus-switch") test.click("vpn-nm-personal-switch") test.click("vpn-mullvad-switch")
    test.eq(test.ipc("state").calls,{{name="nm",target="campus",on=false},{name="nm",target="personal",on=true},{name="app",target="mullvad",on=true}})
    test.eq(test.ipc("state").watches,1)
    test.ipc("capture","nm:personal") test.ipc("change","connected") test.ipc("choose")
    test.eq(test.ipc("state").calls[4],{name="nm",target="personal",on=false})
    test.ipc("change","remove") test.falsy(test.ipc("choose"))
    test.ipc("show","no") test.advance(20) test.eq(test.ipc("state").releases,1)
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
  test.it(style.." Mesh respects tools that cannot toggle",function()
    load(style,"mesh") open() shot(style.."-mesh")
    test.click("vpn-netbird-switch") test.click("vpn-tailscale-switch")
    test.falsy(test.get("vpn-zerotier-switch").visible)
    test.eq(test.ipc("state").calls,{{name="app",target="netbird",on=false},{name="app",target="tailscale",on=true}})
    test.ipc("capture","app:zerotier") test.falsy(test.ipc("choose"))
    test.eq(#test.logs("error"),0)
  end)
  test.it(style.." Tor presents progress and blocks overlapping requests",function()
    load(style,"tor") open() shot(style.."-tor")
    test.click("tor-service-switch") test.advance(100)
    test.eq(test.ipc("state").calls,{{name="tor",target="service",on=true}})
    test.ipc("capture","tor") test.falsy(test.ipc("choose"))
    test.ipc("change","starting") test.advance(100)
    test.truthy(test.find {text="Starting · 42% · Connecting"})
    test.ipc("change","running") test.advance(100) test.click("tor-service-switch")
    test.eq(test.ipc("state").calls[2],{name="tor",target="service",on=false})
    test.ipc("change","failed") test.advance(100) test.truthy(test.find {text="Could not start"})
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
end
test.it("Network details suppress hidden reads, stale replies and dry-run actions",function()
  for _,kind in ipairs {"wired","mesh","tunnel","tor"} do
    load("tsugumori",kind,nil,nil,true) test.advance(100)
    test.eq(test.ipc("state").reads,0)
    open()
    local id=({wired="wired-enp1s0-switch",mesh="vpn-netbird-switch",tunnel="vpn-nm-personal-switch",tor="tor-service-switch"})[kind]
    test.click(id) test.eq(test.ipc("state").calls,{})
    test.ipc("show","no") test.advance(100)
    local reads=test.ipc("state").reads
    test.ipc("change","connected") test.advance(100)
    test.eq(test.ipc("state").reads,reads)
  end
  load("tsugumori","tunnel") open() test.ipc("defer")
  test.click("vpn-mullvad-switch")
  test.eq(test.ipc("state").message,"Connecting…")
  test.ipc("reply","1","no") test.truthy(test.ipc("state").failed)
  test.eq(test.ipc("state").message,"Permission denied")
  test.click("vpn-nm-personal-switch") test.click("vpn-nm-campus-switch")
  test.ipc("reply","2","no") test.eq(test.ipc("state").message,"Disconnecting…")
  test.ipc("show","no") test.ipc("reply","3","no") test.advance(100)
  test.eq(test.ipc("state").message,"")
  test.eq(#test.logs("error"),0)
end)
test.it("Network disappearance clears cached ports and rejects stale connection actions",function()
  for _,kind in ipairs {"wired","tunnel"} do
    load("tsugumori",kind) open()
    test.ipc("capture",kind=="wired" and '["/device/2","enp1s0"]' or "nm:personal")
    test.ipc("change","unavailable") test.advance(100)
    test.falsy(test.ipc("choose")) test.eq(test.ipc("state").calls,{})
    test.truthy(test.find {text="NetworkManager is unavailable."})
    test.eq(#test.logs("error"),0)
  end
end)
test.it("Missing network and Tor services retain honest empty states in both themes",function()
  for _,style in ipairs {"material","tsugumori"} do for _,kind in ipairs {"wired","tor"} do
    load(style,kind,nil,nil,true,true) open()
    local text=kind=="tor" and "Tor is not installed." or style=="material" and "NetworkManager is not running." or "NetworkManager is unavailable."
    test.truthy(test.find {text=text,visible=true})
    shot(style.."-missing-"..kind)
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end end
end)
for _,kind in ipairs {"wired","tunnel"} do
  test.it("Tsugumori compact "..kind.." scrolls to the final connection and resets on reopening",function()
    load("tsugumori",kind,360,420) test.ipc("change","many") open()
    local last=kind=="wired" and "wired-eth-extra-24-switch" or "vpn-nm-extra-24-switch"
    local viewport=test.get(kind=="wired" and "wired-scroll" or "vpn-tunnel-scroll")
    local control=test.get(last)
    test.wheel(0,control.y-200,{x=viewport.x+viewport.width-2,y=viewport.y+200}) test.advance(700)
    control=test.get(last)
    test.truthy(control.y>=0 and control.y+control.height<=420)
    test.click(last) test.eq(#test.ipc("state").calls,1)
    shot(kind.."-compact-last")
    test.ipc("show","no") test.advance(50) open()
    local title=test.get(kind=="wired" and "wired-ports-title" or "vpn-tunnel-network-title")
    test.near(title.y,0,1)
    test.ipc("change","empty") test.advance(2500)
    shot(kind.."-compact-empty")
    if kind=="tunnel" then test.truthy(test.find {text="No supported VPN applications are installed."}) end
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
end
test.it("VPN library reports a completed empty scan separately from initial loading",function()
  test.load("../shell/init.lua",{source=[[
    local ui=require("morf.ui")
    local vpns=require("lib.vpns")
    vpns.read=function(_,done) done({}) end
    ui.Item {width=20,height=20}
    morf.ipc.loaded=function() return vpns.loaded.mesh:get() end
    morf.ipc.watch=function() vpns.watch("mesh",8000) end
    morf.ipc.release=function() vpns.release("mesh") end
  ]]})
  test.falsy(test.ipc("loaded")) test.ipc("watch") test.truthy(test.ipc("loaded")) test.ipc("release")
  test.eq(#test.logs("error"),0)
end)
