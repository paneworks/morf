local test=morf.test
local function load()
  test.stub_run("task",{code=0,stdout="[]"})
  test.load("../shell/init.lua",{size={1920,1080},env={CAELESTIA_STYLE="tsugumori",CAELESTIA_DRY_RUN="1",
    LULE_A="/nonexistent/lule",HYPRLAND_INSTANCE_SIGNATURE=false},source=[[
    require("services").here=function() return true end
    require("services").weather=function() return {available=false} end
    require("init")
    morf.ipc.page=function(index)
      local dashboard=require("dashboard")
      dashboard.tab:set(tonumber(index)) dashboard.drawer.set(true)
    end
  ]]})
  test.advance(2000)
end
-- The heading scrambles some time in its first 900 ms on screen (a short
-- word settles sooner than a long one), then reads its word.
local function decoded(id,expected)
  -- (A few looks only: every find walks the whole scene.)
  local scrambled=false
  for _,ms in ipairs {150,250,250,250} do
    test.advance(ms)
    local label=test.find {id=id.."-text"}
    if label and label.visible and label.text~=expected then scrambled=true end
  end
  local label=test.get(id.."-text")
  test.truthy(label.visible,"heading is hidden: "..id)
  test.truthy(scrambled,"heading never scrambled: "..id)
  test.advance(1700)
  test.eq(test.get(id.."-text").text,expected)
  test.near(test.get(id.."-ghost-a").opacity,0,0.001)
end
test.it("every main panel uses the shared title and replays it on entry",function()
  load()
  for _,case in ipairs {
    {"bottom","open","assistant","assistant-title","ASSISTANT"},
    {"bottom","open","drop","drop-title","DROP"},
    {"tasks","open",nil,"tasks-title","MAKE ROOM FOR TODAY."},
    {"calendar","open",nil,"planner-title","A DAY AT A TIME."},
    {"sidebar","open","notifications","sidebar-title","NOTIFICATIONS"},
    {"capture","open",nil,"capture-title","CAPTURE"},
    {"page","3",nil,"performance-devices-title","DEVICES"},
    {"page","2",nil,"media-empty-title","NOTHING PLAYING"},
    {"page","4",nil,"battery-title","BATTERY"},
    {"page","5",nil,"weather-place","WEATHER"},
    {"lule","open",nil,"lule-wallpaper-heading","WALLPAPER"},
  } do
    test.ipc("close") test.advance(900)
    if case[3] then test.ipc(case[1],case[2],case[3]) else test.ipc(case[1],case[2]) end
    decoded(case[4],case[5])
    if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(case[4]..".png") end
  end
  test.ipc("close") test.advance(900)
  test.ipc("bottom","open","assistant")
  decoded("assistant-title","ASSISTANT")
  test.eq(#test.logs("error"),0)
  test.eq(#test.logs("warn"),0)
end)
test.it("settings detail titles and editor titles begin when their own page appears",function()
  load()
  test.ipc("settings","sound")
  decoded("settings-detail-heading","SOUND")
  test.eq(test.get("sound-title-output-text").text,"OUTPUT")
  test.eq(test.get("sound-title-output-device-text").text,"OUTPUT DEVICE")
  test.eq(test.get("sound-title-apps-text").text,"APPS")
  test.ipc("settings","microphone")
  decoded("settings-detail-heading","MICROPHONE")
  test.eq(test.get("sound-title-input-text").text,"INPUT")
  for _,case in ipairs {
    {"network","wifi-title","WI-FI"},
    {"bluetooth","bluetooth-title","BLUETOOTH"},
    {"power","power-heading-power-mode","POWER MODE"},
    {"bar","bar-heading-show-the-bar","SHOW THE BAR"},
    {"wired","settings-detail-heading","WIRED"},
    {"mesh","settings-detail-heading","MESH"},
    -- (Tor is a part of the Tunnel page now: `settings tor` opens Tunnel.)
    {"tunnel","settings-detail-heading","TUNNEL"},
  } do
    test.ipc("settings",case[1])
    decoded(case[2],case[3])
    if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot("headings-settings-"..case[1]..".png") end
  end
  test.ipc("close") test.advance(900)
  test.ipc("tasks","open") test.advance(2400)
  test.click("tasks-add")
  decoded("task-editor-title","NEW TASK")
  test.click("task-cancel")
  decoded("tasks-title","MAKE ROOM FOR TODAY.")
  test.eq(#test.logs("error"),0)
end)
test.it("notification titles replay when a group expands and collapses",function()
  load()
  test.ipc("notify","Heading replay","Notification body","critical","Heading test")
  test.ipc("sidebar","open","notifications")
  decoded("sidebar-summary-1-1","HEADING REPLAY")
  test.click("sidebar-group-expand-1")
  decoded("sidebar-item-title-1-1","HEADING REPLAY")
  test.click("sidebar-group-expand-1")
  decoded("sidebar-summary-1-1","HEADING REPLAY")
  test.eq(#test.logs("error"),0)
  test.eq(#test.logs("warn"),0)
end)
