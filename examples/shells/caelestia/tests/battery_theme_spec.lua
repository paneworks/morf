local test=morf.test
local HOST=[[
  local ui=require("morf.ui")
  local W,H=tonumber(morf.env("TEST_WIDTH")),tonumber(morf.env("TEST_HEIGHT"))
  morf.surface.height=H
  local tick=morf.signal("test.battery.tick",0)
  local revision=morf.signal("test.battery.revision",0)
  local reads,history_reads,keys=0,0,{}
  local rings={}
  for _,field in ipairs {"percent","power","voltage","temperature"} do rings[field]=require("lib.poll").ring(60) end
  local function sample(n)
    rings.percent.push(n%100)
    rings.power.push(7+math.sin(n/6)*3)
    rings.voltage.push(11.4+math.sin(n/8)*.6)
    rings.temperature.push(32+math.sin(n/10)*4)
  end
  for i=1,60 do sample(i) end
  local reading
  local function update(mode)
    local b={name="BAT1",capacity=76,status="Discharging",power=8.5,health=91,temperature=34.2,
      voltage=11.8,voltage_design=11.4,vendor="Example",model="Portable 54",technology="Li-ion",serial="ABC-123",
      energy=38.5,energy_full=50,energy_design=55,cycles=208,charge_start=70,charge_limit=80,charge_mode="Standard"}
    reading={batteries={b},time_left=7260,time_to_full=3540}
    if mode=="charging" then b.status="Charging"
    elseif mode=="empty" then reading={batteries={}}
    elseif mode=="partial" then reading={batteries={{name="BAT1",capacity=0,status="Unknown"}}}
    elseif mode=="broken" then reading=false end
    revision:set(revision:get()+1)
  end
  update("discharging")
  local sources={}
  for _,name in ipairs {"cpu","memory","drives","network","gpu","fans","battery"} do
    sources[name]={interval=3000,pinned=false,pin=function(self,on) self.pinned=on end}
  end
  package.loaded["lib.sysinfo"]={history_size=60,sources=sources,
    restore_history=function() end,snapshot_history=function() return {} end,
    battery=function()
      reads=reads+1 tick:get() revision:get()
      if reading==false then error("device removed") end
      return reading
    end,
    history=function(key)
      history_reads=history_reads+1 tick:get() revision:get() keys[key]=true
      if reading==false then error("device removed") end
      local field=key:match("^bat:BAT1:(.+)$")
      return field and rings[field].list() or {}
    end}
  local state=require("dashboard_state")
  local paths={}
  local original=ui.Path
  ui.Path=function(props)
    local node=original(props)
    if props.id then paths[props.id]=node end
    return node
  end
  -- What each chart is drawn from: its series, as the theme's line reads it.
  local kit=require("kit")
  local chart,charts=kit.chart,{}
  kit.chart=function(spec) if spec.id then charts[spec.id]=spec end return chart(spec) end
  local view=require("dashboard_battery")
  kit.chart=chart
  ui.Path=original
  if view.resize then view.resize(W-24,H-24) end
  local C=require("theme").color
  ui.Item {width=W,height=H,
    ui.Rect {anchors={fill=true},color=function() return C.surfaceContainerLowest end},
    ui.Flickable {id="battery-viewport",x=12,y=12,width=W-24,height=H-24,clip=true,
      visible=function() return state.opened:get() end,
      ui.Item {width=view.width or view.WIDTH,height=view.height or view.HEIGHT,view.page}},
    ui.Timer {interval=3000,["repeat"]=true,running=true,on_triggered=function()
      if sources.battery.pinned then local next=tick:get()+1 sample(60+next) tick:set(next) end
    end},
  }
  morf.ipc.shown=function(on) state.displayed:set(4) state.opened:set(on=="yes") end
  morf.ipc.update=update
  morf.ipc.resize=function(w) if view.resize then view.resize(tonumber(w)) end end
  morf.ipc.status=function()
    return {reads=reads,history_reads=history_reads,pinned=sources.battery.pinned,keys=keys,
      history=rings.percent.list(),samples=tick:get(),allocated=#rings.percent.items}
  end
  morf.ipc.path=function(id) return paths[id] and paths[id].d or "" end
  morf.ipc.series=function(id)
    local out={}
    for i,v in ipairs(charts[id].first()) do out[i]=("%.3f"):format(v) end
    return table.concat(out," ")
  end
]]
local function load(style,w,h)
  test.load("../shell/init.lua",{source=HOST,size={w or 1100,h or 680},env={CAELESTIA_STYLE=style,
    CAELESTIA_DRY_RUN="1",TEST_WIDTH=tostring(w or 1100),TEST_HEIGHT=tostring(h or 680)}})
  test.advance(100)
end
local function shot(name)
  if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(name..".png") end
end
test.it("Tsugumori battery status changes flash without adding hidden data reads",function()
  load("tsugumori")
  test.ipc("shown","yes") test.advance(1000)
  test.ipc("update","charging") test.advance(90)
  test.truthy(test.get("battery-charge-change-flash").opacity>0)
  shot("tsugumori-battery-change")
  test.ipc("shown","no") test.advance(1)
  local before=test.ipc("status")
  test.ipc("update","discharging") test.advance(100)
  test.eq(test.ipc("status").reads,before.reads)
  test.near(test.get("battery-charge-change-flash").opacity,0,.001)
  test.ipc("shown","yes") test.advance(100)
  test.near(test.get("battery-charge-change-flash").opacity,0,.001)
  test.eq(#test.logs("error"),0)
end)
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." battery keeps readings and charge status while history uses the device name",function()
    load(style)
    local state=test.ipc("status")
    test.eq(state.reads,0) test.eq(state.history_reads,0) test.truthy(state.pinned)
    test.ipc("shown","yes") test.advance(2500)
    test.eq(test.get("battery-percent").text,"76%")
    test.truthy(test.find {text="8.5 W",visible=true})
    test.truthy(test.find {text="2h 01m",visible=true})
    test.truthy(test.find {text="70–80 %",visible=true})
    for _,field in ipairs {"percent","power","voltage","temperature"} do
      test.truthy(test.ipc("status").keys["bat:BAT1:"..field])
    end
    -- Every chart draws the whole history (60 samples) across its box.
    for _,field in ipairs {"charge","power","voltage","temperature"} do
      local _,count=test.ipc("series","battery-graph-"..field):gsub("%S+","")
      test.eq(count,60)
      test.truthy(test.get("battery-graph-"..field).width>100)
    end
    shot(style.."-battery")
    test.ipc("update","charging") test.advance(2400)
    test.truthy(test.find {text=style=="tsugumori" and "FULL IN" or "Full in",visible=true})
    test.truthy(test.find {text="59m",visible=true})
    test.eq(#test.logs("error"),0)
  end)
  test.it(style.." battery retains bounded history while hidden without updating its UI",function()
    load(style)
    test.ipc("shown","yes") test.advance(2400)
    local before_series=test.ipc("series","battery-graph-charge")
    test.ipc("shown","no") test.advance(1000)
    local before=test.ipc("status")
    test.advance(198000)
    local after=test.ipc("status")
    test.eq(after.reads,before.reads) test.eq(after.history_reads,before.history_reads)
    test.truthy(after.samples>before.samples+60)
    test.eq(#after.history,60) test.eq(after.allocated,60)
    test.truthy(after.history[1]~=before.history[1])
    test.ipc("shown","yes") test.advance(200)
    if style=="tsugumori" then test.truthy(test.get("battery-title-text").text~="BATTERY") end
    test.advance(2400)
    test.truthy(test.ipc("status").history_reads>after.history_reads)
    test.truthy(test.ipc("series","battery-graph-charge")~=before_series,"the chart kept the series from before hiding")
    test.eq(#test.logs("error"),0)
  end)
  test.it(style.." battery handles no device, partial readings and source failures",function()
    load(style)
    test.ipc("update","empty") test.ipc("shown","yes") test.advance(2500)
    test.truthy(test.find {text="No battery",visible=true})
    test.eq(test.get("battery-percent").text,"--")
    test.ipc("update","partial") test.advance(2400)
    test.eq(test.get("battery-percent").text,"0%")
    test.ipc("update","broken") test.advance(2400)
    test.eq(test.get("battery-percent").text,"--")
    test.eq(#test.logs("error"),0)
  end)
end
test.it("Tsugumori battery keeps every graph and device fact on the shared page through interrupted reopening",function()
  load("tsugumori")
  test.ipc("shown","yes") test.advance(2500)
  local page=test.get("dashboard-battery")
  test.near(page.width,1040,.01)
  for _,id in ipairs {"battery-charge-card","battery-main","battery-graph-charge","battery-graph-power",
    "battery-graph-voltage","battery-graph-temperature","battery-facts"} do
    local node=test.get(id)
    test.truthy(node.x>=page.x and node.x+node.width<=page.x+page.width+.5,id.." leaves the page")
    test.truthy(node.y>=page.y and node.y+node.height<=page.y+page.height+.5,id.." leaves the page")
  end
  test.truthy(test.get("battery-graph-voltage").y>test.get("battery-graph-charge").y)
  test.ipc("shown","no") test.advance(100)
  test.ipc("shown","yes") test.advance(70)
  test.ipc("shown","no") test.advance(60)
  test.ipc("shown","yes") test.advance(2500)
  test.near(test.get("battery-charge-card").opacity,1,.001)
  test.near(test.get("battery-facts").opacity,1,.001)
  test.eq(#test.logs("warn"),0) test.eq(#test.logs("error"),0)
end)
