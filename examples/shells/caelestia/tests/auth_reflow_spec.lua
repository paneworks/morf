-- Safe previews of the real lock/greeter layouts; never authenticate.
local test=morf.test
local SOURCE=[[
  package.loaded["lib.services.keyboards"]={attached=function() return false end}
  local role=morf.env("AUTH_ROLE")
  local keyboards=require("themes.auth_keyboard")
  local make=keyboards.new
  local board,context
  keyboards.new=function(ctx) context=ctx board=make(ctx) return board end
  require("init")
  morf.ipc.board=function(action)
    if action=="password" then context.method:set("password") end
    if action=="numbers" then board.keys.numbers:set(true) end
    return {shown=board.active(),reserved=board.reserved(),height=board.content_height(),stage=context.stage:get()}
  end
  morf.ipc.key_id=function(key) return context.prefix..".osk."..context.output..".key.full.letters."..key end
]]
local function load(role,style)
  test.load("../"..role.."/init.lua",{source=SOURCE,size={620,1380},
    args=role=="lock" and {"window","preview"} or {"preview"},
    env={AUTH_ROLE=role,CAELESTIA_STYLE=style,CAELESTIA_SCALE_MODE="compositor",
      CAELESTIA_DRY_RUN="1",GREETD_SOCK=false}})
  test.advance(700)
end
local function pair(x,y,dy)
  test.touch("down",10,x,y) test.touch("down",11,x+60,y)
  test.touch("move",10,x,y+dy) test.touch("move",11,x+60,y+dy)
  test.touch("up",10,x,y+dy) test.touch("up",11,x+60,y+dy)
  test.advance(900)
end
for _,style in ipairs {"material","tsugumori"} do
  for _,role in ipairs {"lock","greet"} do
    test.it(style.." "..role.." keyboard resizes the entire content viewport and restores it on hide",function()
      load(role,style)
      test.near(test.get(role.."-content").height,1380,1)
      test.ipc("stage","sheet") test.ipc("board","password") test.advance(900)
      local state=test.ipc("board")
      local body=test.get(role.."-content")
      local keyboard=test.get(role.."-keyboard")
      test.truthy(state.shown) test.truthy(body.clip)
      test.truthy(body.height<1100)
      test.near(body.height,1380-state.reserved,1)
      test.near(body.y+body.height,keyboard.y,1)
      test.truthy(test.get(role.."-sheet").y+test.get(role.."-sheet").height<keyboard.y)
      local field_y=test.get(role.."-field").y
      test.ipc("board","numbers") test.advance(900)
      test.truthy(test.get(role.."-content").height<body.height-30)
      test.truthy(test.get(role.."-field").y<field_y-30)
      local q=test.get(test.ipc("key_id","q"))
      pair(q.x+10,q.y+10,100)
      -- Hiding the lock keyboard also dismisses the account card: only
      -- an intentional upward gesture may reveal it again.
      test.eq(test.ipc("board").stage,role=="lock" and "rest" or "sheet")
      test.falsy(test.ipc("board").shown)
      test.near(test.get(role.."-content").height,1380,1)
      if role=="lock" then test.falsy(test.get("lock-field").visible)
      else test.truthy(test.get("greet-field").y>field_y) end
      pair(230,1375,-120)
      state=test.ipc("board")
      test.truthy(state.shown)
      test.near(test.get(role.."-content").height,1380-state.reserved,1)
      test.near(test.get(role.."-content").height,test.get(role.."-keyboard").y,1)
      test.eq(test.logs("error"),{})
    end)
  end
end
