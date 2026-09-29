local test=morf.test
local HOST=[[
  local ui=require("morf.ui")
  local W,H=tonumber(morf.env("TEST_WIDTH")),tonumber(morf.env("TEST_HEIGHT"))
  morf.surface.height=H
  local here=morf.signal("test.notifications.here",true)
  package.loaded.services={here=function() return here:get() end}
  package.loaded.bar={desk=function() return 10,10,W-20,H-20 end}
  local change,remote={},{}
  local dismissed={}
  package.loaded["lib.notifications"]={serve=function(options)
    change=options.on_change
    return {open=function(id) return remote[id]~=nil end,
      dismiss=function(id)
        dismissed[#dismissed+1]=id remote[id]=nil
        local list={} for _,entry in pairs(remote) do list[#list+1]=entry end
        change(list)
      end}
  end}
  local notifications=require("notifs")
  local C=require("theme").color
  ui.Item {width=W,height=H,
    ui.Rect {width=W,height=H,color=function() return C.surface end},
    ui.Item {x=10,y=10,width=W-20,height=H-20,
      ui.Sdf {anchors={fill=true},fill_color=function() return C.surfaceContainer end,notifications.drawer.shape},
      notifications.drawer.panel},
  }
  morf.ipc.push=function(summary,body,urgency,timeout)
    return notifications.push {summary=summary,body=body,urgency=tonumber(urgency),timeout_ms=tonumber(timeout),app="Example app"}
  end
  morf.ipc.state=function()
    local ids={} for _,entry in ipairs(notifications.shown()) do ids[#ids+1]=entry.id end
    return {ids=ids,history=#notifications.history:get(),open=notifications.drawer.open:get(),dismissed=dismissed}
  end
  morf.ipc.remote=function(id)
    id=tonumber(id)
    remote[id]={id=id,app="Remote app",summary="Remote notification",body="From the server",urgency=2}
    local list={} for _,entry in pairs(remote) do list[#list+1]=entry end
    change(list)
  end
  morf.ipc.dnd=function(on) notifications.dnd:set(on=="yes") end
  morf.ipc.cover=function(on) notifications.covered:set(on=="yes") end
  morf.ipc.here=function(on) here:set(on=="yes") end
  morf.ipc.expand=function(id) notifications.toggle(tonumber(id)) end
  morf.ipc.expanded=function(id) return notifications.expanded(tonumber(id)) end
  morf.ipc.forget=function(id) notifications.forget(tonumber(id)) end
  morf.ipc.clear=notifications.clear
]]
local function load(style,w,h)
  test.load("../shell/init.lua",{source=HOST,size={w or 1100,h or 750},env={
    CAELESTIA_STYLE=style,TEST_WIDTH=tostring(w or 1100),TEST_HEIGHT=tostring(h or 750)}})
end
local function snapshot(name)
  if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(name..".png") end
end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." popups preserve expansion, identity during dismissal, and history",function()
    load(style)
    local first=test.ipc("push","Build complete","<b>All checks</b> passed &amp; artifacts are ready.","1","20000")
    test.advance(2400)
    test.truthy(test.ipc("state").open)
    test.eq(test.get("notification-body-1").text,"All checks passed & artifacts are ready.")
    local h=test.get("notification-1").height
    test.click("notification-expand-1") test.advance(400)
    test.truthy(test.ipc("expanded",tostring(first)))
    test.truthy(test.get("notification-1").height>h)
    snapshot(style.."-notification-expanded")
    local card=test.get("notification-1")
    test.click(card.x+20,card.y+20) test.advance(60)
    local second=test.ipc("push","New arrival","Must survive the previous dismissal.","2")
    test.advance(2500)
    test.eq(test.ipc("state").ids,{second})
    test.eq(test.ipc("state").history,2)
    test.falsy(test.ipc("expanded",tostring(first)))
    test.near(test.get("notification-1").opacity,1,0.001)
    test.ipc("forget",tostring(first)) test.eq(test.ipc("state").history,1)
    test.ipc("clear") test.advance(1100)
    test.falsy(test.ipc("state").open)
    test.eq(test.ipc("state").history,0)
    test.eq(#test.logs("error"),0)
  end)
  test.it(style.." popup expiry, DND, history coverage and focused output stay shared",function()
    load(style)
    test.ipc("dnd","yes")
    test.ipc("push","Quiet notification","Saved in history","1","900")
    test.advance(400) test.falsy(test.ipc("state").open)
    test.eq(test.ipc("state").history,1)
    test.advance(600) test.eq(#test.ipc("state").ids,0)
    test.eq(test.ipc("state").history,1)
    test.ipc("push","Critical notification","Stays until dismissed","2")
    test.advance(6000) test.eq(#test.ipc("state").ids,1)
    test.ipc("dnd","no") test.advance(2200)
    test.truthy(test.ipc("state").open)
    snapshot(style.."-notification-critical")
    test.ipc("cover","yes") test.advance(1000) test.falsy(test.ipc("state").open)
    test.ipc("cover","no") test.advance(2200) test.truthy(test.ipc("state").open)
    test.ipc("here","no") test.advance(1000) test.falsy(test.ipc("state").open)
    test.ipc("here","yes") test.advance(2200) test.truthy(test.ipc("state").open)
    if style=="tsugumori" then test.eq(test.get("notification-summary-1-text").text,"CRITICAL NOTIFICATION") end
    test.eq(#test.logs("error"),0)
  end)
  test.it(style.." dismisses server-owned notifications through the server",function()
    load(style) test.ipc("remote","19") test.advance(2400)
    test.click("notification-1") test.advance(1300)
    test.eq(test.ipc("state").dismissed,{19})
    test.eq(test.ipc("state").history,1)
    test.falsy(test.ipc("state").open)
  end)
end
test.it("Tsugumori notification stack stays within a compact output when expanded",function()
  load("tsugumori",800,480)
  local body=string.rep("An expanded notification with more detail. ",8)
  for i=1,6 do local id=test.ipc("push","Notification "..i,body,"2") test.ipc("expand",tostring(id)) end
  test.advance(2400)
  local panel=test.get("drawer-notifications")
  test.truthy(panel.x>=0 and panel.x+panel.width<=800)
  test.truthy(panel.y>=0 and panel.y+panel.height<=480)
  test.truthy(test.get("notification-stack").height<5*test.get("notification-1").height)
  snapshot("tsugumori-notifications-compact")
  test.wheel(0,1000,{x=panel.x+80,y=panel.y+80}) test.advance(600)
  local last,viewport=test.get("notification-5"),test.get("notification-stack")
  test.truthy(last.y>=viewport.y and last.y+last.height<=viewport.y+viewport.height+1,
    "the last displayed notification cannot be reached by scrolling")
  test.eq(#test.logs("warn"),0)
  test.eq(#test.logs("error"),0)
end)
