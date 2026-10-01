local test=morf.test
local HOST=[[
  local ui=require("morf.ui")
  morf.surface.height=240
  local motion=require("theme").motion
  local active=morf.signal("cue.active",true)
  local value=morf.signal("cue.value",10)
  local reads=0
  local phase=morf.signal("cue.phase","waiting")
  local attempts=morf.signal("cue.attempts",0)
  local host=ui.Item {id="cue-host",x=20,y=20,width=300,height=180,
    ui.Rect {anchors={fill=true},color="#171d24"}}
  motion.value_flash(host,"metric",{x=260,y=70,height=24,
    active=function() return active:get() end,
    read=function() reads=reads+1 return value:get() end,
    changed=function(before,now) return math.abs(now-before)>=20 end})
  motion.auth_result(host,"identity",{active=function() return active:get() end,
    read=function() return phase:get(),attempts:get() end})
  local list=morf.signal("cue.notifications",{{id=1}})
  local opened=morf.signal("cue.opened",true)
  local dnd=morf.signal("cue.dnd",false)
  local covered=morf.signal("cue.covered",false)
  local here=morf.signal("cue.here",true)
  package.loaded.services={here=function() return here:get() end}
  local cards={ui.Item {x=340,y=20,width=250,height=64},ui.Item {x=340,y=100,width=250,height=64}}
  local function shown()
    local all=list:get() local out={}
    for i=#all,math.max(1,#all-1),-1 do out[#out+1]=all[i] end
    return out
  end
  motion.notification_acquire(host,cards,{list=list,opened=opened,dnd=dnd,covered=covered},shown)
  ui.Item {width=640,height=240,host,table.unpack(cards)}
  morf.ipc.active=function(on) active:set(on) end
  morf.ipc.value=function(n) value:set(n) end
  morf.ipc.reads=function() return reads end
  morf.ipc.phase=function(name,n) phase:set(name) attempts:set(n or 0) end
  morf.ipc.notifications=function(ids)
    local out={} for _,id in ipairs(ids) do out[#out+1]={id=id} end list:set(out)
  end
  morf.ipc.open=function(on) opened:set(on) end
  morf.ipc.dnd=function(on) dnd:set(on) end
]]
local function load()
  test.load("../shell/init.lua",{source=HOST,size={640,240},env={CAELESTIA_STYLE="tsugumori"}})
  test.advance(1)
end
local function clean()
  test.eq(#test.logs("warn"),0) test.eq(#test.logs("error"),0)
end
test.it("meaningful readout changes flash once, throttle bursts and stop reading hidden pages",function()
  load()
  test.ipc("value",12) test.advance(100)
  test.near(test.get("metric-change-flash").opacity,0,.001)
  test.ipc("value",65) test.advance(90)
  test.truthy(test.get("metric-change-flash").opacity>0)
  if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot("tsugumori-value-cue.png") end
  test.advance(500) test.ipc("value",95) test.advance(90)
  test.near(test.get("metric-change-flash").opacity,0,.001)
  test.ipc("active",false) test.advance(1)
  local reads=test.ipc("reads")
  test.ipc("value",10) test.advance(5000)
  test.eq(test.ipc("reads"),reads)
  test.ipc("active",true) test.advance(100)
  test.near(test.get("metric-change-flash").opacity,0,.001)
  test.ipc("value",70) test.advance(90)
  test.truthy(test.get("metric-change-flash").opacity>0)
  test.ipc("active",false) test.advance(1)
  test.near(test.get("metric-change-flash").opacity,0,.001)
  clean()
end)
test.it("authentication results distinguish success from retries and cancel on dismissal",function()
  load()
  local original_x=test.get("identity-register-1").x
  test.ipc("phase","failed",1) test.advance(50)
  test.truthy(math.abs(test.get("identity-register-1").x-original_x)>0)
  test.near(test.get("identity-result-edge").opacity,0,.001)
  if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot("tsugumori-auth-failure.png") end
  test.advance(400)
  test.ipc("phase","failed",2) test.advance(50)
  test.truthy(test.get("identity-register-1").opacity>0)
  test.ipc("phase","done",2) test.advance(50)
  test.truthy(test.get("identity-result-edge").opacity>0)
  if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot("tsugumori-auth-success.png") end
  test.advance(500) test.near(test.get("identity-result-edge").opacity,0,.001)
  test.ipc("phase","failed",3) test.advance(50)
  test.ipc("active",false) test.advance(1)
  test.near(test.get("identity-register-1").opacity,0,.001)
  test.near(test.get("identity-register-1").x,original_x,.001)
  clean()
end)
test.it("notification acquisition follows new identities without replaying reflow or DND history",function()
  load()
  test.near(test.get("notification-1-register-1").opacity,0,.001)
  test.ipc("notifications",{1,2}) test.advance(80)
  test.truthy(test.get("notification-1-register-1").opacity>0)
  test.near(test.get("notification-2-register-1").opacity,0,.001)
  if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot("tsugumori-notification-acquire.png") end
  test.advance(600) test.ipc("notifications",{1}) test.advance(80)
  test.near(test.get("notification-1-register-1").opacity,0,.001)
  test.ipc("dnd",true) test.ipc("notifications",{1,3}) test.advance(1)
  test.ipc("dnd",false) test.advance(80)
  test.near(test.get("notification-1-register-1").opacity,0,.001)
  test.ipc("open",false) test.advance(1)
  test.ipc("notifications",{1,3,4}) test.advance(1)
  test.ipc("open",true) test.advance(200)
  test.near(test.get("notification-1-register-1").opacity,0,.001)
  test.advance(380)
  test.truthy(test.get("notification-1-register-1").opacity>0)
  test.ipc("open",false) test.advance(1)
  test.near(test.get("notification-1-register-1").opacity,0,.001)
  clean()
end)
