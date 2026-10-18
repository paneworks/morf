-- The lock held, as `morf -c caelestia/lock` runs it: the headless backend
-- grants the session lock and the file builds a tree per output. The door is
-- a fixture; nothing here reaches PAM.
local test=morf.test
local SOURCE=[[
  local submits=0
  local verdict="wrong"
  pcall(function()
    morf.pam=setmetatable({},{__index=function(_,name) error("the spec reached PAM: "..name) end})
  end)
  package.loaded["lib.services.accounts"]={me=function() return {name="fixture",label="Fixture",initial="F"} end}
  package.loaded["lib.services.keyboards"]={attached=function() return true end}
  package.loaded["lib.services.mpris"]={connect=function() return {state={active={}}} end}
  local auth=require("lib.util.auth")
  package.loaded["lib.util.auth"]=setmetatable({lock=function(on)
    return {
      submit=function(_,password)
        submits=submits+1
        on.on_busy(true)
        morf.timer(50,function()
          on.on_busy(false)
          if verdict=="right" and password=="right" then on.on_open() else on.on_failed("Wrong password") end
        end,false)
      end,
      listen=function() end, stop=function() end,
    }
  end},{__index=auth})
  local ctx
  local theme=require("themes").current.lock local build=require(theme)
  package.loaded[theme]=function(context) ctx=ctx or context return build(context) end
  require("init")
  morf.ipc.audit=function(answer)
    if answer then verdict=answer end
    return {state=morf.session_lock_state(),stage=ctx.stage:get(),submits=submits}
  end
]]
local function load(screens)
  test.load("../lock/init.lua",{source=SOURCE,size={1920,1080},screens=screens,
    env={CAELESTIA_STYLE="material",CAELESTIA_SCALE_MODE="compositor",CAELESTIA_DRY_RUN="1"}})
  test.settle(1500)
end
local function locks()
  local found={}
  for _,surface in ipairs(test.surfaces()) do
    if surface.kind=="lock" then found[#found+1]=surface end
  end
  return found
end

test.it("a held lock covers every screen and a wrong password keeps it",function()
  load(2)
  local surfaces=locks()
  test.eq(#surfaces,2)
  test.eq(#test.surfaces(),2,"a lock client has only its lock surfaces")
  for _,surface in ipairs(surfaces) do
    test.eq({surface.width,surface.height},{1920,1080})
  end
  test.eq(test.ipc("audit").state,"locked")
  test.eq(test.ipc("audit").stage,"rest")
  test.type("w")
  test.settle(300)
  test.eq(test.ipc("audit").stage,"sheet")
  test.type("rong")
  test.key("Return")
  test.advance(200)
  test.settle(500)
  local audit=test.ipc("audit")
  test.eq(audit.submits,1)
  test.eq(audit.state,"locked")
  test.eq(#locks(),2)
  local message=test.find {id="lock-message"}
  test.truthy(message and tostring(message.text):find("Wrong"),"no wrong: "..tostring(message and message.text))
  test.eq(#test.logs("error"),0)
end)

test.it("the right password lifts the lock and takes its surfaces away",function()
  load(1)
  test.eq(#locks(),1)
  test.ipc("audit","right")
  test.type("right")
  test.key("Return")
  test.advance(1000)
  test.settle(500)
  test.eq(test.ipc("audit").state,"unlocked")
  test.eq(#locks(),0)
  test.eq(#test.logs("error"),0)
end)
