local test=morf.test
local source=[[
  local ui=require("morf.ui")
  morf.surface.width=800 morf.surface.height=650 morf.surface.keyboard_focus="none"
  local active=morf.signal("fixture.active",true)
  package.loaded.services={here=function() return active:get() end,output=function() return "fixture" end,
    active_output=function() return active:get() and "fixture" or "other" end}
  morf.primary=function() return true end
  morf.broadcast=function() return false end
  local events,queries,rows=nil,0,{}
  morf.spawn=function(options) events=options.on_stdout return {kill=function() end} end
  morf.run=function(_,_,done)
    queries=queries+1 done({ok=true,stdout=morf.json.encode(rows)})
    return {kill=function() end}
  end
  local popup=require("headphones")
  ui.Item {width=800,height=650,ui.Sdf {width=800,height=650,
    fill_color=function() return require("theme").color.surface end,popup.drawer.shape},popup.drawer.panel}
  morf.ipc.rows=function(json) rows=morf.json.decode(json) events("Event 'change' on card #1") end
  morf.ipc.focus=function(value) active:set(value=="here") end
  morf.ipc.state=function() return {open=popup.drawer.open:get(),name=popup.device.name,kind=popup.device.kind,
    queries=queries,focus=morf.surface.keyboard_focus} end
]]
-- Not a dry run: the shell watches the audio server here, through the
-- stubbed morf.spawn / morf.run above (a dry run reaches no server).
local function load(style)
  test.load("../shell/init.lua",{size={800,650},source=source,env={CAELESTIA_STYLE=style or "tsugumori",CAELESTIA_DRY_RUN="0"}})
  test.advance(300)
end
local function sink(name,form,address)
  return {name=name,description=name,properties={["device.form_factor"]=form,["api.bluez5.address"]=address}}
end
local function rows(value) test.ipc("rows",morf.json.encode(value)) test.advance(300) end
test.it("headphones connect in a centered transient popup and dismiss without taking keyboard focus",function()
  for _,style in ipairs {"material","tsugumori"} do
    load(style)
    test.falsy(test.ipc("state").open)
    rows {sink("Studio headphones","headphone","AA:BB:CC:DD:EE:FF")}
    test.advance(700)
    test.truthy(test.ipc("state").open)
    local panel=test.get("drawer-headphones")
    test.near(panel.x+panel.width/2,400,1) test.near(panel.y+panel.height/2,325,1)
    test.eq(test.ipc("state").kind,"headphones")
    test.eq(test.ipc("state").focus,"none")
    if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(style.."-headphones.png") end
    test.advance(3200) test.falsy(test.ipc("state").open)
    test.eq(#test.logs("error"),0)
  end
end)
test.it("earbuds have their own icon and a new connection replaces the old expiry",function()
  load()
  rows {sink("Studio headphones","headphone","AA")}
  test.advance(2000)
  rows {sink("Studio headphones","headphone","AA"),sink("Galaxy Buds","headphone","BB")}
  test.eq(test.ipc("state").kind,"earbuds") test.eq(test.ipc("state").name,"Galaxy Buds")
  test.advance(1800) test.truthy(test.ipc("state").open)
  test.advance(1600) test.falsy(test.ipc("state").open)
end)
test.it("volume updates and Bluetooth profile recreation do not create connection popups",function()
  load()
  rows {sink("Studio headphones","headphone","AA")}
  test.advance(4000) test.falsy(test.ipc("state").open)
  local changed=sink("Studio headphones","headphone","AA") changed.volume=35
  rows {changed} test.falsy(test.ipc("state").open)
  rows {} rows {sink("Studio handsfree","headset","AA")}
  test.falsy(test.ipc("state").open)
  rows {} test.advance(1100) rows {sink("Studio headphones","headphone","AA")}
  test.truthy(test.ipc("state").open)
end)
test.it("wired jack availability triggers a popup, speakers and passive outputs stay quiet",function()
  load()
  local wired={name="built-in",properties={["device.form_factor"]="internal"},
    active_port="speaker",ports={{name="speaker",type="Speaker",availability="available"},
      {name="analog-output-headphones",description="Headphones",type="Headphones",availability="not available"}}}
  rows {wired} test.falsy(test.ipc("state").open)
  wired.ports[2].availability="available" wired.active_port="analog-output-headphones"
  rows {wired} test.truthy(test.ipc("state").open)
  test.advance(4000) test.ipc("focus","other")
  rows {wired,sink("AirPods Pro","headphone","BB")}
  test.falsy(test.ipc("state").open)
  test.eq(#test.logs("error"),0)
end)
test.it("already-connected devices form a silent baseline and unavailable headphone ports stay quiet",function()
  test.load("../shell/init.lua",{source=[[
    local ui=require("morf.ui")
    local count=0
    local tracker=require("models.headphones").new(function() count=count+1 end)
    local device={name="Studio headphones",properties={["device.form_factor"]="headphone"}}
    tracker.update({device}) tracker.update({device})
    tracker.update({{name="speakers",ports={{name="headphone",availability="not available"}}}})
    ui.Item {width=800,height=650}
    morf.ipc.count=function() return count end
  ]]})
  test.eq(test.ipc("count"),0)
end)
