-- Visual authentication regression fixtures. Preview modes never contact PAM
-- or greetd, including when exercising error/reopen and rapid account changes.
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
  require("init")
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
local function shot(name)
  if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(name..".png") end
end
local function clean()
  test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
end
for _,part in ipairs {"lock","greet"} do
  test.it(part.." reveals on first and repeated opening, including interrupted reopens",function()
    load(part)
    shot(part.."-rest")
    test.eq(test.ipc("auth_test").busy,false)
    test.ipc("stage","sheet") test.ipc("auth_test","password") test.advance(90)
    test.truthy(test.get(part.."-auth-cover").visible)
    shot(part.."-enter-090")
    test.advance(180)
    local width=test.get(part.."-auth-cover").width
    test.truthy(width>0 and width<test.get(part.."-sheet").width,"reveal didn't move")
    shot(part.."-enter-270")
    test.advance(600)
    test.falsy(test.get(part.."-auth-cover").visible)
    shot(part.."-password")
    -- Closing midway through a reveal must cancel its old completion.
    test.ipc("stage","rest") test.advance(30)
    test.ipc("stage","sheet") test.advance(100)
    test.ipc("stage","rest") test.advance(30)
    test.ipc("stage","sheet") test.advance(60)
    test.truthy(test.get(part.."-auth-cover").visible)
    test.advance(650)
    test.truthy(test.get(part.."-sheet").visible)
    test.falsy(test.get(part.."-auth-cover").visible)
    test.type("example") test.eq(test.ipc("auth_test").typed,7)
    test.ipc("auth_test","bad") test.advance(400)
    test.eq(test.ipc("auth_test").busy,false)
    test.truthy(test.get(part.."-message").text:find("not accepted",1,true))
    shot(part.."-error")
    test.ipc("auth_test","pattern") test.advance(800)
    shot(part.."-pattern")
    clean()
  end)
  test.it(part.." clock rolls only changed digits and settles",function()
    load(part)
    test.eq(test.get(part.."-clock-5-digit").text,"2")
    local before=test.get(part.."-clock-1-digit").y
    test.ipc("clock_test","08:43") test.advance(80)
    test.eq(test.get(part.."-clock-5-digit").text,"3")
    test.near(test.get(part.."-clock-1-digit").y,before,.01)
    test.advance(700)
    test.near(test.get(part.."-clock-1-digit").y,test.get(part.."-clock-5-digit").y,.01)
    clean()
  end)
  test.it(part.." preserves Material control geometry and compact keyboard reachability",function()
    for _,size in ipairs {{1280,900,false},{500,720,true}} do
      local baseline={}
      for _,style in ipairs {"material","tsugumori"} do
        load(part,size[1],size[2],size[3],style)
        test.ipc("stage","sheet") test.ipc("auth_test","password") test.advance(1800)
        for _,suffix in ipairs {"sheet","field","submit","method"} do
          local node=test.get(part.."-"..suffix)
          if style=="material" then baseline[suffix]=node else
            for _,axis in ipairs {"x","y","width","height"} do
              test.near(node[axis],baseline[suffix][axis],1,suffix.." "..axis)
            end
          end
        end
        if size[3] then
          local keys=0
          for _,node in ipairs(test.nodes()) do
            if node.visible and node.id and node.id:find("key.fulln.letters",1,true) then
              keys=keys+1
              test.truthy(node.x>=0 and node.x+node.width<=size[1]+1,node.id.." outside screen width")
              test.truthy(node.y>=0 and node.y+node.height<=size[2]+1,node.id.." outside screen height")
            end
          end
          test.truthy(keys>20,"compact keyboard did not appear")
        end
        shot(style.."-"..part.."-"..size[1])
        clean()
      end
    end
  end)
end
