-- Window/preview only: no PAM or greetd authentication. Capture only length,
-- never the controller's input buffer, to verify hidden OSK cancellation.
local test=morf.test
for _,style in ipairs {"material","tsugumori"} do
  for _,part in ipairs {"lock","greet"} do
    test.it(style.." "..part.." cancels held keys when the authentication sheet hides",function()
      test.load("../"..part.."/init.lua",{size={1920,1080},args=part=="lock" and {"window","preview"} or {"preview"},
        env={CAELESTIA_STYLE=style,CAELESTIA_DRY_RUN="1",TEST_AUTH_PART=part},source=[[
          package.loaded["lib.services.keyboards"]={attached=function() return false end}
          local part=morf.env("TEST_AUTH_PART")
          local name=require("themes").current[part]
          local build=require(name)
          local state
          package.loaded[name]=function(ctx) state=ctx return build(ctx) end
          require("init")
          morf.ipc.count=function() return state.typed:get() end
        ]]})
      test.advance(100) test.ipc("stage","sheet") test.advance(2500)
      test.type("abcdef") test.eq(test.ipc("count"),6)
      local back
      for _,node in ipairs(test.nodes()) do
        if node.visible and node.id and node.id:find("key.full.letters.backspace",1,true) then back=node break end
      end
      test.truthy(back,"on-screen backspace missing")
      local x,y=back.x+back.width/2,back.y+back.height/2
      test.truthy(y<1080,"authentication keyboard is outside the output")
      test.press(x,y) test.advance(100)
      test.eq(test.ipc("count"),6)
      test.ipc("stage","rest")
      local count=test.ipc("count")
      test.advance(900) test.eq(test.ipc("count"),count,"hidden authentication keyboard kept deleting")
      test.release(x,y)
      test.eq(test.ipc("count"),count)
      test.eq(test.ipc("stage"),"rest")
      test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
    end)
  end
end

do
local test=morf.test
local HOST=[=[
  local role=morf.env("AUTH_ROLE")
  package.loaded["lib.services.keyboards"]={attached=function() return false end}
  local visual=require("themes").current
  local build=require(visual[role])
  local auth,board,options
  package.loaded[visual[role]]=function(ctx) auth=ctx return build(ctx) end
  local keyboards=require("themes.auth_keyboard")
  local make=keyboards.new
  keyboards.new=function(ctx) options=ctx board=make(ctx) return board end
  require("init")
  morf.ipc.state=function() return {stage=auth.stage:get(),open=board.active(),
    mode=board.keys.mode:get(),numbers=board.keys.numbers:get(),typed=auth.typed:get()} end
  morf.ipc.key_id=function(key) return options.prefix..".osk."..options.output..".key."..board.keys.mode:get()..".letters."..key end
  morf.ipc.draft=function() auth.type_text("x") end
  morf.ipc.password=function() auth.method:set("password") end
]=]
local function load(role,style)
  test.load("../"..role.."/init.lua",{source=HOST,size={620,1380},
    args=role=="lock" and {"window","preview"} or {"preview"},
    env={AUTH_ROLE=role,CAELESTIA_STYLE=style,CAELESTIA_SCALE_MODE="compositor",
      CAELESTIA_DRY_RUN="1",GREETD_SOCK=false}})
  test.advance(600)
end
local function state() return test.ipc("state") end
local function pair(x,y,dy)
  test.touch("down",10,x,y) test.touch("down",11,x+60,y)
  test.touch("move",10,x,y+dy) test.touch("move",11,x+60,y+dy)
  test.touch("up",10,x,y+dy) test.touch("up",11,x+60,y+dy)
  test.advance(500)
end
for _,style in ipairs {"material","tsugumori"} do
  for _,role in ipairs {"lock","greet"} do
    test.it(style.." "..role.." shares the large keyboard and its two-finger gestures",function()
      load(role,style)
      test.swipe({310,1375},{310,1130},{duration=300}) test.advance(900)
      test.eq(state().stage,"sheet") test.ipc("password") test.advance(300) test.truthy(state().open)
      local q=test.get(test.ipc("key_id","q"))
      test.truthy(q.height>=56,"Authentication shrank the shared keyboard")
      test.falsy(state().numbers)
      local field=test.get(role.."-field")
      test.truthy(field.y+field.height<test.get(role.."-keyboard").y)
      pair(q.x+10,q.y+20,-100)
      test.eq(state().mode,"dev") test.eq(state().typed,0)
      q=test.get(test.ipc("key_id","q"))
      pair(q.x+10,q.y+20,-100)
      test.eq(state().mode,"full") test.eq(state().typed,0)
      q=test.get(test.ipc("key_id","q"))
      pair(q.x+10,q.y+10,100)
      test.falsy(state().open) test.eq(state().stage,"sheet")
      pair(210,700,-100) test.falsy(state().open)
      pair(210,1375,-120) test.truthy(state().open) test.eq(state().mode,"full")
      test.eq(state().typed,0)
      test.eq(test.logs("error"),{})
    end)
    test.it(style.." "..role.." swipes never erase a draft and downward sheet swipes stay locked",function()
      load(role,style) test.ipc("stage","sheet") test.advance(900)
      test.ipc("draft")
      local back=test.get(test.ipc("key_id","backspace"))
      pair(back.x+back.width/2-60,back.y+back.height/2,-100)
      test.eq(state().typed,1)
      local q=test.get(test.ipc("key_id","q"))
      test.touch("down",0,q.x+10,q.y+10) test.touch("up",0,q.x+10,q.y+10)
      test.eq(state().typed,2)
      local handle=test.get(role.."-sheet-handle")
      test.swipe({handle.x+handle.width/2,handle.y+10},{handle.x+handle.width/2,handle.y+140},{duration=300})
      test.advance(900)
      test.eq(state().stage,"rest") test.eq(state().typed,0) test.falsy(state().open)
      test.eq(test.logs("error"),{})
    end)
  end
end

end
