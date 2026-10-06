-- A phone: every surface at a Fairphone 6's logical size at the scale the
-- shell is meant for there (868 x 1932), in both themes. Nothing a person touches may fall off
-- the screen; MORF_PHONE_SNAPSHOTS=1 keeps a picture of each step in
-- snapshots/phone/.
local test=morf.test
local W,H=tonumber(morf.env("PHONE_W") or "") or 868,tonumber(morf.env("PHONE_H") or "") or 1932
local SHOTS=morf.env("MORF_PHONE_SNAPSHOTS")=="1"
local function shot(name) if SHOTS then test.snapshot("phone/"..name..".png") end end
local source=[[
  local morf=require("morf")
  local accounts={
    {name="akari",label="Akari",initial="A"},
    {name="ren",label="Ren",initial="R"},
  }
  package.loaded["lib.services.accounts"]={me=function() return accounts[1] end,list=function() return accounts end}
  package.loaded["lib.services.sessions"]={list=function() return {
    {name="Hyprland",command={"false"}},{name="Plasma",command={"false"}}} end,default_index=function() return 1 end}
  package.loaded["lib.services.keyboards"]={attached=function() return morf.env("AUTH_TOUCH")~="1" end}
  package.loaded["lib.integrations.lule"]={watch=function() return {get=function() return nil end} end}
  package.loaded["lib.integrations.weather"]={new=function() return {get=function() return {} end} end,material_symbol=function() return "cloud" end}
  package.loaded["lib.services.mpris"]={connect=function() return {state={active={}}} end}
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
local function auth(part,style)
  test.load("../"..part.."/init.lua",{size={W,H},source=source,
    args=part=="lock" and {"window","preview"} or {"preview"},
    env={CAELESTIA_STYLE=style,CAELESTIA_DRY_RUN="1",GREETD_SOCK=false,AUTH_PART=part,AUTH_TOUCH="1"}})
  test.advance(1500)
end
local function on_screen(id)
  local node=test.get(id)
  test.truthy(node.x>=0 and node.y>=0 and node.x+node.width<=W+0.5 and node.y+node.height<=H+0.5,
    id.." falls off the phone: "..node.x..","..node.y.." "..node.width.."x"..node.height)
end
for _,style in ipairs {"material","tsugumori"} do
  for _,part in ipairs {"lock","greet"} do
    test.it(style.." "..part.." on a phone: rest, sheet, keyboard",function()
      auth(part,style)
      shot(style.."-"..part.."-1-rest")
      test.click(part.."-output") test.advance(900)
      shot(style.."-"..part.."-2-sheet")
      test.ipc("auth_test","password") test.advance(900)
      shot(style.."-"..part.."-3-keyboard")
      test.eq(#test.logs("error"),0)
    end)
  end
end

-- ------------------------------------------------------------------ shell --

local SHELL=[[
  require("init")
  morf.ipc.dash_tab=function(i) require("dashboard").tab:set(tonumber(i)) end
  morf.ipc.detail=function(key) require("sidebar").select("settings") require("utilities").request(key) end
]]
local function shell(style)
  for _,program in ipairs {"systemctl","loginctl"} do test.stub_run(program,{code=0}) end
  test.stub_run("task",{code=0,stdout="[]"})
  test.load("../shell/init.lua",{size={W,H},source=SHELL,
    env={CAELESTIA_STYLE=style,CAELESTIA_WALLPAPER="",CAELESTIA_FONT_FILE="",CAELESTIA_DRY_RUN="1",
      LULE_A="/nonexistent/lule",HOME=morf.env("XDG_CACHE_HOME")}})
  test.settle(2000)
end
local PANELS={
  {"launcher",{"launcher","open"}},
  {"session",{"session","open"}},
  {"sidebar",{"sidebar","open"}},
  {"leftbar",{"leftbar","open"}},
  {"bottom",{"bottom","open"}},
  {"keyboard",{"keyboard","open"}},
  {"capture",{"capture","open"}},
  {"notifications",{"sidebar","open","notifications"}},
}
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." shell on a phone: the frame and every panel",function()
    shell(style)
    shot(style.."-shell-0-rest")
    for i,p in ipairs(PANELS) do
      test.ipc(table.unpack(p[2])) test.settle(1200)
      shot(style.."-shell-"..i.."-"..p[1])
      test.ipc("close") test.settle(900)
    end
    test.ipc("sidebar","open") test.ipc("detail","theme") test.settle(1200)
    shot(style.."-shell-theme")
    test.ipc("detail","theme/lule") test.settle(1200)
    shot(style.."-shell-lule")
    test.ipc("close") test.settle(900)
    test.ipc("dashboard","open") test.settle(1200)
    for tab=1,6 do
      test.ipc("dash_tab",tostring(tab)) test.settle(1200)
      shot(style.."-shell-dash-"..tab)
    end
    test.ipc("close") test.settle(900)
    test.eq(#test.logs("error"),0)
  end)
end
