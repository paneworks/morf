local test=morf.test
test.it("one lock conversation follows the pointer between differently sized output trees",function()
  test.load("../lock/init.lua",{size={1300,720},env={CAELESTIA_STYLE="tsugumori",CAELESTIA_DRY_RUN="1"},source=[[
    local ui=require("morf.ui")
    morf.screens={{name="laptop",width=800,height=650},{name="external",width=500,height=720}}
    package.loaded["lib.services.accounts"]={me=function() return {name="fixture",label="Fixture",initial="F"} end}
    package.loaded["lib.services.keyboards"]={attached=function() return true end}
    package.loaded["lib.integrations.lule"]={watch=function() return {get=function() return nil end} end}
    package.loaded["lib.integrations.hyprland"]={json=function(_,done) done({{name="laptop",focused=true}}) end}
    local submits=0
    package.loaded["lib.util.auth"]={lock_readers=function() return {} end,
      lock=function() return {listen=function() end,stop=function() end,submit=function() submits=submits+1 end} end}
    local ctx
    local roots={}
    local canvas
    local path=require("themes").current.lock
    local build=require(path)
    package.loaded[path]=function(context) ctx=context return build(context) end
    local factory
    morf.lock_surface=function(builder) factory=builder end
    require("init")
    canvas=ui.Rect {width=1300,height=720,color="#000000"}
    local a=factory(morf.screens[1])
    a.anchors={fill=false} a.width=800 a.height=650
    local b=factory(morf.screens[2])
    b.anchors={fill=false} b.x=800 b.width=500 b.height=720
    roots={a,b}
    ui.reparent(a,canvas) ui.reparent(b,canvas)
    morf.surface.session_lock=false
    morf.surface.width=1300 morf.surface.height=720
    morf.ipc.fixture=function(action)
      if action=="sheet" then ctx.stage:set("sheet") ctx.method:set("password") end
      return {output=ctx.main_output:get(),typed=ctx.typed:get(),submits=submits}
    end
  ]]})
  test.advance(800) test.ipc("fixture","sheet") test.advance(800)
  if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot("lock-mixed-outputs.png") end
  test.move(200,100) test.type("disposable")
  test.eq(test.ipc("fixture").typed,10)
  test.move(1100,100) test.advance(800)
  local state=test.ipc("fixture")
  test.eq(state.output,"external") test.eq(state.typed,10) test.eq(state.submits,0)
  local sheets={}
  for _,node in ipairs(test.nodes()) do
    if node.id=="lock-sheet" then sheets[#sheets+1]=node end
  end
  test.eq(#sheets,2)
  test.falsy(sheets[1].visible) test.truthy(sheets[2].visible)
  test.truthy(sheets[1].height~=sheets[2].height,"outputs reused the first monitor's geometry")
  test.truthy(sheets[1].x>=0 and sheets[1].x+sheets[1].width<=800)
  test.truthy(sheets[2].x>=800 and sheets[2].x+sheets[2].width<=1300)
  test.move(200,100) test.advance(800)
  test.eq(test.ipc("fixture").output,"laptop")
  test.eq(test.ipc("fixture").typed,10)
  test.eq(#test.logs("error"),0)
end)

test.it("a greeter draft cleared before its grant never contacts authentication or leaves controls busy",function()
  test.load("../greet/init.lua",{size={800,650},env={CAELESTIA_STYLE="tsugumori",CAELESTIA_DRY_RUN="1"},source=[[
    morf.screens={{name="fixture",width=800,height=650},{name="other",width=800,height=650}}
    morf.primary=function() return true end
    package.loaded["lib.services.accounts"]={list=function() return {{name="fixture",label="Fixture",initial="F"}} end}
    package.loaded["lib.services.sessions"]={list=function() return {{name="Fixture",command={"false"}}} end,default_index=function() return 1 end}
    package.loaded["lib.services.keyboards"]={attached=function() return true end}
    local submits=0
    package.loaded["lib.util.auth"]={greeter=function(options) return {available=true,switch=function() end,
      submit=function() submits=submits+1 options.on_busy(true) end} end}
    local routing
    local factory=require("models.greet_outputs")
    package.loaded["models.greet_outputs"]=function(options) routing=factory(options) return routing end
    morf.broadcast=function(_,action,name,payload)
      morf.timer(35,function() routing.receive(action,name,payload) end,false)
      return true
    end
    local ctx
    local path=require("themes").current.greet local build=require(path)
    package.loaded[path]=function(context) ctx=context return build(context) end
    require("init")
    morf.ipc.fixture=function() return {submits=submits,busy=ctx.busy:get(),typed=ctx.typed:get(),focus=morf.surface.keyboard_focus} end
  ]]})
  test.advance(500) test.ipc("stage","sheet") test.advance(500)
  test.type("disposable") test.key("Return")
  test.ipc("greet-output","claim","other") test.advance(500)
  local state=test.ipc("fixture")
  test.eq(state.submits,0) test.falsy(state.busy) test.eq(state.typed,0) test.eq(state.focus,"none")
  test.falsy(test.get("greet-sheet").visible)
  test.ipc("greet-output","claim","fixture") test.advance(500)
  test.type("disposable") test.key("Return") test.advance(200)
  state=test.ipc("fixture") test.eq(state.submits,1) test.truthy(state.busy)
  test.eq(#test.logs("error"),0)
end)

test.it("greeter output arbitration grants one explicit submission and broadcasts no drafts",function()
  test.load("../greet/init.lua",{size={800,650},source=[[
    local ui=require("morf.ui")
    morf.screens={{name="laptop"},{name="external"}}
    morf.primary=function() return true end
    local messages,submitted,cleared={},0,0
    local routing
    morf.broadcast=function(_,action,name,payload)
      messages[#messages+1]={action=action,name=name,payload=payload}
      routing.receive(action,name,payload)
      return true
    end
    local pending=false
    routing=require("models.greet_outputs") {
      clear=function() cleared=cleared+1 end,
      busy=function(value) pending=value end,
      submit=function() submitted=submitted+1 end,
      snapshot=function() return {stage="sheet",who=1,which=1} end,
      restore=function() end,
    }
    ui.Item {width=800,height=650}
    morf.ipc.route=routing.receive
    morf.ipc.fixture=function() return {output=routing.active:get(),owner=routing.owner:get(),
      pending=pending,submitted=submitted,cleared=cleared,messages=messages} end
  ]]})
  test.advance(10)
  test.ipc("route","claim","laptop")
  test.ipc("route","request","laptop")
  test.ipc("route","request","external")
  local state=test.ipc("fixture")
  test.eq(state.owner,"laptop") test.eq(state.submitted,1) test.truthy(state.pending)
  test.ipc("route","claim","external")
  state=test.ipc("fixture")
  test.eq(state.output,"external") test.eq(state.owner,"laptop")
  test.ipc("route","release","external") test.eq(test.ipc("fixture").owner,"laptop")
  test.ipc("route","release","laptop") test.falsy(test.ipc("fixture").pending)
  test.ipc("route","request","external") test.eq(test.ipc("fixture").owner,"external")
  for _,event in ipairs(test.ipc("fixture").messages) do
    test.falsy((event.payload or ""):find("password",1,true))
  end
  test.eq(#test.logs("error"),0)
end)

for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." greeter follows keyboard focus when Cage focuses a different output",function()
    test.load("../greet/init.lua",{size={800,650},args={"preview"},env={CAELESTIA_STYLE=style,CAELESTIA_DRY_RUN="1"},source=[[
      morf.screens={{name="eDP-1",width=800,height=650},{name="DP-5",width=1920,height=1080}}
      package.loaded["lib.services.accounts"]={list=function() return {{name="fixture",label="Fixture",initial="F"}} end}
      package.loaded["lib.services.sessions"]={list=function() return {{name="Fixture",command={"false"}}} end,default_index=function() return 1 end}
      package.loaded["lib.services.keyboards"]={attached=function() return true end}
      local incoming
      morf.on_keyboard_focus=function(callback) incoming=callback end
      -- Broadcast delivery is asynchronous in the real supervisor.
      morf.broadcast=function() return true end
      local ctx
      local path=require("themes").current.greet local build=require(path)
      package.loaded[path]=function(context) ctx=context return build(context) end
      require("init")
      morf.ipc.focus_fixture=function()
        morf.capabilities={layer_shell=false}
        if incoming then incoming(true) end
      end
      morf.ipc.fixture=function() return {typed=ctx.typed:get(),stage=ctx.stage:get(),main=ctx.main(),focus=morf.surface.keyboard_focus} end
    ]]})
    test.advance(1200)
    test.falsy(test.ipc("fixture").main)
    -- No pointer enter: the compositor delivers the first typed character
    -- to its focused toplevel, which is not the alphabetically first output.
    test.type("a") test.advance(100)
    local state=test.ipc("fixture")
    test.eq(state.typed,1) test.eq(state.stage,"sheet") test.truthy(state.main)
    test.ipc("greet-output","claim","DP-5") test.advance(100)
    test.ipc("focus_fixture") test.advance(100)
    test.truthy(test.ipc("fixture").main)
    test.type("b") test.eq(test.ipc("fixture").typed,1)
    test.eq(#test.logs("error"),0)
  end)
end
