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
  -- On a desk a panel's tabs name its pages: the frame's title is not drawn.
  for _,case in ipairs {
    {"bottom","open","assistant","assistant-title"},
    {"tasks","open",nil,"tasks-title"},
    {"sidebar","open","notifications","sidebar-title"},
  } do
    test.ipc("close") test.advance(900)
    if case[3] then test.ipc(case[1],case[2],case[3]) else test.ipc(case[1],case[2]) end
    test.advance(1200)
    local title=test.find {id=case[4]}
    test.truthy(not title or not title.visible,"a desk page drew its title: "..case[4])
  end
  for _,case in ipairs {
    {"capture","open",nil,"capture-title","CAPTURE"},
    {"page","2",nil,"media-empty-title","NOTHING PLAYING"},
  } do
    test.ipc("close") test.advance(900)
    if case[3] then test.ipc(case[1],case[2],case[3]) else test.ipc(case[1],case[2]) end
    decoded(case[4],case[5])
    if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(case[4]..".png") end
  end
  test.eq(#test.logs("error"),0)
  test.eq(#test.logs("warn"),0)
end)
test.it("settings detail titles and editor titles begin when their own page appears",function()
  load()
  test.ipc("settings","sound")
  decoded("settings-title","SOUND")
  test.truthy(test.get("sound-title-output").visible)
  test.truthy(test.get("sound-title-output-device").visible)
  test.truthy(test.get("sound-title-apps").visible)
  test.ipc("settings","microphone")
  decoded("settings-title","MICROPHONE")
  test.truthy(test.get("sound-title-input").visible)
  for _,case in ipairs {
    {"network","settings-title","NETWORK"},
    {"bluetooth","settings-title","BLUETOOTH"},
    {"power","settings-title","POWER"},
    {"bar","settings-title","BAR"},
    {"wired","settings-title","WIRED"},
    {"mesh","settings-title","MESH"},
    -- (Tor is a part of the Tunnel page now: `settings tor` opens Tunnel.)
    {"tunnel","settings-title","TUNNEL"},
  } do
    test.ipc("settings",case[1])
    decoded(case[2],case[3])
    if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot("headings-settings-"..case[1]..".png") end
  end
  test.ipc("close") test.advance(900)
  test.ipc("tasks","open") test.advance(2400)
  test.click("tasks-add")
  test.advance(900)
  test.truthy(test.get("task-editor-title").visible)
  test.click("task-cancel") test.advance(1200)
  -- (The page's title is the panel frame's: it stays as the editor shuts.)
  test.eq(test.get("tasks-title-text").text,"TASKS")
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
