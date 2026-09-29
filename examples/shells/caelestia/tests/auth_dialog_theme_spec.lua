local test=morf.test
local HOST=[[
  local ui=require("morf.ui")
  local W,H=tonumber(morf.env("TEST_WIDTH")),tonumber(morf.env("TEST_HEIGHT"))
  morf.surface.height=H
  package.loaded.bar={desk=function() return 10,10,W-20,H-20 end}
  local here=morf.signal("test.auth.here",true)
  package.loaded.services={here=function() return here:get() end}
  package.loaded.dashboard={drawer={set=function() end}}
  local polkit=require("polkit")
  local markers=require("authsteps")
  local C=require("theme").color
  ui.Item {width=W,height=H,
    ui.Rect {width=W,height=H,color=function() return C.surface end},
    ui.Item {x=10,y=10,width=W-20,height=H-20,
      ui.Sdf {anchors={fill=true},fill_color=function() return C.surfaceContainer end,
        polkit.drawer.shape,markers.drawer.shape},polkit.drawer.panel,markers.drawer.panel},
  }
  morf.ipc.demo=polkit.demo
  morf.ipc.state=function() return {phase=polkit.phase:get(),info=polkit.info:get(),
    open=polkit.drawer.open:get(),requested=polkit.request:get()~=false,registered=polkit.registered:get()} end
  morf.ipc.view=function(...) polkit.message("view",...) end
  morf.ipc.here=function(on) here:set(on=="yes") end
  morf.ipc.close=function() polkit.drawer.set(false) end
  morf.ipc.reopen=function() polkit.drawer.set(true) end
  morf.ipc.mark=markers.steps.mark
  morf.ipc.marker=function() return {open=markers.drawer.open:get(),step=markers.steps.state.step,failures=markers.steps.state.failures} end
]]
local function load(style,w,h)
  test.load("../shell/init.lua",{source=HOST,size={w or 1000,h or 700},env={CAELESTIA_STYLE=style,
    CAELESTIA_DRY_RUN="1",TEST_WIDTH=tostring(w or 1000),TEST_HEIGHT=tostring(h or 700)}})
end
local function snapshot(name)
  if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(name..".png") end
end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." polkit retries, clears submitted input and finishes a simulated request",function()
    load(style) test.ipc("demo") test.advance(2200)
    test.truthy(test.ipc("state").open)
    test.falsy(test.ipc("state").registered)
    snapshot(style.."-polkit-asking")
    test.key("Return") test.eq(test.ipc("state").phase,"asking")
    test.type("wrong") test.key("Return") test.advance(100)
    test.eq(test.ipc("state").phase,"checking")
    test.type("ignored-while-checking")
    snapshot(style.."-polkit-checking")
    test.advance(900)
    test.eq(test.ipc("state").phase,"wrong")
    test.truthy(test.get("polkit-info").visible)
    snapshot(style.."-polkit-wrong")
    -- Checking did not accept a second secret. Return on the empty field
    -- stays in the wrong phase rather than submitting the ignored text.
    test.key("Return") test.advance(50)
    test.eq(test.ipc("state").phase,"wrong")
    test.type("test-only-secret") test.eq(test.ipc("state").phase,"asking")
    test.click("polkit-ok") test.advance(1050)
    test.eq(test.ipc("state").phase,"done")
    snapshot(style.."-polkit-done")
    test.advance(3000)
    test.falsy(test.ipc("state").open)
    test.falsy(test.ipc("state").requested)
    test.eq(#test.runs(),0)
    for _,level in ipairs {"error","warn","info"} do
      for _,log in ipairs(test.logs(level)) do
        test.falsy(log.message:find("test-only-secret",1,true))
      end
    end
    test.eq(#test.logs("error"),0)
  end)
  test.it(style.." polkit cancels during checking and protects a newer prompt from old timers",function()
    load(style) test.ipc("demo") test.advance(900)
    test.type("example") test.key("Return") test.advance(100)
    test.key("Escape") test.advance(50)
    test.eq(test.ipc("state").phase,"refused")
    test.ipc("demo") test.advance(2400)
    test.eq(test.ipc("state").phase,"asking")
    test.truthy(test.ipc("state").open)
    test.type("discard-on-close") test.ipc("close") test.advance(1000)
    test.ipc("reopen") test.advance(1000)
    test.key("Return") test.advance(50)
    test.eq(test.ipc("state").phase,"asking")
    test.click("polkit-cancel") test.advance(1800)
    test.falsy(test.ipc("state").open)
    test.ipc("here","no") test.ipc("demo") test.advance(1000)
    test.falsy(test.ipc("state").open)
    test.eq(#test.logs("error"),0)
  end)
  test.it(style.." authentication markers follow stages, retry and dismiss",function()
    load(style)
    test.ipc("mark","face","sudo") test.advance(2200)
    test.truthy(test.ipc("marker").open,morf.json.encode({state=test.ipc("marker"),errors=test.logs("error")}))
    snapshot(style.."-auth-face")
    test.ipc("mark","finger","sudo") test.advance(900)
    snapshot(style.."-auth-finger")
    test.ipc("mark","password","sudo") test.advance(700)
    test.ipc("mark","face","sudo") test.advance(900)
    test.eq(test.ipc("marker").failures,1)
    test.truthy(test.find {text="Try again · Looking for your face",visible=true},morf.json.encode({state=test.ipc("marker"),errors=test.logs("error")}))
    test.ipc("mark","ok","sudo") test.advance(300)
    snapshot(style.."-auth-ok")
    test.advance(2800) test.falsy(test.ipc("marker").open)
    test.ipc("here","no") test.ipc("mark","finger","sudo") test.advance(1000)
    test.falsy(test.ipc("marker").open)
    test.truthy(test.settle(500)<100,"hidden authentication effects did not settle")
    test.eq(#test.runs(),0)
    test.eq(#test.logs("error"),0)
  end)
end
test.it("Tsugumori authentication fits a compact output and its masked field",function()
  load("tsugumori",800,480) test.ipc("demo") test.advance(2300)
  local panel=test.get("drawer-polkit")
  test.truthy(panel.x>=0 and panel.x+panel.width<=800)
  test.truthy(panel.y>=0 and panel.y+panel.height<=480)
  test.type("abcdefghijklmnopqrstuv") test.advance(400)
  local last,button=test.get("polkit-dot-18"),test.get("polkit-submit")
  test.truthy(last.visible)
  test.truthy(last.x+last.width<button.x)
  snapshot("tsugumori-polkit-compact")
  test.eq(#test.logs("warn"),0)
end)
