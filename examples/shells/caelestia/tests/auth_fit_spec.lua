-- Safe previews: adaptive geometry on both skins and both authentication screens.
local test=morf.test
local source=[[
  local morf=require("morf")
  local accounts={
    {name="akari",label="Akari",initial="A"},
    {name="ren",label="Ren",initial="R"},
  }
  package.loaded["lib.accounts"]={me=function() return accounts[1] end,list=function() return accounts end}
  package.loaded["lib.sessions"]={list=function() return {
    {name="Hyprland",command={"false"}},{name="Plasma",command={"false"}}} end,default_index=function() return 1 end}
  package.loaded["lib.keyboards"]={attached=function() return morf.env("AUTH_TOUCH")~="1" end}
  package.loaded["lib.lule"]={watch=function() return {get=function() return nil end} end}
  package.loaded["lib.weather"]={new=function() return {get=function() return {} end} end,material_symbol=function() return "cloud" end}
  package.loaded["lib.mpris"]={connect=function() return {state={active={}}} end}
  local part=morf.env("AUTH_PART")
  local path=require("themes").current[part]
  local build=require(path)
  local ctx
  local clock=morf.signal("fixture.clock","08:42")
  package.loaded[path]=function(context)
    ctx=context ctx.clock=clock ctx.day={get=function() return "Tuesday, 29 September" end}
    return build(ctx)
  end
  local ui=require("morf.ui")
  local presentation
  local item,rect=ui.Item,ui.Rect
  local function capture(constructor)
    return function(props)
      local node=constructor(props)
      if props.id==part.."-output" then presentation=node end
      return node
    end
  end
  ui.Item,ui.Rect=capture(item),capture(rect)
  require("init")
  ui.Item,ui.Rect=item,rect
  -- The harness keeps its canvas size fixed. Resize the output under a
  -- parent to drive the same layout feedback a compositor configure supplies.
  presentation.anchors={fill=false}
  presentation.width,presentation.height=morf.screens[1].width,morf.screens[1].height
  local canvas=ui.Item {width=3200,height=2400}
  ui.reparent(presentation,canvas)
  morf.ipc.resize_test=function(w,h)
    presentation.anchors={fill=false}
    presentation.width,presentation.height=tonumber(w),tonumber(h)
  end
  morf.ipc.clock_test=function(value) clock:set(value) end
  morf.ipc.auth_test=function(action)
    if action=="bad" then ctx.bad:set(true) ctx.message:set("Password not accepted. Try again.") end
    if action=="password" then ctx.method:set("password") end
    if action=="pattern" then ctx.method:set("pattern") end
    return {typed=ctx.typed:get(),busy=ctx.busy:get(),method=ctx.method:get(),bad=ctx.bad:get()}
  end
]]
local function load(part,w,h,touch,style)
  test.load("../"..part.."/init.lua",{size={w or 1920,h or 1080},source=source,
    args=part=="lock" and {"window","preview"} or {"preview"},
    env={CAELESTIA_STYLE=style or "tsugumori",CAELESTIA_DRY_RUN="1",GREETD_SOCK=false,
      AUTH_PART=part,AUTH_TOUCH=touch and "1" or "0"}})
  test.advance(1200)
end

