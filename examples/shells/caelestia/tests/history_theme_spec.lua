local test = morf.test
local HOST = [[
  local ui = require("morf.ui")
  local W,H = tonumber(morf.env("TEST_WIDTH")),tonumber(morf.env("TEST_HEIGHT"))
  morf.surface.height = H
  package.loaded.bar = { desk = function() return 0,0,W,H end }
  package.loaded.services = { here = function() return true end }
  package.loaded["lib.services.notifications"] = { serve = function() return { open = function() return false end } end }
  local copied = ""
  morf.clipboard.set = function(value) copied = value end
  local notifications = require("notifs")
  local history = require("notification_history")
  local presentation = require("presentation")
  local shown = morf.signal("test.history.shown",false)
  local page = history.build(W-32,function() return H-32 end)
  ui.Item { width = W, height = H,
    ui.Item { visible = false, notifications.drawer.panel,
      ui.Sdf { width = 1, height = 1, notifications.drawer.shape } },
    ui.Rect { width = W,height = H,color = function() return require("theme").color.surface end },
    ui.Item { x = 16,y = 16,width = W-32,height = H-32,visible = function() return shown:get() end,page },
  }
  morf.ipc.show = function(on)
    shown:set(on=="yes")
    presentation.set("sidebar.notifications",on=="yes")
    notifications.covered:set(on=="yes")
  end
  morf.ipc.push = function(app,summary,body)
    return notifications.push {app=app,summary=summary,body=body,urgency=2}
  end
  morf.ipc.clear = history.clear
  morf.ipc.toggle = history.toggle
  morf.ipc.state = function()
    local ids = {}
    for _,n in ipairs(notifications.history:get()) do ids[#ids+1]=n.id end
    return {groups=history.groups(),ids=ids,copied=copied}
  end
  morf.ipc.expanded = history.is_open
]]
local function load(style,w,h)
  test.load("../shell/init.lua",{source=HOST,size={w or 440,h or 800},env={CAELESTIA_STYLE=style,
    TEST_WIDTH=tostring(w or 440),TEST_HEIGHT=tostring(h or 800)}})
end
local function shot(name)
  if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(name..".png") end
end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." history groups, expands, copies and dismisses by identity",function()
    load(style)
    test.ipc("push","Build","First task","First body")
    test.ipc("push","Mail","Inbox","New message")
    test.ipc("push","Build","Build complete","<b>All checks</b> passed &amp; ready.")
    test.ipc("show","yes") test.advance(2400)
    local groups=test.ipc("state").groups
    test.eq(groups[1].app,"Build") test.eq(#groups[1].items,2)
    test.eq(groups[1].items[1].summary,"Build complete")
    local height=test.get("sidebar-group-1").height
    shot(style.."-history-grouped")
    test.click("sidebar-group-expand-1") test.advance(2400)
    test.truthy(test.ipc("expanded","Build"))
    test.truthy(test.get("sidebar-group-1").height>height)
    test.click("sidebar-copy-1-1")
    test.eq(test.ipc("state").copied,"All checks passed & ready.")
    shot(style.."-history-expanded")
    test.click("sidebar-dismiss-1-1") test.advance(2400)
    test.eq(test.ipc("state").groups[1].app,"Mail")
    test.eq(#test.ipc("state").ids,2)
    test.ipc("clear") test.advance(2400)
    test.eq(#test.ipc("state").ids,0)
    test.falsy(test.ipc("expanded","Build"))
    test.truthy(test.get("sidebar-empty").visible)
    shot(style.."-history-empty")
    test.eq(#test.logs("warn"),0)
    test.eq(#test.logs("error"),0)
  end)
  test.it(style.." clear keeps arrivals during exit and recovers after rapid hiding",function()
    load(style)
    test.ipc("push","Build","Old task","Clear this")
    test.ipc("show","yes") test.advance(2400)
    test.click("sidebar-clear") test.advance(50)
    local next=test.ipc("push","Mail","New arrival","Keep this")
    test.ipc("clear") -- repeated clicks belong to the same pending operation
    test.ipc("show","no") test.advance(900)
    test.eq(test.ipc("state").ids,{next})
    test.ipc("show","yes") test.advance(2400)
    test.near(test.get("sidebar-group-1").opacity,1,.001)
    test.near(test.get("sidebar-clear").opacity,1,.001)
    test.ipc("show","no") test.advance(16)
    test.ipc("show","yes") test.advance(2400)
    test.truthy(test.settle(500)<100)
    test.eq(#test.logs("error"),0)
  end)
end
test.it("Tsugumori history titles replay on expansion and the compact list scrolls",function()
  load("tsugumori",392,460)
  test.ipc("push","Build","Heading replay","A longer body for the compact panel")
  test.ipc("show","yes") test.advance(850)
  test.truthy(test.get("sidebar-summary-1-1-text").text~="HEADING REPLAY")
  test.advance(1700)
  test.eq(test.get("sidebar-summary-1-1-text").text,"HEADING REPLAY")
  test.click("sidebar-group-expand-1") test.advance(850)
  test.truthy(test.get("sidebar-item-title-1-1-text").text~="HEADING REPLAY")
  test.advance(1700)
  test.eq(test.get("sidebar-item-title-1-1-text").text,"HEADING REPLAY")
  test.click("sidebar-group-expand-1") test.advance(2400)
  for i=2,8 do test.ipc("push","App "..i,"Notification "..i,"Body") end
  test.advance(2400)
  local list=test.get("sidebar-history-list")
  test.truthy(list.x>=0 and list.x+list.width<=392)
  -- (Clear all is in the summary card, at the head of the list.)
  test.truthy(test.get("sidebar-clear").visible)
  test.wheel(0,2000,{x=list.x+20,y=list.y+20}) test.advance(500)
  local last=test.get("sidebar-group-8")
  test.truthy(last.y>=list.y and last.y+last.height<=list.y+list.height+1)
  test.truthy(test.get("sidebar-group-app-8-text").text~="BUILD","scrolled application heading missed its reveal")
  test.truthy(test.get("sidebar-summary-8-1-text").text~="HEADING REPLAY","scrolled notification title missed its reveal")
  test.advance(2200)
  test.eq(test.get("sidebar-group-app-8-text").text,"BUILD")
  test.eq(test.get("sidebar-summary-8-1-text").text,"HEADING REPLAY")
  test.ipc("show","no") test.advance(100)
  test.ipc("show","yes") test.advance(500)
  test.truthy(test.get("sidebar-summary-8-1-text").text~="HEADING REPLAY")
  test.eq(test.get("sidebar-group-app-1-text").text,"APP 8","offscreen group should remain idle")
  test.advance(2200)
  shot("tsugumori-history-compact")
  test.eq(#test.logs("warn"),0)
end)
