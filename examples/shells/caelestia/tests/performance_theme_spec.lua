local test=morf.test
local HOST=[[
  local ui=require("morf.ui")
  local W,H=tonumber(morf.env("TEST_WIDTH")),tonumber(morf.env("TEST_HEIGHT"))
  morf.surface.height=H
  local shown=morf.signal("test.performance.shown",false)
  local revision=morf.signal("test.performance.revision",0)
  local reads,histories,requests,pending,process_reads={},{},{},{},0
  local G=1024^3
  local data={cpu={usage=42,frequency=2400,threads=884,count=4,cores={10,20,30,40}},
    memory={total=16*G,used=8*G,available=8*G,percent=50,cached=2*G,committed=10*G,shared=G,buffers=G/2,dirty=0,commit_limit=20*G,
      swap={total=4*G,used=G,free=3*G}},
    drives={drives={{name="nvme0n1",kind="NVMe",model="Preview disk",capacity=512*G,busy=20,
      read_rate=2048,write_rate=1024,read_total=4*G,write_total=2*G,system=true,removable=false,
      units={{name="nvme0n1p1",label="EFI",depth=1,mounts={"/boot"},size=G,read_rate=0,write_rate=0},
        {name="cryptroot",label="Root",depth=2,mounts={"/","/home"},size=500*G,read_rate=2048,write_rate=1024}}}}},
    network={interfaces={{name="wlan0",wireless=true,state="up",speed=866,address="02:00:00:00:00:01",rx_rate=4096,tx_rate=1024,rx_bytes=G,tx_bytes=G/2},
      {name="eth0",wireless=false,state="up",speed=1000,rx_rate=2048,tx_rate=512,rx_bytes=2*G,tx_bytes=G},
      {name="bridge0",virtual=true,state="up"}}},
    gpu={cards={{name="card0",model="Shared graphics",vendor="Example",busy=23,clock_mhz=900,temperature=48,driver="preview",slot="00:02.0"},
      {name="card1",model="Dedicated graphics",vendor="Example",suspended=true,vram_total=4*G,vram_used=G,busy=45,
        encoder=5,decoder=10,power=20,power_limit=60,clock_mhz=1200,max_mhz=1800,memory_clock_mhz=3500,memory_max_mhz=4000,
        temperature=52,driver_version="1",driver="preview",slot="01:00.0"}}},
    fans={fans={{key="fan0",label="CPU fan",rpm=2200,max=6000,chip="preview"}}},
    temperatures={cpu=58},system={uptime=9384}}
  local info={model="11th Gen Example(R) CPU @ 2.40GHz",logical=4,sockets=1,base_mhz=2400,max_mhz=4200,
    virtualization="Preview-V",caches={L1d=128*1024,L1i=128*1024,L2=1024^2,L3=8*1024^2},
    driver="preview",governor="powersave",preference="balanced"}
  local sysinfo={history_size=60,sources={},cpu_info=function() return info end}
  for _,name in ipairs {"cpu","memory","drives","network","gpu","fans","battery","temperatures","system"} do
    sysinfo.sources[name]={interval=2000,pin=function() end}
    sysinfo[name]=function() revision:get() reads[name]=(reads[name] or 0)+1 return data[name] end
  end
  sysinfo.history=function(name)
    revision:get() histories[name]=(histories[name] or 0)+1
    local values={}
    for i=1,60 do values[i]=25+15*math.sin(i/6) end
    return values
  end
  package.loaded["lib.sysinfo"]=sysinfo
  package.loaded.dashboard_state={context=function() return {opened=function() return shown:get() end} end}
  morf.run=function(argv,options,callback)
    requests[#requests+1]=argv pending[#requests]=callback
    return {cancel=function() end}
  end
  local model
  local source=require("performance_model")
  source.process_count=function() process_reads=process_reads+1 return "123" end
  local new=source.new
  source.new=function(ctx) model=new(ctx) return model end
  local view=require("dashboard_performance")
  if view.resize then view.resize(W-24,H-24) end
  local C=require("theme").color
  local viewport=ui.Flickable {id="performance-viewport",x=12,y=12,width=W-24,height=H-24,clip=true,
    visible=function() return shown:get() end,
    ui.Item {width=view.width or view.WIDTH,height=view.height or view.HEIGHT,view.page}}
  ui.Item {width=W,height=H,ui.Rect {anchors={fill=true},color=function() return C.surfaceContainerLowest end},viewport}
  if view.navigation then morf.effect("test.performance.scroll",function()
    view.navigation:get() viewport.content_x,viewport.content_y=0,0
  end) end
  morf.ipc.shown=function(on) shown:set(on=="yes") end
  morf.ipc.select=function(key) model.selected:set(key) end
  morf.ipc.respond=function(index,stdout)
    local callback=pending[tonumber(index)]
    if callback then callback({ok=true,stdout=stdout}) end
  end
  morf.ipc.update=function(mode)
    if mode=="remove-drive" then data.drives.drives={}
    elseif mode=="remove-net" then data.network.interfaces={}
    elseif mode=="wake-gpu" then data.gpu.cards[2].suspended=false
    else data.cpu.usage=75 end
    revision:set(revision:get()+1)
  end
  morf.ipc.status=function()
    return {reads=reads,histories=histories,requests=requests,process_reads=process_reads,
      selected=model.selected:get(),displayed=model.displayed:get(),addresses=model.addresses:get(),devices=model.list.devices:len()}
  end
]]
local function load(style,w,h)
  test.load("../shell/init.lua",{source=HOST,size={w or 1460,h or 980},env={CAELESTIA_STYLE=style,
    CAELESTIA_DRY_RUN="1",TEST_WIDTH=tostring(w or 1460),TEST_HEIGHT=tostring(h or 980)}})
  test.advance(100)
end
local function shot(name)
  if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(name..".png") end
end
local function select(key) test.ipc("select",key) test.advance(2500) end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." Performance preserves every device kind, readings and logical units",function()
    load(style)
    test.eq(test.ipc("status").reads,{})
    test.eq(test.ipc("status").requests,{})
    test.ipc("shown","yes") test.advance(2500)
    test.truthy(test.get("performance-core-3").visible)
    test.truthy(test.find {text="42%",visible=true})
    test.eq(test.ipc("status").devices,8)
    test.falsy(test.find("performance-device-net:bridge0"))
    shot(style.."-performance-cpu")
    test.click("performance-device-memory") test.advance(2500)
    test.truthy(test.get("performance-memory-graph").visible)
    test.falsy(test.get("performance-core-0").visible)
    test.truthy(test.get("performance-memory-composition").visible)
    shot(style.."-performance-memory")
    select("drive:nvme0n1")
    test.truthy(test.get("performance-drive-throughput").visible)
    test.truthy(test.find {text_contains="/home",visible=true})
    shot(style.."-performance-drive")
    select("net:wlan0")
    test.truthy(test.get("performance-net-throughput").visible)
    select("gpu:card1")
    test.truthy(test.find {text=style=="material" and "Powered down" or "POWERED DOWN",visible=true})
    test.ipc("update","wake-gpu") test.advance(2500)
    test.truthy(test.get("performance-gpu-video").visible)
    test.truthy(test.get("performance-gpu-memory").visible)
    shot(style.."-performance-gpu")
    select("fan:fan0")
    test.truthy(test.get("performance-fan-graph").visible)
    test.truthy(test.find {text="2200 RPM",visible=true})
    test.eq(#test.logs("error"),0)
  end)
  test.it(style.." Performance ignores stale network replies and recovers when a device disappears",function()
    load(style)
    test.ipc("shown","yes") test.advance(1000)
    select("net:wlan0")
    test.eq(#test.ipc("status").requests,2)
    select("net:eth0")
    test.eq(#test.ipc("status").requests,3)
    test.ipc("respond","1",'[{"addr_info":[{"family":"inet","local":"10.0.0.1"}]}]')
    test.ipc("respond","2","Old campus\n")
    test.ipc("respond","3",'[{"addr_info":[{"family":"inet","local":"10.0.0.2"}]}]')
    test.eq(test.ipc("status").addresses,{v4="10.0.0.2",v6="",ssid=""})
    select("net:wlan0")
    test.ipc("shown","no") test.advance(100)
    test.ipc("respond","4",'[{"addr_info":[{"family":"inet","local":"10.0.0.3"}]}]')
    test.ipc("respond","5","Hidden campus\n")
    test.eq(test.ipc("status").addresses,{v4="",v6="",ssid=""})
    test.ipc("shown","yes") test.advance(1000)
    test.ipc("respond","7","Current campus\n")
    test.ipc("respond","6",'[{"addr_info":[{"family":"inet","local":"10.0.0.4"},{"family":"inet6","scope":"global","local":"2001:db8::4"}]}]')
    test.eq(test.ipc("status").addresses,{v4="10.0.0.4",v6="2001:db8::4",ssid="Current campus"})
    test.ipc("update","remove-net") test.advance(1000)
    test.eq(test.ipc("status").selected,"cpu") test.eq(test.ipc("status").displayed,"cpu")
    select("drive:nvme0n1")
    test.ipc("update","remove-drive") test.advance(1000)
    test.eq(test.ipc("status").selected,"cpu")
    test.eq(#test.logs("error"),0)
  end)
  test.it(style.." Performance stops hidden reads and handles rapid selection during closing",function()
    load(style)
    test.ipc("shown","yes") test.advance(2500)
    select("memory")
    local before=test.ipc("status").process_reads
    test.ipc("update","cpu") test.advance(100)
    test.eq(test.ipc("status").process_reads,before)
    test.ipc("shown","no") test.advance(1000)
    before=test.ipc("status")
    test.ipc("update","cpu") test.advance(3000)
    test.eq(test.ipc("status").reads,before.reads)
    test.eq(test.ipc("status").histories,before.histories)
    test.eq(test.ipc("status").requests,before.requests)
    test.ipc("shown","yes") test.advance(100)
    test.ipc("select","gpu:card0") test.advance(80)
    test.ipc("select","drive:nvme0n1") test.advance(80)
    test.ipc("shown","no") test.advance(60)
    test.ipc("select","fan:fan0") test.ipc("shown","yes") test.advance(2500)
    test.eq(test.ipc("status").displayed,"fan:fan0")
    test.truthy(test.get("performance-fan-graph").visible)
    if style=="tsugumori" then test.falsy(test.get("performance-main-page-curtain").visible) end
    test.eq(#test.logs("error"),0)
  end)
end
test.it("Tsugumori Performance keeps compact device navigation and all detail rows reachable",function()
  load("tsugumori",500,720)
  test.ipc("shown","yes") test.advance(2500)
  test.truthy(test.get("performance-device-strip").visible)
  test.click("performance-compact-device-memory") test.advance(2500)
  test.eq(test.ipc("status").displayed,"memory")
  local page=test.get("dashboard-performance")
  test.near(page.width,476,.01)
  test.truthy(test.get("performance-main").x+test.get("performance-main").width<=page.x+page.width)
  shot("tsugumori-performance-compact-memory")
  test.wheel(0,2500,{x=250,y=600}) test.advance(500)
  local last=test.get("performance-fact-memory-5")
  local viewport=test.get("performance-viewport")
  test.truthy(last.y>=viewport.y and last.y+last.height<=viewport.y+viewport.height)
  select("drive:nvme0n1")
  test.wheel(0,2500,{x=250,y=600}) test.advance(500)
  last=test.get("performance-unit-cryptroot")
  test.truthy(last.y>=viewport.y and last.y+last.height<=viewport.y+viewport.height)
  shot("tsugumori-performance-compact-drive")
  select("fan:fan0")
  local button=test.get("performance-compact-device-fan:fan0")
  local strip=test.get("performance-device-strip")
  test.truthy(button.x>=strip.x and button.x+button.width<=strip.x+strip.width)
  test.eq(#test.logs("warn"),0) test.eq(#test.logs("error"),0)
end)
