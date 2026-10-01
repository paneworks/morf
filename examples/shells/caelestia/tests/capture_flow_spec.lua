-- Exercise PrintScreen -> original bottom controls -> selection -> editor.
local test=morf.test
local SOURCE=[[
  local backend=require("lib.capture")
  local fixture='<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="800"><rect width="1200" height="800" fill="#37474f"/></svg>'
  local snapshots,closed=0,0
  backend.windows=function(cb) cb({{x=100,y=100,w=500,h=350}}) end
  backend.session=function()
    return {snapshot=function(_,cb) snapshots=snapshots+1 cb(true,{source=fixture,width=1200,height=800}) end,
      render=function(_,_,cb) morf.timer(20,function() cb(true,fixture) end,false) end,
      close=function() closed=closed+1 end,remove=function() end}
  end
  require("init")
  require("config").set("polkit.agent","off")
  -- Services initialise in preview mode. Only the capture exercise below
  -- uses the normal path, with its acquisition backend replaced above.
  local env=morf.env
  morf.env=function(key) if key=="CAELESTIA_DRY_RUN" then return "0" end return env(key) end
  local capture=require("capture")
  morf.ipc.flow=function() return {panel=capture.drawer.open:get(),active=capture.editor.active:get(),
    stage=capture.editor.phase:get(),phase=capture.phase:get(),snapshots=snapshots,closed=closed,
    keyboard=morf.surface.keyboard_focus} end
]]
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." PrintScreen keeps the bottom panel and waits for selection before editing",function()
    test.load("../shell/init.lua",{source=SOURCE,size={1200,800},env={CAELESTIA_STYLE=style,CAELESTIA_DRY_RUN="1"}})
    test.ipc("capture","open") test.advance(500)
    local state=test.ipc("flow")
    test.truthy(state.panel) test.falsy(state.active) test.eq(state.snapshots,0)
    test.truthy(test.get("drawer-capture").visible)
    test.click("capture-target-region") test.click("capture-screenshot") test.advance(600)
    state=test.ipc("flow")
    test.falsy(state.panel) test.truthy(state.active) test.eq(state.stage,"selecting")
    test.falsy(test.get("capture-editor-toolbar").visible)
    test.press(150,120) test.move(700,450) test.release(700,450) test.advance(250)
    test.eq(test.ipc("flow").stage,"editing") test.truthy(test.get("capture-editor-toolbar").visible)
    test.key("Escape") test.advance(100)
    state=test.ipc("flow")
    test.falsy(state.active) test.eq(state.phase,"ready") test.eq(state.keyboard,"none")
    test.ipc("capture","open") test.advance(500)
    test.click("capture-target-screen") test.click("capture-screenshot") test.advance(600)
    state=test.ipc("flow")
    test.falsy(state.panel) test.truthy(state.active) test.eq(state.stage,"editing")
    test.truthy(test.get("capture-editor-toolbar").visible)
    test.ipc("capture","open") test.advance(500)
    state=test.ipc("flow")
    test.truthy(state.panel) test.falsy(state.active) test.eq(state.closed,2)
    test.eq(state.snapshots,2)
    test.ipc("capture","close") test.advance(350)
    test.falsy(test.ipc("flow").panel) test.eq(#test.logs("error"),0)
  end)
end
