local test=morf.test
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." keyring supports unlock, new-password confirmation and cancellation",function()
    test.load("../shell/init.lua",{env={CAELESTIA_STYLE=style,CAELESTIA_DRY_RUN="1"},source=[[
      require("init")
    ]]})
    test.truthy(test.ipc("keyring","demo")) test.advance(1800)
    test.truthy(test.get("keyring-title").visible)
    test.truthy(test.get("keyring-field-1").visible)
    test.eq(test.get("keyring-field-2").visible,false)
    test.type("disposable") test.key("Return") test.advance(1000)
    test.eq(test.ipc("keyring").open,false)
    test.ipc("keyring","demo","new") test.advance(1600)
    test.truthy(test.get("keyring-field-2").visible)
    test.type("new-password") test.key("Return") test.type("mismatch")
    test.click("keyring-continue") test.advance(50)
    test.eq(test.get("keyring-warning").text,"Passwords do not match")
    test.truthy(test.ipc("keyring").open)
    test.key("Escape") test.advance(1000)
    test.eq(test.ipc("keyring").open,false)
    test.ipc("keyring","demo","confirm") test.advance(1600)
    test.eq(test.get("keyring-field-1").visible,false)
    test.click("keyring-continue") test.advance(1000)
    test.eq(test.ipc("keyring").open,false)
    test.eq(#test.logs("error"),0)
  end)
end
test.it("live keyring demos are disposable and yield to real requests",function()
  test.load("../shell/init.lua",{env={CAELESTIA_STYLE="tsugumori",CAELESTIA_DRY_RUN="0"},size={800,650},source=[[
    local ui=require("morf.ui")
    local callbacks,answers,cancels
    answers,cancels=0,0
    package.loaded["lib.services.keyring_agent"]={serve=function(options)
      callbacks=options
      options.on_status({"org.gnome.keyring.SystemPrompter"})
      return {close=function() end}
    end}
    local keyring=require("keyring")
    morf.surface.height=650
    ui.Item {width=800,height=650,
      ui.Sdf {width=800,height=650,keyring.drawer.shape},keyring.drawer.panel}
    morf.ipc.demo=keyring.demo
    morf.ipc.real=function()
      local request={kind="password",properties={title="Real request fixture",message="Unlock fixture"}}
      request.answer=function() answers=answers+1 callbacks.on_close(request) end
      request.cancel=function() cancels=cancels+1 callbacks.on_close(request) end
      callbacks.on_request(request)
    end
    morf.ipc.state=function() return {open=keyring.opened:get(),length=keyring.lengths[1]:get(),answers=answers,cancels=cancels} end
  ]]})
  test.truthy(test.ipc("demo")) test.advance(1800)
  test.type("test") test.key("Return") test.advance(700)
  test.falsy(test.ipc("state").open)
  test.eq(test.ipc("state").answers,0)
  test.truthy(test.ipc("demo")) test.advance(1000)
  test.type("discard-this-demo")
  test.ipc("real") test.advance(100)
  test.eq(test.ipc("state").length,0)
  test.falsy(test.ipc("demo"))
  test.eq(test.ipc("state").cancels,0)
  test.type("fixture-only-secret") test.key("Return") test.advance(700)
  test.eq(test.ipc("state").answers,1)
  test.falsy(test.ipc("state").open)
  test.eq(#test.runs(),0)
  test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
end)
