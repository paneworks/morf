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
        if node.visible and node.id and node.id:find("key.fulln.letters.backspace",1,true) then back=node break end
      end
      test.truthy(back,"on-screen backspace missing")
      local x,y=back.x+back.width/2,back.y+back.height/2
      test.truthy(y<1080,"authentication keyboard is outside the output")
      test.press(x,y) test.advance(100)
      test.eq(test.ipc("count"),5)
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
