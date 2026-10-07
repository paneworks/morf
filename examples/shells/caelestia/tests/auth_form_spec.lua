-- Password entry must remain usable throughout keyboard resizing, not just
-- fit inside a rectangle after the transition has finished. No authentication.
local test=morf.test
local SOURCE=[[
  local accounts={{name="akari",label="Akari",initial="A"}}
  package.loaded["lib.services.accounts"]={me=function() return accounts[1] end,list=function() return accounts end}
  package.loaded["lib.services.sessions"]={list=function() return {{name="Hyprland",command={"false"}}} end,default_index=function() return 1 end}
  package.loaded["lib.services.keyboards"]={attached=function() return false end}
  local keyboard=require("themes.auth_keyboard") local make=keyboard.new local board,ctx
  keyboard.new=function(options) ctx=options board=make(options) return board end
  require("init")
  morf.ipc.form=function(action)
    if action=="password" then ctx.method:set("password") end
    if action=="hide" then board.hide() end
    if action=="show" then board.show("full") end
    if action=="dev" then board.keys.mode:set("dev") end
    if action=="numbers" then board.keys.numbers:set(true) end
    return {shown=board.active(),height=board.content_height(),stage=ctx.stage:get()}
  end
]]
local function separated(role)
  local identity=test.get(role.."-identity")
  local field=test.get(role.."-field")
  local status=test.get(role.."-message")
  local sheet=test.get(role.."-sheet")
  local keyboard=test.get(role.."-keyboard")
  test.truthy(identity.y+identity.height+12<=field.y,"account overlaps the input")
  test.truthy(field.y+field.height+6<=status.y,"input overlaps the status")
  test.truthy(status.y+status.height<=sheet.y+sheet.height,"status outside its card")
  test.falsy(test.get(role.."-glance").visible,"large clock still competes with the password form")
  if keyboard.visible then
    test.truthy(sheet.y+sheet.height+12<=keyboard.y,"password card touches or overlaps keyboard")
  end
  if role=="greet" then
    local session,power=test.get("greet-session"),test.get("greet-power")
    test.truthy(status.y+status.height+8<=session.y,"status overlaps session selector")
    test.truthy(power.y+power.height+8<=sheet.y,"power controls overlap password card")
    test.falsy(test.get("greet-people").visible,"hidden account picker still receives touches")
  end
end
for _,style in ipairs {"material","tsugumori"} do
  for _,role in ipairs {"lock","greet"} do
    test.it(style.." "..role.." separates password controls at phone compositor scales",function()
      for _,size in ipairs {{744,1656},{620,1380},{507,1129},{406,903},{360,800}} do
        test.load("../"..role.."/init.lua",{source=SOURCE,size=size,
          args=role=="lock" and {"window","preview"} or {"preview"},
          env={CAELESTIA_STYLE=style,CAELESTIA_SCALE_MODE="compositor",CAELESTIA_DRY_RUN="1",GREETD_SOCK=false}})
        test.advance(700) test.ipc("stage","sheet") test.ipc("form","password")
        for _,ms in ipairs {1,40,100,300,600} do test.advance(ms) separated(role) end
        test.type("disposable") test.advance(250) separated(role)
        test.ipc("form","numbers")
        for _,ms in ipairs {1,80,400} do test.advance(ms) separated(role) end
        test.ipc("form","dev") test.advance(450) separated(role)
        if morf.env("MORF_THEME_SNAPSHOTS")=="1" and (size[1]==744 or size[1]==406) then
          test.snapshot(style.."-"..role.."-"..size[1].."-password.png")
        end
        test.ipc("form","hide") test.advance(1) separated(role)
        test.advance(450) separated(role)
        test.ipc("form","show") test.advance(1) separated(role)
        test.advance(450) separated(role)
        test.eq(#test.logs("error"),0)
      end
    end)
  end
end
