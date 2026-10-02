local test=morf.test
local HOST=[[
  local ui=require("morf.ui")
  local W,H=tonumber(morf.env("TEST_WIDTH")),tonumber(morf.env("TEST_HEIGHT"))
  morf.surface.height=H
  local kind=morf.env("TEST_KIND")
  local key=kind=="input" and "microphone" or "sound"
  local revision=morf.signal("sound.fixture.revision",0)
  local shown=morf.signal("sound.fixture.shown",false)
  local available,broken,reads=true,false,0
  local calls={}
  local devices={
    {id=10,name="speakers",description="Desk speakers",kind="sink",default=true,volume=.6,volumes={.5,.7},channels=2,muted=false},
    {id=11,name="headphones",description="Studio headphones",kind="sink",default=false,volume=.4,volumes={.4,.4},channels=2,muted=false},
    {id=12,name="morf.equalizer.input",description="Morf Equalizer",kind="sink",default=false,volume=1,volumes={1,1},channels=2,muted=false},
    {id=20,name="microphone",description="USB microphone",kind="source",default=true,volume=.7,volumes={.7},channels=1,muted=false},
    {id=21,name="speakers.monitor",description="Monitor of speakers",kind="source",default=false,volume=.6,volumes={.6,.6},channels=2,muted=false},
  }
  local streams={
    {id=30,app_name="Music",app_id="music",binary="player",pid=100,media_name="Preview track",direction="playback",device=10,volume=.5,muted=false},
    {id=31,app_name="Recorder",app_id="recorder",binary="recorder",pid=101,direction="record",device=20,volume=.3,muted=false},
    {id=32,app_name="Morf Equalizer",app_id="morf.equalizer",binary="pipewire",pid=102,direction="playback",device=10,volume=1,muted=false},
  }
  local lists=morf.state {sinks={},sources={},streams={}}
  local function update()
    local sinks,sources={},{}
    for _,d in ipairs(devices) do
      local list=d.kind=="sink" and sinks or sources list[#list+1]=d
    end
    lists.sinks:replace(sinks,"id") lists.sources:replace(sources,"id") lists.streams:replace(streams,"id")
    revision:set(revision:get()+1)
  end
  local function read() revision:get() reads=reads+1 if broken then error("fixture server gone") end end
  local function lookup(rows,id) for _,row in ipairs(rows) do if row.id==id then return row end end end
  local function default(kind)
    read() for _,d in ipairs(devices) do if d.kind==kind and d.default then return d end end
  end
  local function record(name,id,value) calls[#calls+1]={name=name,id=id,value=value} end
  morf.audio={sinks=lists.sinks,sources=lists.sources,streams=lists.streams,
    available=function() read() return available end,
    default_sink=function() return default("sink") end,default_source=function() return default("source") end,
    device=function(id) read() return lookup(devices,id) end,stream=function(id) read() return lookup(streams,id) end,
    set_volume=function(id,v) record("volume",id,v) local row=lookup(devices,id) or lookup(streams,id) row.volume=v update() return true end,
    set_mute=function(id,on) record("mute",id,on) local row=lookup(devices,id) or lookup(streams,id) row.muted=on update() return true end,
    set_default=function(id)
      record("default",id,true) local row=lookup(devices,id)
      for _,d in ipairs(devices) do if d.kind==row.kind then d.default=d.id==id end end update() return true
    end,
    set_channel_volumes=function(id,values) record("channels",id,values) lookup(devices,id).volumes=values update() return true end,
    move_stream=function(id,dest) record("route",id,dest) lookup(streams,id).device=dest update() return true end,
  }
  update()
  local models=require("sound_model")
  local create=models.new
  local model
  models.new=function(...) model=create(...) return model end
  local page=require("sound_page")
  local content=(kind=="input" and page.input_page or page.output_page)(W,function() return H end)
  ui.Item {width=W,height=H,visible=function() return shown:get() end,content}
  local function show(on)
    shown:set(on=="yes") require("presentation").set("settings."..key,shown:get())
  end
  morf.ipc.show=show
  morf.ipc.state=function() return {calls=calls,reads=reads,devices=devices,streams=streams} end
  morf.ipc.destination=function() local stream=model.stream(30) return stream and stream.device end
  morf.ipc.change=function(what)
    if what=="rename" then devices[1].description="Renamed speakers" streams[1].app_name="Renamed player"
    elseif what=="missing" then devices={} streams={}
    elseif what=="offline" then available=false
    elseif what=="equalized" then streams[1].device=12
    elseif what=="broken" then broken=true
    elseif what=="many" then
      for i=3,12 do devices[#devices+1]={id=100+i,name="sink"..i,description="Output "..i,kind="sink",volume=.3,volumes={.3},channels=1,default=false,muted=false} end
      for i=2,5 do streams[#streams+1]={id=200+i,app_name="Application "..i,binary="app"..i,pid=200+i,direction="playback",device=10,volume=.4,muted=false} end
    elseif what=="channels" then
      devices[1].channels=12 devices[1].volumes={}
      for i=1,12 do devices[1].volumes[i]=i/20 end
    elseif what=="reorder" then devices[1],devices[2]=devices[2],devices[1]
    elseif what=="replacement" then
      devices[1]={id=10,name="new-speakers",description="Replacement speakers",kind="sink",default=true,volume=.4,volumes={.4},channels=1,muted=false}
      streams[1]={id=30,pid=999,app_id="new-player",binary="different",app_name="Replacement player",direction="playback",device=10,volume=.3,muted=false}
    end
    update()
  end
  morf.ipc.palette=function() require("theme").follow("#45aa86") end
  morf.ipc.guard=function(what)
    local device,stream=model.default(),model.stream(30)
    if what=="reused-device" then
      devices[1]={id=10,name="replacement",kind="sink",volume=.5,channels=1}
      return model.select_device(device)
    elseif what=="reused-stream" then
      streams[1]={id=30,pid=999,binary="different",direction="playback",volume=.5}
      return model.toggle_stream(stream)
    elseif what=="removed" then
      devices={}
      return model.set_device_volume(device,.9)
    elseif what=="invalid-channel" then return model.set_channel(99,.8)
    elseif what=="invalid-volume" then return model.set_device_volume(device,0/0)
    elseif what=="hidden" then show("no") return model.toggle_device(device)
    end
  end
]]
local function load(style,kind,w,h,dry)
  test.load("../shell/init.lua",{source=HOST,size={w or 408,h or 1100},env={CAELESTIA_STYLE=style,
    TEST_KIND=kind or "output",TEST_WIDTH=tostring(w or 408),TEST_HEIGHT=tostring(h or 1100),CAELESTIA_DRY_RUN=dry and "1" or "0"}})
  test.advance(100)
end
local function open() test.ipc("show","yes") test.advance(2500) end
local function shot(name) if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(name..".png") end end
local function last() local calls=test.ipc("state").calls return calls[#calls] end
local function slider(id,fraction)
  local node=test.get(id)
  local rail=test.find(id.."-track") or node
  test.click(rail.x+rail.width*fraction,node.y+node.height/2) test.advance(100)
end
local function scroll(dy)
  local view=test.get("sound-scroll")
  test.wheel(0,dy,{x=view.x+view.width-2,y=view.y+math.min(view.height-10,200)})
end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." Sound preserves volume, channels, mute, selection and app routing",function()
    load(style) open() shot(style.."-sound-output")
    test.click("sound-output-mute") test.advance(100)
    test.eq(last().name,"mute") test.eq(last().id,10) test.eq(last().value,true)
    slider("sound-output-volume",.8) test.eq(last().id,10) test.near(last().value,.8,.06)
    slider("sound-channel-1",.3) test.eq(last().name,"channels") test.eq(last().id,10)
    test.near(last().value[1],.3,.06) test.near(last().value[2],.7,.001)
    test.click("sound-sink-11") test.advance(100) test.eq(last().name,"default") test.eq(last().id,11)
    test.click("sound-app-30-mute") test.advance(100) test.eq(last().id,30) test.eq(last().value,true)
    slider("sound-app-30-volume",.6) test.eq(last().id,30) test.near(last().value,.6,.06)
    test.click("sound-app-30-to-11") test.advance(100) test.eq(last().name,"route") test.eq(last().value,11)
    test.falsy(test.find{id="sound-app-31-volume"})
    test.falsy(test.find{id="sound-sink-12"}) test.falsy(test.find{id="sound-app-32-volume"})
    test.ipc("change","equalized") test.advance(100)
    test.eq(test.ipc("destination"),11)
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
  test.it(style.." Microphone filters monitors and uses the input device",function()
    load(style,"input") open() shot(style.."-sound-input")
    test.truthy(test.get("sound-source-20").visible)
    test.falsy(test.find{id="sound-source-21"})
    test.click("sound-input-mute") test.advance(100) test.eq(last().id,20) test.eq(last().value,true)
    slider("sound-input-volume",.4) test.eq(last().name,"volume") test.eq(last().id,20) test.near(last().value,.4,.06)
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
  test.it(style.." Sound stops reading when hidden and honors dry run",function()
    load(style,"output",408,1100,true)
    test.eq(test.ipc("state").reads,0)
    open()
    test.click("sound-output-mute") slider("sound-output-volume",.9)
    test.click("sound-sink-11") test.click("sound-app-30-to-11")
    test.eq(#test.ipc("state").calls,0)
    test.ipc("show","no") test.advance(100)
    local reads=test.ipc("state").reads
    test.ipc("change","rename") test.advance(2400)
    test.eq(test.ipc("state").reads,reads)
    open()
    test.truthy(test.find{text="Renamed speakers"})
    test.truthy(test.find{text="Renamed player"})
    test.ipc("change","offline") test.advance(100)
    test.truthy(test.find{text="No sound server",visible=true})
    test.ipc("change","broken") test.advance(100)
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
end
test.it("Tsugumori Sound reaches every channel, output and app route in a compact pane",function()
  load("tsugumori","output",340,520) open()
  test.ipc("change","channels") test.ipc("change","many") test.advance(100)
  shot("sound-compact-start")
  local channel=test.get("sound-channel-12")
  scroll(channel.y-180) test.advance(200)
  slider("sound-channel-12",.8)
  test.eq(last().name,"channels") test.eq(#last().value,12) test.near(last().value[12],.8,.06)
  local dest=test.get("sound-sink-112")
  scroll(dest.y-180) test.advance(200)
  test.click("sound-sink-112") test.advance(100) test.eq(last().name,"default") test.eq(last().id,112)
  scroll(30000) test.advance(200)
  local route=test.get("sound-app-205-to-112")
  test.truthy(route.y>=0 and route.y+route.height<=520,"final route is clipped")
  test.click("sound-app-205-to-112") test.advance(100) test.eq(last().name,"route") test.eq(last().id,205) test.eq(last().value,112)
  test.ipc("palette") test.advance(2500) shot("sound-compact-last-route")
  scroll(-30000) test.advance(200)
  test.truthy(test.get("sound-title-output-text").text~="OUTPUT")
  scroll(30000) test.advance(100)
  test.ipc("show","no") test.advance(100) test.eq(test.get("sound-title-output-text").text,"OUTPUT")
  open() test.eq(test.get("sound-title-output-text").text,"OUTPUT")
  test.truthy(test.get("sound-title-output").y<40,"reopening did not reset the mixer viewport")
  test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
end)
test.it("Sound refreshes controls when native IDs are reused or rows are reordered",function()
  load("tsugumori") open()
  test.ipc("change","replacement") test.advance(2500)
  test.truthy(test.find{text="Replacement speakers"})
  test.truthy(test.find{text="Replacement player"})
  test.click("sound-sink-10") test.advance(100)
  test.eq(last().name,"default") test.eq(last().id,10)
  test.click("sound-app-30-mute") test.advance(100)
  test.eq(last().name,"mute") test.eq(last().id,30)
  test.ipc("change","reorder") test.advance(100)
  test.click("sound-sink-11") test.advance(100)
  test.eq(last().name,"default") test.eq(last().id,11)
  test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
end)
test.it("Sound rejects stale devices, reused stream IDs and invalid or hidden actions",function()
  for _,case in ipairs {"reused-device","reused-stream","removed","invalid-channel","invalid-volume","hidden"} do
    load("tsugumori") open()
    test.falsy(test.ipc("guard",case),case)
    test.eq(#test.ipc("state").calls,0,case)
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end
end)
test.it("Tsugumori Sound distinguishes an empty server from no server",function()
  load("tsugumori") open()
  test.ipc("change","missing") test.advance(2500)
  test.truthy(test.find{text="No output selected",visible=true})
  test.truthy(test.find{text="No outputs available",visible=true})
  test.truthy(test.find{text="Nothing playing",visible=true})
  shot("sound-empty")
  test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
end)
