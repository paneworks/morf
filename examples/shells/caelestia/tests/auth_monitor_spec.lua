-- Routing tests use disposable requests and private-transport fixtures, never PAM.
local test=morf.test
local HOST=[[
  local ui=require("morf.ui")
  local focused=morf.signal("fixture.focus","external")
  local output=morf.env("TEST_OUTPUT")
  local broadcasts,answers={},{}
  package.loaded.services={
    here=function() return focused:get()==output end,
    output=function() return output end,
    active_output=function() return focused:get() end,
  }
  package.loaded.dashboard={drawer={set=function() end}}
  morf.broadcast=function(verb,...)
    broadcasts[#broadcasts+1]={verb,...}
    return false
  end
  morf.request_socket=function(path,payload,done)
    answers[#answers+1]={path=path,data=morf.json.decode(payload)}
    morf.timer(1,function() done("ok\n") end,false)
  end
  local keyring=require("keyring")
  local polkit=require("polkit")
  local markers=require("authsteps")
  morf.surface.height=650
  ui.Item {width=800,height=650,
    ui.Sdf {width=800,height=650,keyring.drawer.shape,polkit.drawer.shape,markers.drawer.shape},
    keyring.drawer.panel,polkit.drawer.panel,markers.drawer.panel}
  morf.ipc.focus=function(value) focused:set(value) end
  morf.ipc.prompt=function(id,target,endpoint)
    keyring.message("prompt",morf.json.encode({id=tonumber(id),output=target,endpoint=endpoint or "/fixture/private-answer",
      kind="password",properties={title="Fixture",message="Disposable request"}}))
  end
  morf.ipc.finished=function(id,endpoint)
    keyring.message("close",morf.json.encode({id=tonumber(id),endpoint=endpoint or "/fixture/private-answer"}))
  end
  morf.ipc.polkit=function(id,target,phase)
    polkit.message("view",id,"Fixture","test.action","test","Password:",phase or "asking","",0,target)
  end
  morf.ipc.demo=polkit.demo
  morf.ipc.mark=markers.steps.mark
  morf.ipc.state=function() return {keyring=keyring.opened:get(),polkit=polkit.opened:get(),
    marker=markers.drawer.open:get(),length=keyring.lengths[1]:get(),answers=answers,broadcasts=broadcasts} end
]]
local function load(output)
  test.load("../shell/init.lua",{size={800,650},source=HOST,
    env={TEST_OUTPUT=output,CAELESTIA_DRY_RUN="1",CAELESTIA_STYLE="material"}})
end
test.it("authentication stays off the laptop when the external monitor owns the request",function()
  load("laptop")
  test.ipc("prompt","1","external") test.ipc("polkit","p1","external")
  test.ipc("mark","face","sudo") test.advance(1000)
  local state=test.ipc("state")
  test.falsy(state.keyring) test.falsy(state.polkit) test.falsy(state.marker)
  -- Focusing this monitor during later PAM updates must not duplicate polkit.
  test.ipc("focus","laptop") test.ipc("polkit","p1","external","checking") test.advance(100)
  test.falsy(test.ipc("state").polkit)
  test.truthy(test.ipc("state").marker)
  test.eq(#test.logs("error"),0)
end)
test.it("keyring uses the active monitor and answers only through its private transport",function()
  load("external") test.ipc("prompt","1","external") test.advance(1600)
  test.truthy(test.ipc("state").keyring)
  test.type("disposable-secret")
  test.ipc("focus","laptop") test.advance(100)
  -- Input remains on its original monitor until that request finishes.
  test.truthy(test.ipc("state").keyring)
  test.key("Return") test.advance(600)
  local state=test.ipc("state")
  test.falsy(state.keyring) test.eq(state.length,0)
  test.eq(#state.answers,1)
  test.eq(state.answers[1].path,"/fixture/private-answer")
  test.eq(state.answers[1].data.password,"disposable-secret")
  test.eq(#state.broadcasts,0)
  test.eq(#test.logs("error"),0)
end)
test.it("keyring close events cannot dismiss a newer bridge request",function()
  load("external") test.ipc("prompt","1","external","/old") test.advance(1000)
  test.type("old-draft") test.ipc("prompt","1","external","/new") test.advance(100)
  test.eq(test.ipc("state").length,0)
  test.ipc("finished","1","/old") test.advance(100)
  test.truthy(test.ipc("state").keyring)
  test.key("Escape") test.advance(600)
  local state=test.ipc("state")
  test.falsy(state.keyring) test.eq(#state.answers,1)
  test.eq(state.answers[1].path,"/new") test.eq(state.answers[1].data.action,"cancel")
  test.eq(#state.broadcasts,0)
  test.eq(#test.logs("error"),0)
end)
test.it("sudo feedback follows focus and dismisses success on every monitor",function()
  load("external") test.ipc("mark","face","sudo") test.advance(1000)
  test.truthy(test.ipc("state").marker)
  test.ipc("focus","laptop") test.advance(100)
  test.falsy(test.ipc("state").marker)
  test.ipc("mark","password","sudo") test.ipc("focus","external") test.advance(1000)
  test.truthy(test.ipc("state").marker)
  test.ipc("mark","ok","sudo") test.advance(2800)
  test.falsy(test.ipc("state").marker)
  test.eq(#test.logs("error"),0)
end)
test.it("polkit captures its destination once, including repeated PAM updates",function()
  load("external") test.ipc("demo") test.advance(1600)
  local state=test.ipc("state")
  test.truthy(state.polkit)
  test.eq(state.broadcasts[1][11],"external")
  test.ipc("focus","laptop") test.type("wrong") test.key("Return") test.advance(1100)
  state=test.ipc("state")
  test.truthy(state.polkit)
  for _,event in ipairs(state.broadcasts) do
    if event[2]=="view" then test.eq(event[11],"external") end
  end
  test.eq(#test.logs("error"),0)
end)
test.it("lock chooses the focused external output even when a laptop display exists",function()
  test.load("../lock/init.lua",{args={"window","preview"},source=[[
    morf.screens={{name="eDP-1",width=800,height=650},{name="DP-5",width=800,height=650}}
    package.loaded["lib.integrations.hyprland"]={json=function(_,callback) callback({{name="eDP-1",focused=false},{name="DP-5",focused=true}}) end}
    local theme=require("themes").current
    local build=require(theme.lock)
    local state
    package.loaded[theme.lock]=function(ctx) state=ctx return build(ctx) end
    require("init")
    morf.ipc.output=function() return state.main_output:get() end
  ]],env={CAELESTIA_DRY_RUN="1",CAELESTIA_STYLE="material"}})
  test.eq(test.ipc("output"),"DP-5")
  test.eq(#test.logs("error"),0)
end)