for _,part in ipairs {"lock","greet"} do
  test.it(part.." fits short, portrait and ultrawide outputs without clipping controls",function()
    for _,style in ipairs {"material","tsugumori"} do
      for _,size in ipairs {{320,240},{640,360},{800,480},{500,720},{1080,1920},{3440,1440}} do
        load(part,size[1],size[2],true,style)
        if part=="greet" and size[2]<375 then
          local clock,people,power=test.get("greet-glance"),test.get("greet-people"),test.get("greet-power")
          test.truthy(clock.y+clock.height<=people.y+1,"clock overlaps accounts on a short output")
          test.truthy(people.y+people.height<=power.y+1,"accounts overlap power controls")
        end
        if morf.env("MORF_THEME_SNAPSHOTS")=="1" and style=="tsugumori" and size[1]<=800 then
          test.snapshot(part.."-"..size[1].."x"..size[2].."-rest.png")
        end
        for _,method in ipairs {"password","pattern"} do
          test.ipc("stage","sheet") test.ipc("auth_test",method) test.advance(1200)
          local sheet=test.get(part.."-sheet")
          test.truthy(sheet.y>=0,"sheet above output: "..style.." "..size[1].."x"..size[2].." "..method)
          test.truthy(sheet.x>=0 and sheet.x+sheet.width<=size[1]+1,"sheet too wide")
          test.truthy(sheet.y+sheet.height<=size[2]+1,"sheet below output")
          if part=="greet" and size[2]<375 then
            test.truthy(sheet.y+sheet.height<=test.get("greet-power").y+1,"power controls overlap the login sheet")
          end
          local viewport=test.get(part.."-sheet-scroll")
          local keys=0
          for _,node in ipairs(test.nodes()) do
            if node.visible and node.id and (node.id:find("key.fulln.letters",1,true) or node.id:match("%.pattern$")) then
              keys=keys+1
              test.truthy(node.x>=0 and node.x+node.width<=size[1]+1,node.id.." outside width")
              test.truthy(node.height>=30,node.id.." shrunk below a usable target")
              if node.id:match("%.pattern$") then
                test.truthy(node.y>=viewport.y and node.y+node.height<=viewport.y+viewport.height+1,"pattern clipped")
              end
            end
          end
          test.truthy(keys>0)
          test.truthy(viewport.y>=0 and viewport.y+viewport.height<=size[2]+1)
          local submit=test.get(part.."-submit")
          if method=="password" then
            test.truthy(submit.y>=viewport.y and submit.y+submit.height<=viewport.y+viewport.height+1,"submit clipped")
          end
          if morf.env("MORF_THEME_SNAPSHOTS")=="1" and style=="tsugumori" and method=="password" then
            test.snapshot(part.."-"..size[1].."x"..size[2]..".png")
          end
          if method=="password" and size[2]<375 then
            test.wheel(0,1000,{x=sheet.x+sheet.width/2,y=sheet.y+sheet.height/2}) test.advance(300)
            local enter
            for _,node in ipairs(test.nodes()) do
              if node.visible and node.id and node.id:find("key.fulln.letters.enter",1,true) then enter=node break end
            end
            test.truthy(enter,"scrolling never revealed the keyboard's final row")
            test.truthy(enter.y>=viewport.y and enter.y+enter.height<=viewport.y+viewport.height+1,"keyboard's final row cannot be reached")
            if morf.env("MORF_THEME_SNAPSHOTS")=="1" and style=="tsugumori" then
              test.snapshot(part.."-"..size[1].."x"..size[2].."-keyboard.png")
            end
          end
        end
        test.eq(#test.logs("error"),0)
      end
    end
  end)
  for _,style in ipairs {"material","tsugumori"} do
    test.it(style.." "..part.." rebuilds after resize and rotation without losing its draft",function()
      load(part,1920,1080,false,style)
      test.ipc("stage","sheet") test.ipc("auth_test","password") test.advance(1000)
      test.type("disposable") test.eq(test.ipc("auth_test").typed,10)
      for _,size in ipairs {{500,720},{800,480},{320,240},{2560,1440},{720,500}} do
        test.ipc("resize_test",tostring(size[1]),tostring(size[2])) test.advance(1100)
        local sheet=test.get(part.."-sheet")
        test.truthy(sheet.x>=0 and sheet.x+sheet.width<=size[1]+1)
        test.truthy(sheet.y>=0 and sheet.y+sheet.height<=size[2]+1)
        test.eq(test.ipc("auth_test").typed,10)
        test.falsy(test.ipc("auth_test").busy)
        local boards=0
        for _,node in ipairs(test.nodes()) do
          if node.id==part.."-sheet" then boards=boards+1 end
        end
        test.eq(boards,1,"resize retained the old output tree")
      end
      test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
    end)
    test.it(style.." "..part.." cancels a held keyboard key when resizing replaces its tree",function()
      load(part,500,720,true,style)
      test.ipc("stage","sheet") test.ipc("auth_test","password") test.advance(1000)
      test.type("abcdef")
      local back
      for _,node in ipairs(test.nodes()) do
        if node.visible and node.id and node.id:find("key.fulln.letters.backspace",1,true) then back=node break end
      end
      test.truthy(back)
      test.press(back.x+back.width/2,back.y+back.height/2) test.advance(100)
      test.eq(test.ipc("auth_test").typed,5)
      test.ipc("resize_test","800","480") test.advance(1200)
      test.eq(test.ipc("auth_test").typed,5,"destroyed keyboard kept repeating backspace")
      test.release(300,300) test.advance(100)
      test.eq(test.ipc("auth_test").typed,5)
      test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
    end)
  end
end

for _,part in ipairs {"greet","lock"} do
  for _,style in ipairs {"material","tsugumori"} do
    test.it(style.." "..part.." accepts physical keys after clicking the sheet and after resize",function()
      load(part,1920,1080,false,style)
      test.key("Return") test.advance(1200)
      test.ipc("auth_test","password") test.advance(500)
      for _,id in ipairs {part.."-sheet-scroll",part.."-field"} do
        local n=test.get(id)
        test.click(n.x+n.width/2,n.y+n.height/2)
        test.type("a")
      end
      test.eq(test.ipc("auth_test").typed,2)
      test.ipc("resize_test","800","480") test.advance(1200)
      local n=test.get(part.."-field")
      test.click(n.x+n.width/2,n.y+n.height/2) test.type("b")
      test.eq(test.ipc("auth_test").typed,3)
      test.key("BackSpace") test.eq(test.ipc("auth_test").typed,2)
      test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
    end)
  end
end
