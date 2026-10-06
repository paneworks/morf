local test=morf.test
local HOST=[[
  local ui=require("morf.ui")
  local kind=morf.env("TEST_KIND")
  local W,H=tonumber(morf.env("TEST_WIDTH")),tonumber(morf.env("TEST_HEIGHT"))
  morf.surface.height=H
  local revision=morf.signal("radio.fixture.revision",0)
  local shown=morf.signal("radio.fixture.shown",false)
  local data={available=true,enabled=true,discovering=false}
  local reads,calls,pending,defer=0,{},{},false
  local aps={
    {key="Campus",ssid="Campus",path="/ap/1",device="wlan0",security="enterprise",known=true,in_use=true,strength=50,secure=true},
    {key="Cafe",ssid="Cafe",path="/ap/2",device="wlan0",security="open",known=false,in_use=false,strength=90,secure=false},
    {key="Quiet",ssid="Quiet",path="/ap/3",device="wlan0",security="wpa2",known=true,in_use=false,strength=20,secure=true},
    {key="Locked",ssid="Locked",path="/ap/4",device="wlan0",security="wpa2",known=false,in_use=false,strength=80,secure=true},
  }
  local devices={
    {path="/bt/1",address="AA:01",alias="Speakers",name="Speakers",named=true,connected=true,paired=true,in_range=true,has_battery=true,battery=82},
    {path="/bt/2",address="AA:02",alias="Keyboard",name="Keyboard",named=true,connected=false,paired=true,in_range=true},
    {path="/bt/3",address="AA:03",alias="Mouse",name="Mouse",named=true,connected=false,paired=false,in_range=true},
    {path="/bt/4",address="AA:04",alias="AA:04",name="",named=false,connected=false,paired=false},
  }
  local lists=morf.state {aps={},devices={}}
  local function update()
    lists.aps:replace(aps,"key") lists.devices:replace(devices,"path") revision:set(revision:get()+1)
  end
  local service_state=setmetatable({},{__index=function(_,key)
    revision:get() reads=reads+1
    return ({available=data.available,wifi_enabled=data.enabled,powered=data.enabled,discovering=data.discovering,
      access_points=lists.aps,devices=lists.devices})[key]
  end})
  local function record(name,value) calls[#calls+1]={name=name,value=value} end
  local function reply(done)
    if done then if defer then pending[#pending+1]=done else done(true) end end
    return true
  end
  local function device_action(name,selected,done)
    record(name,selected.path)
    for _,d in ipairs(devices) do if d.path==selected.path then d.connected=name=="connect-device" end end
    update() return reply(done)
  end
  package.loaded.services={
    net={state=service_state,
      set_wifi=function(on,done) record("wifi",on) data.enabled=on update() return reply(done) end,
      request_scan=function(_,done) record("scan",true) return reply(done) end,
      connect=function(ap,_,_,done)
        record("connect-network",{ssid=ap.ssid,path=ap.path})
        if ap.ssid=="Locked" then return nil,"a password is needed" end
        for _,row in ipairs(aps) do row.in_use=row.key==ap.key end update() return reply(done)
      end},
    bt={state=service_state,
      set_powered=function(on,_,done) record("power",on) data.enabled=on update() return reply(done) end,
      start_discovery=function(_,done) record("discovery",true) data.discovering=true update() return reply(done) end,
      stop_discovery=function(_,done) record("discovery",false) data.discovering=false update() return reply(done) end,
      connect=function(row,done) return device_action("connect-device",row,done) end,
      disconnect=function(row,done) return device_action("disconnect-device",row,done) end},
  }
  morf.spawn=function(spec) record("spawn",spec.command) return true end
  update()
  local models=require("connectivity_model")
  local create=models.new
  local model
  models.new=function(...) model=create(...) return model end
  local page=require("connectivity")
  local content=(kind=="network" and page.network_page or page.bluetooth_page)(W,function() return H end)
  ui.Item {width=W,height=H,visible=function() return shown:get() end,content}
  local function rows() return model and model.list() or kind=="network" and page.networks() or page.devices() end
  local function show(on)
    shown:set(on=="yes") require("presentation").set("settings."..kind,shown:get())
  end
  morf.ipc.show=show
  morf.ipc.entry=function(label)
    for i,row in ipairs(rows()) do
      if (row.ssid or row.alias)==label then
        if morf.env("CAELESTIA_STYLE")=="material" then return kind.."-row-"..i end
        return require("themes").view("connectivity").row_id(model,row)
      end
    end
  end
  morf.ipc.state=function()
    return {calls=calls,reads=reads,rows=rows(),enabled=data.enabled,discovering=data.discovering,
      message=model and model.message:get() or "",failed=model and model.failed:get() or false}
  end
  morf.ipc.delay=function(on) defer=on=="yes" end
  morf.ipc.reply=function(index,on) pending[tonumber(index)](on=="yes" and true or nil,"Delayed failure") end
  morf.ipc.change=function(what)
    if what=="many" then
      for i=1,27 do
        aps[#aps+1]={key="Net "..i,ssid="Net "..i,path="/ap/"..(100+i),device="wlan0",security="open",known=false,in_use=false,strength=40,secure=false}
        devices[#devices+1]={path="/bt/"..(100+i),address="BB:"..i,alias="Device "..i,named=true,connected=false,paired=false}
      end
      aps[#aps+1]={key="Last network",ssid="Last network",path="/ap/999",device="wlan0",security="open",known=false,in_use=false,strength=0,secure=false}
      devices[#devices+1]={path="/bt/999",address="BB:99",alias="ZZZ final device",named=true,connected=false,paired=false}
    elseif what=="offline" then data.available=false
    elseif what=="empty" then aps={} devices={}
    elseif what=="disabled" then data.enabled=false
    elseif what=="renamed" then aps[2].strength=42 devices[2].alias="Desk keyboard"
    elseif what=="handoff" then aps[2].path="/ap/new-cafe"
    end
    update()
  end
  morf.ipc.stale=function()
    local row=model.list()[2]
    if kind=="network" then aps={} else devices={} end
    update()
    return model.choose(row)
  end
  morf.ipc.palette=function() require("theme").follow("#45aa86") end
]]
local function load(style,kind,w,h,dry)
  test.load("../shell/init.lua",{source=HOST,size={w or 408,h or 950},env={CAELESTIA_STYLE=style,
    TEST_KIND=kind,TEST_WIDTH=tostring(w or 408),TEST_HEIGHT=tostring(h or 950),CAELESTIA_DRY_RUN=dry and "1" or "0"}})
  test.advance(100)
end
local function open() test.ipc("show","yes") test.advance(2500) end
local function shot(name) if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(name..".png") end end
local function choose(label)
  local id=test.ipc("entry",label)
  test.truthy(id,"missing row: "..label) test.click(id) test.advance(100)
end
local function last() local calls=test.ipc("state").calls return calls[#calls] end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." Wi-Fi keeps connection order, radio control, scan and selection",function()
    load(style,"network") open() shot(style.."-network")
    local rows=test.ipc("state").rows
    test.eq(rows[1].ssid,"Campus") test.eq(rows[2].ssid,"Cafe") test.eq(rows[4].ssid,"Quiet")
    test.click("network-wifi") test.advance(100) test.eq(last().name,"wifi") test.eq(last().value,false)
    test.click("network-wifi") test.advance(100) test.eq(last().value,true)
    test.click("network-rescan") test.advance(100) test.eq(last().name,"scan")
    test.ipc("change","handoff") test.advance(100)
    choose("Cafe") test.eq(last().name,"connect-network") test.eq(last().value.path,"/ap/new-cafe")
    test.eq(test.ipc("state").rows[1].ssid,"Cafe")
    local count=#test.ipc("state").calls choose("Cafe") test.eq(#test.ipc("state").calls,count)
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
  test.it(style.." Bluetooth keeps pairing order, discovery, connection and settings actions",function()
    load(style,"bluetooth") open() shot(style.."-bluetooth")
    local rows=test.ipc("state").rows
    test.eq(#rows,3) test.eq(rows[1].alias,"Speakers") test.eq(rows[2].alias,"Keyboard")
    test.click("bluetooth-power") test.advance(100) test.eq(last().name,"power") test.eq(last().value,false)
    test.click("bluetooth-power") test.advance(100)
    test.click("bluetooth-discover") test.advance(100) test.eq(last().name,"discovery") test.eq(last().value,true)
    test.click("bluetooth-discover") test.advance(100) test.eq(last().value,false)
    choose("Speakers") test.eq(last().name,"disconnect-device") test.eq(last().value,"/bt/1")
    choose("Keyboard") test.eq(last().name,"connect-device") test.eq(last().value,"/bt/2")
    test.click("bluetooth-settings") test.advance(100) test.eq(last().name,"spawn")
    test.truthy(#last().value>0)
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
  test.it(style.." radio pages suppress actions in dry run and stop hidden reads",function()
    for _,kind in ipairs {"network","bluetooth"} do
      load(style,kind,408,950,true)
      test.eq(test.ipc("state").reads,0)
      open()
      test.click(kind=="network" and "network-wifi" or "bluetooth-power")
      test.click(kind=="network" and "network-rescan" or "bluetooth-discover")
      choose(kind=="network" and "Cafe" or "Keyboard")
      if kind=="bluetooth" then test.click("bluetooth-settings") end
      test.eq(#test.ipc("state").calls,0)
      test.ipc("show","no") test.advance(100)
      local reads=test.ipc("state").reads
      test.ipc("change","renamed") test.advance(2200)
      test.eq(test.ipc("state").reads,reads)
      open()
      if kind=="bluetooth" then test.truthy(test.find{text="Desk keyboard",visible=true}) end
      test.ipc("change","offline") test.advance(100)
      test.eq(#test.ipc("state").rows,0)
      test.truthy(test.find{text=kind=="network" and "No network manager" or "No Bluetooth adapter",visible=true})
      test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
    end
  end)
end
test.it("radio actions report errors and ignore obsolete or hidden replies",function()
  load("tsugumori","network") open()
  choose("Locked")
  test.truthy(test.ipc("state").failed)
  test.eq(test.get("network-status").text,"a password is needed")
  test.ipc("delay","yes")
  choose("Cafe")
  test.click("network-rescan") test.advance(100)
  test.ipc("reply","1","no") test.advance(100)
  test.falsy(test.ipc("state").failed)
  test.eq(test.ipc("state").message,"Scanning for networks…")
  test.ipc("reply","2","yes") test.advance(100)
  test.eq(test.ipc("state").message,"Request sent")
  test.click("network-rescan") test.advance(100)
  test.ipc("show","no") test.advance(100)
  test.ipc("reply","3","no") test.advance(100)
  test.eq(test.ipc("state").message,"") test.falsy(test.ipc("state").failed)
  open() test.eq(test.ipc("state").message,"")
  test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
end)
for _,kind in ipairs {"network","bluetooth"} do
  test.it("Tsugumori "..kind.." reaches rows beyond twenty and resets its compact viewport",function()
    load("tsugumori",kind,340,440) open()
    test.ipc("change","many") test.advance(100)
    test.truthy(#test.ipc("state").rows>20)
    shot(kind.."-compact-top")
    local view=test.get(kind.."-scroll")
    test.wheel(0,30000,{x=view.x+view.width-2,y=200}) test.advance(200)
    local label=kind=="network" and "Last network" or "ZZZ final device"
    local last_row=test.get(test.ipc("entry",label))
    test.truthy(last_row.y>=0 and last_row.y+last_row.height<=440,"last row clipped")
    test.ipc("palette") test.advance(2400) shot(kind.."-compact-bottom")
    choose(label)
    test.eq(last().name,kind=="network" and "connect-network" or "connect-device")
    test.ipc("show","no") test.advance(100)
    test.ipc("show","yes") test.advance(200)
    local heading=kind=="network" and "wifi-title" or "bluetooth-title"
    test.truthy(test.get(heading).y<40,"did not return to radio controls")
    test.advance(2400)
    shot(kind.."-compact-reopened")
    test.falsy(test.ipc("stale"))
    test.ipc("change","empty") test.ipc("change","disabled") test.advance(100)
    test.truthy(test.find{text=kind=="network" and "Turn on Wi-Fi to find networks." or "Turn on Bluetooth to find devices.",visible=true})
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
end
