-- Real shell/controller/layout behavior under explicit native-authority models.
-- These tests do not claim root, logind or kernel descriptor-release acceptance.
local test=morf.test
local TOKEN=string.rep("c",32)
local SOURCE=[[
  local native_enabled,target=true,false
  __morf_global_recognizers_enabled=function() return native_enabled end
  __morf_prek_handoff_authorized=function(token,enabled)
    return token==string.rep("c",32) and enabled==target
  end
  local switches={}
  require("services").workspace.go=function(id) switches[#switches+1]=id end
  require("init")
  morf.surface.width,morf.surface.height=1116,2484
  morf.ipc.native_policy_model=function(enabled,desired) native_enabled,target=enabled,desired end
  morf.ipc.local_keyboard=function() require("keyboard").show("full") end
  morf.ipc.local_sidebar=function() local sidebar=require("sidebar") sidebar.select("settings") sidebar.drawer.set(true) end
  morf.ipc.handoff_ui_state=function()
    local gestures=require("phone_gestures")
    return {enabled=gestures.global_allowed(),switches=switches,
      preview=gestures.workspace_preview and gestures.workspace_preview.active or false,
      dashboard=require("dashboard").drawer.open:get(),sidebar=require("sidebar").drawer.open:get(),
      tab=require("sidebar").tab:get(),keyboard=require("keyboard").active(),
      keyboard_mode=require("keyboard").keys.mode:get()}
  end
]]
local function load(style,driver)
  test.stub_run("task",{code=0,stdout="[]"})
  test.load("../shell/init.lua",{source=SOURCE,size={1116,2484},env={
    CAELESTIA_STYLE=style,CAELESTIA_DRY_RUN="1",CAELESTIA_WALLPAPER="",CAELESTIA_GESTURE_DRIVER=driver or ""}})
  test.advance(500)
end
local function state() return test.ipc("handoff_ui_state") end
local function quiesce()
  test.ipc("native_policy_model",false,false)
  return test.ipc("prek-native-handoff",TOKEN,false)
end

for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." handoff cancels an in-flight workspace preview and blocks new global gestures",function()
    load(style)
    test.falsy(test.ipc("prek-native-handoff","forged",false))
    test.touch("down",0,850,2480) test.touch("move",0,350,2480)
    test.truthy(state().preview)
    local ack=quiesce() test.truthy(ack.ready) test.eq(ack.driver,"native")
    test.falsy(state().enabled) test.falsy(state().preview)
    test.touch("up",0,350,2480) test.advance(700)
    test.eq(state().switches,{})
    test.swipe({558,2480},{558,2200}) test.swipe({558,4},{558,300}) test.advance(700)
    test.falsy(state().dashboard) test.falsy(state().sidebar) test.eq(state().switches,{})
  end)
  test.it(style.." handoff preserves local panel navigation and keyboard gestures",function()
    load(style) test.truthy(quiesce().ready)
    test.ipc("local_sidebar") test.advance(700)
    local page=test.get("sidebar-pages")
    local x,y=page.x+page.width/2,page.y+4
    local old=state().tab
    test.swipe({x-180,y},{x+180,y}) test.advance(700)
    test.truthy(state().tab~=old)
    test.ipc("local_keyboard") test.advance(700)
    test.truthy(state().keyboard)
    local key=test.get("caelestia.osk.key.full.letters.q")
    local px,py=key.x+10,key.y+20
    test.touch("down",0,px,py) test.touch("down",1,px+80,py)
    test.touch("move",0,px,py-100) test.touch("move",1,px+80,py-100)
    test.touch("up",0,px,py-100) test.touch("up",1,px+80,py-100) test.advance(700)
    test.eq(state().keyboard_mode,"dev") test.falsy(state().enabled)
  end)
  test.it(style.." release prepares UI but native gate controls when global recognition resumes",function()
    load(style) test.truthy(quiesce().ready)
    test.ipc("native_policy_model",false,true)
    test.truthy(test.ipc("prek-native-handoff",TOKEN,true).ready)
    test.falsy(state().enabled)
    test.swipe({558,4},{558,300}) test.advance(700) test.falsy(state().sidebar)
    test.ipc("native_policy_model",true,true)
    test.truthy(state().enabled)
    test.swipe({558,4},{558,300}) test.advance(700) test.truthy(state().sidebar)
  end)
end
test.it("lisgd cannot acknowledge quiesce while its external recognizer is unmanaged",function()
  load("material","lisgd")
  local ack=quiesce() test.falsy(ack.ready) test.eq(ack.driver,"lisgd")
  test.swipe({558,4},{558,300})
  test.falsy(test.ipc("phone-gesture","top")) test.falsy(state().sidebar)
end)
