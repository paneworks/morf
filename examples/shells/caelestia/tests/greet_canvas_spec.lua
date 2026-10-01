-- Preview only: a single stable login view, never PAM or a live greeter.
local test=morf.test
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." greeter stays on one display despite pointer and monitor metadata changes",function()
    test.load("../greet/init.lua",{size={900,600},args={"preview"},env={CAELESTIA_STYLE=style,CAELESTIA_DRY_RUN="1"},source=[[
      morf.capabilities={desktop_canvas=true}
      morf.screens={{name="eDP-1",x=-800,y=70,width=800,height=650},
        {name="DP-5",x=0,y=0,width=600,height=720}}
      package.loaded["lib.accounts"]={list=function() return {{name="fixture",label="Fixture",initial="F"}} end}
      package.loaded["lib.sessions"]={list=function() return {{name="Fixture",command={"false"}}} end,default_index=function() return 1 end}
      package.loaded["lib.keyboards"]={attached=function() return true end}
      local revision=morf.signal("fixture.revision",0)
      morf.screens_revision=function() return revision:get() end
      local ctx,count
      count=0
      local path=require("themes").current.greet local build=require(path)
      package.loaded[path]=function(context) ctx=context count=count+1 return build(context) end
      local ui=require("morf.ui") local root,item
      item=ui.Item
      ui.Item=function(props) local node=item(props) if props.id=="greet-output" then root=node end return node end
      require("init") ui.Item=item
      root.anchors={fill=false} root.width,root.height=900,600
      ui.reparent(root,item {width=3000,height=2200})
      morf.ipc.fixture=function(action,w,h)
        if action=="monitors" then
          morf.screens={{name="DP-5",x=500,y=-600,width=3840,height=2160}}
          revision:set(revision:get()+1)
        elseif action=="resize" then root.width,root.height=tonumber(w),tonumber(h) end
        return {typed=ctx.typed:get(),busy=ctx.busy:get(),stage=ctx.stage:get(),main=ctx.main(),
          builds=count,keyboard=morf.surface.keyboard_focus}
      end
    ]]})
    test.advance(1200)
    local view=test.get("greet-view.main")
    test.eq(view.x,0) test.eq(view.y,0) test.eq(view.width,900) test.eq(view.height,600)
    test.type("disposable") test.advance(700)
    local state=test.ipc("fixture") test.eq(state.typed,10)
    local count=state.builds
    test.move(1100,300) test.advance(250) test.move(100,100) test.advance(250)
    test.ipc("fixture","monitors") test.advance(500)
    state=test.ipc("fixture")
    test.truthy(state.main) test.eq(state.typed,10) test.falsy(state.busy)
    test.eq(state.builds,count,"metadata or pointer movement rebuilt the login screen")
    test.eq(state.keyboard,"exclusive")
    for _,size in ipairs {{480,320},{540,960},{1240,900}} do
      test.ipc("fixture","resize",size[1],size[2]) test.advance(700)
      view=test.get("greet-view.main")
      test.eq(view.width,size[1]) test.eq(view.height,size[2])
      test.eq(test.ipc("fixture").typed,10)
      local views=0
      for _,node in ipairs(test.nodes()) do
        if node.id and node.id:find("greet-view.",1,true) then views=views+1 end
      end
      test.eq(views,1)
    end
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
end
