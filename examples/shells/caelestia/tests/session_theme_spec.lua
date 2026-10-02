local test = morf.test
local HOST = [[
  local ui = require("morf.ui")
  local W,H=tonumber(morf.env("TEST_WIDTH")),tonumber(morf.env("TEST_HEIGHT"))
  morf.surface.height=H
  package.loaded.bar={desk=function() return 10,10,W-20,H-20 end}
  local session=require("session")
  local theme=require("theme")
  ui.Item {width=W,height=H,
    ui.Rect {width=W,height=H,color=function() return theme.color.surface end},
    session.dim(),
    ui.Item {x=10,y=10,width=W-20,height=H-20,
      ui.Sdf {anchors={fill=true},fill_color=function() return theme.color.surfaceContainer end,session.drawer.shape},
      session.drawer.panel},
  }
  morf.ipc.open=function(on) session.drawer.set(on=="yes") end
  morf.ipc.state=function() return {open=session.opened:get(),focus=session.focus:get()} end
  morf.ipc.command=session.command
  morf.ipc.color=function(accent) theme.follow(accent) end
]]
local function load(style,w,h,dry)
  test.stub_run("systemctl",{code=0})
  test.stub_run("loginctl",{code=0})
  test.load("../shell/init.lua",{source=HOST,size={w or 1280,h or 800},env={
    CAELESTIA_STYLE=style,CAELESTIA_DRY_RUN=dry==false and "0" or "1",USER="theme-test",
    TEST_WIDTH=tostring(w or 1280),TEST_HEIGHT=tostring(h or 800)}})
end
local function open() test.ipc("open","yes") test.advance(2200) end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." session preserves navigation, dismissal and dry-run actions",function()
    load(style) open()
    test.eq(#test.runs(),0)
    test.eq(test.ipc("state").focus,1)
    -- The shared console: a 300-wide panel, a full-width row per action
    -- under the operator block.
    local panel=test.get("drawer-session")
    test.eq(panel.width,300)
    test.truthy(test.get("session-picture").visible)
    local last
    for _,id in ipairs {"logout","shutdown","hibernate","reboot"} do
      local row=test.get("session-"..id)
      test.eq(row.width,276)
      if last then test.truthy(row.y>=last.y+last.height,"rows overlap: "..id) end
      last=row
    end
    test.truthy(last.y+last.height<=panel.y+panel.height,"rows leave the panel")
    if style=="material" then test.eq(test.get("session-title").text,"Session")
    else test.eq(test.get("session-title-text").text,"SESSION") end
    if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(style.."-session.png") end
    test.key("Up") test.eq(test.ipc("state").focus,4)
    test.key("Tab") test.eq(test.ipc("state").focus,1)
    test.key("Down") test.eq(test.ipc("state").focus,2)
    test.key("Escape") test.advance(1000)
    test.falsy(test.ipc("state").open)
    test.eq(#test.runs(),0)
    open() test.key("Down") test.key("Return") test.advance(1000)
    test.falsy(test.ipc("state").open)
    local invoked=false
    for _,entry in ipairs(test.logs("info")) do
      if entry.message:find("session shutdown (dry run): systemctl poweroff",1,true) then invoked=true end
    end
    test.truthy(invoked)
    test.eq(#test.runs(),0)
    open() test.click(30,30) test.advance(1000)
    test.falsy(test.ipc("state").open)
    test.eq(#test.runs(),0)
    test.eq(#test.logs("error"),0)
  end)
  test.it(style.." session click dispatches the configured command once",function()
    load(style,nil,nil,false) open()
    test.move("session-reboot") test.advance(350)
    test.eq(test.ipc("state").focus,4)
    test.click("session-reboot") test.advance(1000)
    test.falsy(test.ipc("state").open)
    test.eq(#test.runs(),1)
    test.eq(test.runs()[1],{"systemctl","reboot"})
    test.eq(test.ipc("command","logout"),{"loginctl","terminate-user","theme-test"})
  end)
end
test.it("Tsugumori session fits compact outputs and survives interrupted reopening",function()
  load("tsugumori",800,480) open()
  local panel=test.get("drawer-session")
  test.truthy(panel.x>=0 and panel.x+panel.width<=800)
  test.truthy(panel.y>=0 and panel.y+panel.height<=480)
  test.ipc("open","no") test.advance(180)
  test.ipc("open","yes") test.advance(2400)
  test.truthy(test.get("drawer-session").visible)
  for _,id in ipairs {"logout","shutdown","hibernate","reboot"} do
    test.near(test.get("session-"..id).opacity,1,0.001)
  end
  test.eq(test.get("session-title-text").text,"SESSION")
  test.ipc("color","#9955ee") test.advance(500)
  if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot("tsugumori-session-compact.png") end
  test.eq(#test.logs("error"),0)
  test.eq(#test.logs("warn"),0)
end)
