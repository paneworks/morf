-- Exercise the real controller through a fake agent. Never starts PAM.
local test=morf.test
local function load(style)
  test.load("../shell/init.lua",{size={800,650},env={CAELESTIA_STYLE=style,CAELESTIA_DRY_RUN="0"},source=[[
    local ui=require("morf.ui")
    morf.surface.height=650
    local callbacks,request,answers,cancels=nil,nil,0,0
    package.loaded["lib.polkit_agent"]={serve=function(options) callbacks=options return {} end}
    package.loaded.services={here=function() return true end}
    package.loaded.dashboard={drawer={set=function() end}}
    morf.broadcast=function() return false end
    local polkit=require("polkit")
    ui.Item {width=800,height=650,
      ui.Rect {width=800,height=650,color=function() return require("theme").color.surface end},
      ui.Sdf {width=800,height=650,fill_color=function() return require("theme").color.surfaceContainer end,polkit.drawer.shape},
      polkit.drawer.panel}
    morf.ipc.start=function(id)
      request={id=id,user="test",message="Authorize a test action",action_id="test.action",
        answer=function() answers=answers+1 end,cancel=function() cancels=cancels+1 callbacks.on_done(request,false,"cancelled") end}
      callbacks.on_request(request)
    end
    morf.ipc.info=function(value) request.info=value callbacks.on_request(request) end
    morf.ipc.prompt=function(value) request.prompt=value callbacks.on_request(request) end
    morf.ipc.done=function() callbacks.on_done(request,true) end
    morf.ipc.wrong=function() request.prompt=nil callbacks.on_failure(request) end
    morf.ipc.state=function() return {phase=polkit.phase:get(),info=polkit.info:get(),opened=polkit.opened:get(),answers=answers,cancels=cancels} end
  ]]})
  test.advance(1600)
end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." waits for a real prompt and holds biometric success visibly",function()
    load(style)
    test.ipc("start","face-1") test.advance(600)
    test.eq(test.ipc("state").phase,"waiting")
    test.falsy(test.get("polkit-field").visible)
    test.falsy(test.get("polkit-ok").visible)
    test.type("must-not-be-accepted") test.key("Return")
    test.eq(test.ipc("state").answers,0)
    test.ipc("info","Please look at the camera") test.advance(100)
    test.eq(test.get("polkit-status-text").text,"Looking for your face…")
    if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(style.."-gaze-waiting.png") end
    test.ipc("done") test.advance(1200)
    test.truthy(test.ipc("state").opened)
    test.eq(test.ipc("state").phase,"done")
    test.eq(test.ipc("state").info,"Face verified · Access granted")
    test.eq(test.get("polkit-status-text").text,"Authentication successful")
    test.falsy(test.get("polkit-field").visible)
    if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(style.."-gaze-success.png") end
    test.advance(1200) test.truthy(test.ipc("state").opened)
    test.advance(200) test.falsy(test.ipc("state").opened)
    test.eq(#test.runs(),0)
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
  test.it(style.." reveals password fallback, blocks duplicate answers and waits on retries",function()
    load(style) test.ipc("start","fallback") test.advance(600)
    test.ipc("info","Please look at the camera")
    test.type("ignored")
    test.ipc("info","Face not recognized. Enter your password.") test.advance(30)
    test.falsy(test.get("polkit-field").visible)
    test.ipc("prompt","Password: ") test.advance(100)
    test.truthy(test.get("polkit-field").visible)
    test.key("Return") test.eq(test.ipc("state").answers,0)
    test.type("test-only-secret") test.key("Return") test.advance(40)
    test.eq(test.ipc("state").answers,1)
    test.falsy(test.get("polkit-field").visible)
    test.ipc("info","Checking response") test.advance(30)
    test.eq(test.ipc("state").phase,"checking")
    test.type("discard") test.key("Return") test.eq(test.ipc("state").answers,1)
    test.ipc("wrong") test.advance(100)
    test.eq(test.ipc("state").phase,"waiting")
    test.falsy(test.get("polkit-field").visible)
    test.ipc("prompt","Password: ") test.advance(100)
    test.key("Return") test.eq(test.ipc("state").answers,1)
    test.type("replacement") test.key("Return") test.ipc("done") test.advance(400)
    test.eq(test.ipc("state").info,"Identity verified · Access granted")
    test.click("polkit-cancel") test.advance(100)
    test.falsy(test.ipc("state").opened)
    test.eq(test.ipc("state").cancels,0)
    test.eq(#test.runs(),0)
    for _,log in ipairs(test.logs("info")) do test.falsy(log.message:find("test-only-secret",1,true)) end
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
  test.it(style.." a newer success is not dismissed by an older success timer",function()
    load(style) test.ipc("start","first") test.ipc("done") test.advance(1600)
    test.ipc("start","second") test.ipc("done") test.advance(1200)
    test.truthy(test.ipc("state").opened)
    test.advance(1400) test.falsy(test.ipc("state").opened)
    test.eq(#test.logs("error"),0)
  end)
end
