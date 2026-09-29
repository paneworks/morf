local test=morf.test
local HOST=[[
  local ui=require("morf.ui")
  local W,H=tonumber(morf.env("TEST_WIDTH")),tonumber(morf.env("TEST_HEIGHT"))
  morf.surface.height=H
  local shown=morf.signal("settings.test.shown",false)
  local revision=morf.signal("settings.test.revision",0)
  local calls,reads={},0
  local data={wifi=false,bluetooth=false,volume=.4,brightness=.6,ring="sound",bar=false}
  local function record(name,value) calls[#calls+1]={name=name,value=value} end
  local function update() revision:set(revision:get()+1) end
  local function read(key) revision:get() reads=reads+1 return data[key] end
  local function list(rows) return {len=function() return #rows end,get=function(_,i) return rows[i] end} end
  local netstate=setmetatable({},{__index=function(_,key)
    revision:get() reads=reads+1
    return ({available=true,wifi_enabled=data.wifi,wwan_enabled=false,
      access_points=list({{ssid="Preview university",in_use=data.wifi}}),devices=list({}),vpn_connections=list({}),
      wired={carrier=true,connected=false,device="eth0",ip4=""}})[key]
  end})
  local btstate=setmetatable({},{__index=function(_,key)
    revision:get() reads=reads+1
    return ({available=true,powered=data.bluetooth,devices=list({})})[key]
  end})
  package.loaded.services={
    net={state=netstate,set_wifi=function(on) record("wifi",on) data.wifi=on update() end,
      set_wwan=function(on) record("wwan",on) end,
      connect_device=function(port) record("connect",port) end,disconnect=function(port) record("disconnect",port) end},
    bt={state=btstate,set_powered=function(on) record("bluetooth",on) data.bluetooth=on update() end},
    upower={state={available=true,display={present=true,charging=false,percentage=73}}},
    tor={on=function() return false end,phase=morf.signal("settings.test.tor","off"),socks=9050,
      set=function(on) record("tor",on) end},
    ringer={state=setmetatable({},{__index=function() return read("ring") end}),next=function() record("ringer",true) end},
  }
  package.loaded["lib.vpns"]={links=function() revision:get() reads=reads+1 return {} end,is_mesh_link=function() return false end}
  package.loaded["lib.ringer"]={icon=function() return "volume_up" end}
  package.loaded.notifs={dnd=morf.signal("settings.test.dnd",false)}
  package.loaded.osd={volume=function() return read("volume") end,brightness=function() return read("brightness") end,
    volume_icon=function() return "volume_up" end,brightness_icon=function() return "brightness_high" end,
    set_volume=function(v) record("volume",v) data.volume=v update() end,
    set_brightness=function(v) record("brightness",v) data.brightness=v update() end}
  package.loaded.bar={on=function() return read("bar") end,side=function() return "top" end,
    set_on=function(on) record("bar",on) data.bar=on update() end}
  morf.audio={available=function() return false end}
  morf.idle={inhibit=function(on) record("inhibit",on) end}
  morf.run=function(argv,opts,callback) record("command",argv) if callback then callback{ok=true} end end
  local config=require("config")
  config.set("utilities.commands.mic_off",{"preview-command","~/microphone","$HOME","$DATE"})
  local model=require("utilities")
  local kit=require("kit")
  model.page_content=function(key,w,h)
    return ui.Item {id="fixture-detail-"..key,width=w,height=h,
      kit.heading {id="fixture-title-"..key,text=key,width=w,scope="settings."..key},
      kit.pill {id="fixture-detail-action-"..key,width=120,label="Test action",
        y=function() return h()-40 end,on_clicked=function() record("detail",key) end}}
  end
  -- Also lets the preserved pre-extraction module render the same fixture.
  for _,d in ipairs(model.DETAILS) do d.build=function(w,h) return model.page_content(d.key,w,h) end end
  local function open(on)
    shown:set(on)
    require("presentation").set("sidebar.settings",on)
    if not model.opened then model.shown(on) end
  end
  package.loaded.sidebar={drawer={set=open}}
  package.loaded.capture={drawer={set=function(on) record("capture",on) end}}
  ui.Item {width=W,height=H,
    ui.Rect {anchors={fill=true},color=function() return require("theme").color.surface end},
    ui.Item {x=12,y=12,width=W-24,height=H-24,visible=function() return shown:get() end,
      model.page(W-24,function() return H-24 end)}}
  morf.ipc.open=function(on) open(on=="yes") end
  morf.ipc.select=function(key)
    if model.request then return model.request(key) end
    model.detail:set(key) return true
  end
  morf.ipc.update=function() data.volume=.83 data.wifi=true update() end
  morf.ipc.state=function()
    return {calls=calls,reads=reads,awake=model.awake:get(),dnd=require("notifs").dnd:get(),
      open=shown:get(),requested=model.detail:get(),displayed=(model.displayed or model.detail):get(),
      overview=require("presentation").active("settings.overview")(),volume=data.volume,brightness=data.brightness}
  end
  morf.ipc.clear_calls=function() calls={} end
  morf.ipc.action=function(key,on)
    for _,t in ipairs(model.TOGGLES) do if t.id==key then t.set(on) return end end
  end
  morf.ipc.color=function(accent) require("theme").follow(accent) end
]]
local serial=0
local function load(style,w,h,dry)
  serial=serial+1
  local settings=morf.env("XDG_CACHE_HOME").."/settings-theme-fixture-"..serial..".json"
  morf.fs.remove(settings)
  test.load("../shell/init.lua",{source=HOST,size={w or 432,h or 1160},env={CAELESTIA_STYLE=style,
    TEST_WIDTH=tostring(w or 432),TEST_HEIGHT=tostring(h or 1160),CAELESTIA_DRY_RUN=dry and "1" or "0",
    CAELESTIA_SETTINGS=settings}})
  test.advance(100)
end
local function open() test.ipc("open","yes") test.advance(2500) end
local function shot(name) if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(name..".png") end end
local function called(name)
  for _,entry in ipairs(test.ipc("state").calls) do if entry.name==name then return entry.value end end
end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." Settings preserves readings, controls and capture handoff",function()
    load(style) open()
    shot(style.."-settings-overview")
    test.clear_logs() test.ipc("clear_calls")
    test.click("utilities-toggle-wifi") test.advance(100)
    test.eq(called("wifi"),true)
    test.truthy(test.find{text="Preview university",visible=true})
    test.click("utilities-toggle-bluetooth") test.advance(100)
    test.eq(called("bluetooth"),true)
    test.click("utilities-toggle-awake") test.advance(100)
    test.eq(called("inhibit"),true)
    test.truthy(test.ipc("state").awake)
    test.click("utilities-toggle-dnd") test.advance(100)
    test.truthy(test.ipc("state").dnd)
    local slider=test.get("utilities-volume")
    test.click(slider.x+slider.width*.75,slider.y+slider.height/2) test.advance(100)
    test.near(test.ipc("state").volume,.75,.06)
    test.click("utilities-toggle-mic") test.advance(100)
    local command=called("command")
    test.eq(command[1],"preview-command")
    test.truthy(command[2]:sub(1,1)=="/" and command[3]:sub(1,1)=="/")
    test.truthy(command[4]:match("^%d%d%d%d%d%d%d%d_"))
    test.click("utilities-more-bluetooth") test.advance(2500)
    test.eq(test.ipc("state").displayed,"bluetooth")
    test.truthy(test.get("fixture-detail-bluetooth").visible)
    shot(style.."-settings-detail")
    test.click("settings-back") test.advance(2500)
    test.click("utilities-capture") test.advance(100)
    test.eq(called("capture"),true)
    test.falsy(test.ipc("state").open)
    test.truthy(test.ipc("state").awake,"closing Settings disabled the idle inhibitor")
    test.eq(#test.logs("error"),0)
    test.eq(#test.logs("warn"),0)
  end)
  test.it(style.." Settings stops hidden reads, resets closed navigation and rejects unknown pages",function()
    load(style)
    test.eq(test.ipc("state").reads,0)
    open()
    test.truthy(test.ipc("state").reads>0)
    test.ipc("select","network") test.advance(2500)
    local reads=test.ipc("state").reads
    test.ipc("update") test.advance(100)
    test.eq(test.ipc("state").reads,reads)
    test.falsy(test.ipc("select","unknown"))
    test.eq(test.ipc("state").requested,"network")
    test.ipc("open","no") test.advance(1000)
    test.eq(test.ipc("state").requested,"")
    reads=test.ipc("state").reads
    test.ipc("update") test.advance(100)
    test.eq(test.ipc("state").reads,reads)
    open()
    test.eq(test.ipc("state").displayed,"")
    test.truthy(test.find{text="Preview university",visible=true})
    test.ipc("open","no") test.advance(50)
    test.ipc("select","power") test.ipc("open","yes") test.advance(100)
    test.ipc("select","sound") test.advance(100)
    test.ipc("select","network") test.advance(2500)
    test.eq(test.ipc("state").displayed,"network")
    test.truthy(test.get("fixture-detail-network").visible)
    test.eq(#test.logs("error"),0)
  end)
  test.it(style.." Settings dry run suppresses radio, audio, Tor, ring and command actions",function()
    load(style,nil,nil,true) open() test.ipc("clear_calls")
    for _,key in ipairs {"wifi","bluetooth","wired","airplane","mic","tor","ringer","awake"} do
      test.ipc("action",key,true)
    end
    for _,key in ipairs {"volume","brightness"} do
      local slider=test.get("utilities-"..key)
      test.click(slider.x+slider.width*.7,slider.y+slider.height/2)
    end
    test.advance(200)
    test.eq(test.ipc("state").calls,{})
    test.truthy(test.ipc("state").awake)
    test.eq(#test.logs("error"),0)
  end)
end
test.it("Tsugumori Settings presents details under the cover and cancels interrupted transitions",function()
  load("tsugumori") open()
  test.click("utilities-more-wifi") test.advance(100)
  test.eq(test.ipc("state").requested,"network")
  test.eq(test.ipc("state").displayed,"")
  test.truthy(test.get("settings-page-curtain").width>0)
  shot("settings-covered-switch")
  test.advance(800)
  test.eq(test.ipc("state").displayed,"network")
  test.truthy(test.get("settings-detail-heading-text").text~="NETWORK")
  test.ipc("open","no") test.advance(100)
  test.eq(test.get("settings-detail-heading-text").text,"NETWORK")
  test.eq(test.ipc("state").displayed,"network","closing flashed the overview")
  test.ipc("open","yes") test.advance(2500)
  test.eq(test.ipc("state").displayed,"")
  test.falsy(test.get("settings-page-curtain").visible)
  test.eq(test.get("settings-title-text").text,"CONTROLS")
  test.eq(#test.logs("error"),0)
end)
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." VPN detail polling follows actual page visibility",function()
    test.load("../shell/init.lua",{size={432,500},env={CAELESTIA_STYLE=style},source=[[
      local ui=require("morf.ui")
      morf.surface.height=500
      local watches,releases,holding=0,0,false
      package.loaded.services={}
      package.loaded["lib.vpns"]={rows={mesh=morf.signal("vpn.test.rows",{})},
        watch=function() watches=watches+1 holding=true end,
        release=function() releases=releases+1 holding=false end,
        set=function() error("A polling test must not change VPN state") end}
      local detail=morf.signal("vpn.test.requested","mesh")
      ui.Item {width=432,height=500,
        require("net_pages").vpn_page("mesh",408,function() return 480 end,detail)}
      morf.ipc.show=function(on) require("presentation").set("settings.mesh",on=="yes") end
      morf.ipc.state=function() return {watches=watches,releases=releases,holding=holding} end
    ]]})
    test.advance(20)
    test.eq(test.ipc("state").watches,0)
    test.ipc("show","yes") test.advance(20)
    test.eq(test.ipc("state").watches,1)
    test.truthy(test.ipc("state").holding)
    test.ipc("show","no") test.advance(20)
    test.eq(test.ipc("state").releases,1)
    test.falsy(test.ipc("state").holding)
    test.ipc("show","yes") test.ipc("show","no") test.advance(20)
    test.falsy(test.ipc("state").holding)
    test.eq(#test.logs("error"),0)
    test.eq(#test.logs("warn"),0)
  end)
end
test.it("Tsugumori compact Settings reveals scrolled titles and reaches the last control",function()
  load("tsugumori",360,580) open()
  shot("settings-compact-top")
  local viewport=test.get("settings-overview-scroll")
  test.wheel(0,3000,{x=viewport.x+viewport.width-2,y=viewport.y+100}) test.advance(800)
  local last=test.get("utilities-toggle-dnd")
  test.truthy(last.y>=viewport.y and last.y+last.height<=viewport.y+viewport.height)
  test.truthy(test.get("settings-tile-title-dnd-text").text~="DO NOT DISTURB")
  test.advance(1600)
  test.eq(test.get("settings-tile-title-dnd-text").text,"DO NOT DISTURB")
  test.click("utilities-toggle-dnd") test.advance(100)
  test.truthy(test.ipc("state").dnd)
  shot("settings-compact-attention")
  test.ipc("color","#45aa86") test.advance(100)
  shot("settings-compact-palette")
  test.ipc("select","sound") test.advance(2500)
  local detail=test.get("settings-detail-sound")
  test.wheel(0,2000,{x=detail.x+80,y=detail.y+100}) test.advance(200)
  test.click("fixture-detail-action-sound")
  test.eq(called("detail"),"sound")
  test.eq(#test.logs("error"),0)
  test.eq(#test.logs("warn"),0)
end)
