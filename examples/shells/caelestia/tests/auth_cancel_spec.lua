-- Fake transports only: these tests never invoke PAM, polkit or a keyring.
local test=morf.test
local function load(style, delayed)
  test.load("../shell/init.lua", {size={800,650}, env={CAELESTIA_STYLE=style,CAELESTIA_DRY_RUN="0",
    TEST_DELAYED=delayed and "1" or "0"}, source=[[
    local ui=require("morf.ui")
    morf.surface.height=650
    local callbacks, requests, cancels, queue = nil, {}, 0, {}
    package.loaded["lib.services.polkit_agent"]={serve=function(options) callbacks=options return {} end}
    package.loaded.services={here=function() return true end}
    package.loaded.dashboard={drawer={set=function() end}}
    morf.broadcast=function(verb,...)
      if morf.env("TEST_DELAYED")~="1" then return false end
      queue[#queue+1]={...} return true
    end
    local polkit=require("polkit")
    local keyring=require("keyring")
    ui.Item {width=800,height=650,
      ui.Sdf {width=800,height=650,polkit.drawer.shape,keyring.drawer.shape},
      polkit.drawer.panel,keyring.drawer.panel}
    morf.ipc.start=function(id,phase)
      local request={id=id,user="fixture",message="Cancel fixture",action_id="test.cancel",
        prompt=phase=="asking" and "Password: " or nil,answer=function() end,
        cancel=function() cancels=cancels+1 end} -- deliberately never completes
      requests[id]=request callbacks.on_request(request)
    end
    morf.ipc.flush=function()
      local old=queue queue={}
      for _,args in ipairs(old) do polkit.message(table.unpack(args)) end
    end
    morf.ipc.late=function(id)
      local request=requests[id]
      request.prompt="Password: " callbacks.on_request(request)
      callbacks.on_failure(request) callbacks.on_done(request,true)
    end
    morf.ipc.view=function(...) polkit.message("view",...) end
    morf.ipc.cancel=polkit.cancel
    morf.ipc.keyring=function()
      keyring.message("prompt",morf.json.encode({id=1,endpoint="/fake/bridge",kind="password",properties={}}))
    end
    morf.request_socket=function() end -- deliberately never replies
    morf.ipc.state=function() return {open=polkit.opened:get(),pending=polkit.pending:get(),
      request=polkit.request:get()~=false,cancels=cancels,keyring=keyring.opened:get(),busy=keyring.busy:get()} end
  ]]})
  test.advance(1600)
end
for _,style in ipairs {"material","tsugumori"} do
  for _,phase in ipairs {"waiting","asking","checking"} do
    test.it(style.." Cancel immediately releases "..phase.." without a backend reply",function()
      load(style)
      test.ipc("start","old",phase=="checking" and "asking" or phase) test.advance(700)
      if phase=="checking" then test.type("fixture-secret") test.key("Return") test.advance(30) end
      test.click("polkit-cancel") test.advance(1)
      local state=test.ipc("state")
      test.falsy(state.open) test.falsy(state.pending) test.falsy(state.request) test.eq(state.cancels,1)
      test.falsy(test.get("polkit-keys").focus) test.eq(test.get("polkit-keys").text,"")
      test.ipc("late","old") test.advance(50) test.falsy(test.ipc("state").open)
      test.ipc("start","new","asking") test.advance(700)
      test.ipc("late","old") test.advance(50) test.truthy(test.ipc("state").pending)
      test.key("Escape") test.advance(1) test.falsy(test.ipc("state").open)
      test.eq(test.ipc("state").cancels,2) test.eq(#test.logs("error"),0)
    end)
  end
  test.it(style.." delayed broadcasts cannot reopen a locally cancelled dialog",function()
    load(style,true) test.ipc("start","old","asking") test.ipc("flush") test.advance(700)
    test.ipc("view","old","fixture","test.cancel","user","","checking","",0)
    test.click("polkit-cancel") test.advance(1)
    test.falsy(test.ipc("state").open) test.eq(test.ipc("state").cancels,0)
    test.ipc("view","old","fixture","test.cancel","user","Password","asking","",0)
    test.falsy(test.ipc("state").open)
    test.ipc("flush") test.advance(1) test.eq(test.ipc("state").cancels,1)
    test.ipc("late","old") test.ipc("flush") test.advance(1000)
    test.falsy(test.ipc("state").open) test.eq(#test.logs("error"),0)
  end)
  test.it(style.." keyring Cancel releases input during an unanswered submission",function()
    load(style) test.ipc("keyring") test.advance(700)
    test.type("fixture-secret") test.key("Return") test.advance(1)
    test.truthy(test.ipc("state").busy)
    test.click("keyring-cancel") test.advance(1)
    test.falsy(test.ipc("state").keyring) test.falsy(test.ipc("state").busy)
    test.falsy(test.get("keyring-keys-1").focus) test.eq(test.get("keyring-keys-1").text,"")
    test.eq(#test.logs("error"),0)
  end)
end

test.it("the shell releases exclusive keyboard focus on Cancel and on completion",function()
  test.load("../shell/init.lua",{size={800,650},env={CAELESTIA_STYLE="tsugumori",CAELESTIA_DRY_RUN="1"},source=[[
    require("init")
    morf.ipc.focus_policy=function() return morf.surface.keyboard_focus end
  ]]})
  test.ipc("polkit","view","policy-1","Fixture","test.action","user","","waiting","",0)
  test.advance(700) test.eq(test.ipc("focus_policy"),"exclusive")
  test.ipc("polkit","cancel") test.advance(1)
  test.eq(test.ipc("focus_policy"),"none")
  test.ipc("polkit","view","policy-2","Fixture","test.action","user","Password","asking","",0)
  test.advance(700) test.eq(test.ipc("focus_policy"),"exclusive")
  test.ipc("polkit","view","policy-2","Fixture","test.action","user","","done","",0)
  test.advance(1)
  test.eq(test.ipc("focus_policy"),"none") test.truthy(test.ipc("polkit","cancel"))
  test.eq(#test.logs("error"),0)
end)

test.it("native polkit Cancel stops its helper and completes despite a lost D-Bus caller",function()
  test.load("../shell/init.lua",{env={CAELESTIA_DRY_RUN="1"},source=[[
    local ui=require("morf.ui")
    ui.Item {width=100,height=100}
    local handler, current, closes, done, answers, starts= nil,nil,0,0,0,0
    local cancel_on_request=false
    local service={on_call=function(_,callback) handler=callback end,call=function() end,
      reply=function() end,reply_error=function() error("caller vanished") end,close=function() end}
    morf.dbus.serve=function() return service,"owned" end
    morf.socket=function()
      starts=starts+1
      return {send=function() answers=answers+1 end,flush=function() end,
        close=function() closes=closes+1 end,receive=function() return nil end}
    end
    local agent=require("lib.services.polkit_agent").serve {
      on_request=function(request)
        current=request
        if cancel_on_request then request.cancel() end
      end,
      on_done=function() done=done+1 end,
    }
    morf.ipc.start=function(cookie)
      handler({interface="org.freedesktop.PolicyKit1.AuthenticationAgent",member="BeginAuthentication",id=1,
        arguments={"test.action","Fixture","",{},cookie,{{"unix-user",{uid=1000}}}}})
    end
    morf.ipc.cancel=function() current.cancel() current.cancel() current.answer("discard") end
    morf.ipc.close=agent.close
    morf.ipc.cancel_on_request=function() cancel_on_request=true end
    morf.ipc.state=function() return {pending=next(agent.pending)~=nil,closes=closes,done=done,answers=answers,starts=starts} end
  ]]})
  test.ipc("start","cancel-1") test.advance(30)
  test.ipc("cancel") test.advance(30)
  local state=test.ipc("state")
  test.falsy(state.pending) test.eq(state.closes,1) test.eq(state.done,1) test.eq(state.answers,1)
  test.ipc("start","cancel-2") test.ipc("close") test.advance(30)
  state=test.ipc("state") test.falsy(state.pending) test.eq(state.closes,2) test.eq(state.done,2)
  test.ipc("cancel_on_request") test.ipc("start","cancel-before-helper") test.advance(30)
  state=test.ipc("state") test.falsy(state.pending) test.eq(state.done,3) test.eq(state.starts,2)
  test.eq(#test.runs(),0) test.eq(#test.logs("error"),0)
end)
