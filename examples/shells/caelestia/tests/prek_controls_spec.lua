local test=morf.test
local SOURCE=[[
  local switches={}
  local go=require("services").workspace.go
  require("services").workspace.go=function(id) switches[#switches+1]=id go(id) end
  require("init")
  morf.surface.width,morf.surface.height=tonumber(morf.env("TEST_WIDTH")),tonumber(morf.env("TEST_HEIGHT"))
  morf.ipc.prek_native_pan=function()
    local sidebar=require("sidebar")
    sidebar.select("settings")
    return sidebar.drawer.begin_drag()
  end
  -- Explicit authorization model; no root/socket acceptance is claimed here.
  local authorized_tokens={}
  __morf_prek_authorized=function(token) return authorized_tokens[token]==true end
  morf.ipc.prek_authority_model=function(token) authorized_tokens={[token]=true} end
  morf.ipc.prek_authorities_model=function(first,second) authorized_tokens={[first]=true,[second]=true} end
  morf.ipc.prek_state=function()
    local gestures=require("phone_gestures")
    return {output=(morf.screens[1] or {}).name,
      dashboard=require("dashboard").drawer.open:get(),
      dashboard_y=require("dashboard").drawer.panel.translate_y,
      sidebar=require("sidebar").drawer.open:get(),
      sidebar_x=require("sidebar").drawer.panel.translate_x,
      notifications=require("sidebar").showing("notifications"),
      settings=require("sidebar").showing("settings"),
      launcher=require("launcher").drawer.open:get(),menu=require("menus").source:get(),
      keyboard=require("keyboard").active(),mode=require("keyboard").keys.mode:get(),
      preview=gestures.workspace_preview and {active=gestures.workspace_preview.active,
        offset=gestures.workspace_preview.offset,target=gestures.workspace_preview.target},
      switches=switches}
  end
]]
local next_id=0
local function state() return test.ipc("prek_state") end
local function load(style,width,height)
  width,height=width or 600,height or 1000
  test.stub_run("task",{code=0,stdout="[]"})
  test.load("../shell/init.lua",{source=SOURCE,size={width,height},env={
    TEST_WIDTH=tostring(width),TEST_HEIGHT=tostring(height),
    CAELESTIA_STYLE=style,CAELESTIA_DRY_RUN="1",CAELESTIA_WALLPAPER=""}})
  test.advance(600)
  next_id=0
end

local function request(action,phase)
  next_id=next_id+1
  return {version=1,output=state().output,epoch=string.rep("a",32),
    gesture=string.format("%032x",next_id),ordinal=1,action=action,
    phase=phase or "discrete",dx=0,dy=0,vx=0,vy=0}
end
local function send(r) return test.ipc("prek-control",morf.json.encode(r)) end
local function accepted(r)
  local reply=send(r)
  test.eq(reply.status,"accepted")
  test.eq(reply.epoch,r.epoch) test.eq(reply.gesture,r.gesture) test.eq(reply.ordinal,r.ordinal)
end

test.it("native controls require a live grant and retire progress on revocation",function()
  load("material")
  local token=string.rep("c",32)
  local r=request("workspace_preview","begin")
  local function native() return test.ipc("prek-native-control",morf.json.encode(r),token) end
  test.eq(native().error,"unauthorized")
  test.ipc("prek_authority_model",token)
  test.eq(native().status,"accepted")
  r.phase,r.ordinal,r.dx="update",2,-400
  test.eq(native().status,"accepted")
  test.ipc("prek_authority_model","retired")
  test.advance(100)
  test.eq(state().switches,{}) test.falsy(state().preview.active)
  test.eq(native().error,"unauthorized")
end)
test.it("gesture guards cancel one controller without revoking a shared native scope",function()
  load("material")
  local first=request("workspace_preview","begin")
  local second=request("dashboard","begin")
  local prefix=string.rep("c",32)..":"
  local one=prefix..first.epoch..":"..first.gesture
  local two=prefix..second.epoch..":"..second.gesture
  local function native(r,token) return test.ipc("prek-native-control",morf.json.encode(r),token) end
  test.ipc("prek_authorities_model",one,two)
  test.eq(native(first,one).status,"accepted")
  test.eq(native(second,two).error,"busy")
  test.truthy(state().preview.active,"a different gesture guard preempted a valid controller")
  test.ipc("prek_authority_model",two)
  test.eq(native(second,two).status,"accepted")
  test.eq(native(first,one).error,"unauthorized")
  second.phase,second.ordinal="cancel",2
  test.eq(native(second,two).status,"accepted")
  test.advance(600)
  test.falsy(state().preview.active)
  test.falsy(state().dashboard)
  test.eq(state().switches,{})
  test.eq(test.logs("error"),{})
end)

test.it("native revocation after end prevents the delayed workspace command",function()
  load("material")
  local token=string.rep("c",32)
  test.ipc("prek_authority_model",token)
  local r=request("workspace_preview","begin")
  local function native() return test.ipc("prek-native-control",morf.json.encode(r),token) end
  test.eq(native().status,"accepted")
  r.phase,r.ordinal,r.dx="update",2,-400 test.eq(native().status,"accepted")
  r.phase,r.ordinal="end",3 test.eq(native().status,"accepted")
  test.ipc("prek_authority_model","retired")
  test.advance(600)
  test.eq(state().switches,{}) test.falsy(state().preview.active)
end)

test.it("authentication after end prevents an ordinary delayed workspace command",function()
  load("material")
  local r=request("workspace_preview","begin") accepted(r)
  r.phase,r.ordinal,r.dx="update",2,-400 accepted(r)
  r.phase,r.ordinal="end",3 accepted(r)
  test.ipc("session","open") test.advance(600)
  test.eq(state().switches,{}) test.falsy(state().preview.active)
end)

test.it("native token replacement cancels the previous gesture and preserves ordinary deduplication",function()
  load("material")
  local ordinary=request("apps") accepted(ordinary)
  local token=string.rep("c",32)
  test.ipc("prek_authority_model",token)
  local r=request("workspace_preview","begin")
  local function native() return test.ipc("prek-native-control",morf.json.encode(r),token) end
  test.eq(native().status,"accepted")
  r.phase,r.ordinal,r.dx="update",2,-400 test.eq(native().status,"accepted")
  token=string.rep("d",32) test.ipc("prek_authority_model",token)
  r=request("apps") test.eq(native().status,"accepted")
  test.advance(600)
  test.eq(state().switches,{}) test.falsy(state().preview.active)
  test.eq(send(ordinary).error,"duplicate")
  test.eq(test.ipc("prek-status",state().output).identities,1)
end)

for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." typed Prek controls reuse real drawers, menus and keyboard",function()
    load(style)
    local status=test.ipc("prek-status",state().output)
    test.eq(status.version,1) test.falsy(status.recognizer_ownership)
    test.falsy(status.active) test.eq(status.identities,0)
    test.falsy(test.ipc("prek-status","unrelated-output"))
    for _,action in ipairs {"dashboard","notifications","quick_settings","apps","web"} do
      accepted(request(action)) test.advance(600)
      local s=state()
      test.eq(s.dashboard,action=="dashboard")
      test.eq(s.sidebar,action=="notifications" or action=="quick_settings")
      test.eq(s.launcher,action=="apps" or action=="web")
      if action=="notifications" then test.truthy(s.notifications) end
      if action=="quick_settings" then test.truthy(s.settings) end
      if action=="apps" or action=="web" then test.eq(s.menu,action) end
    end
    accepted(request("keyboard_show")) test.advance(600)
    test.truthy(state().keyboard) test.eq(state().mode,"full")
    local toggle=request("keyboard_toggle_layout")
    accepted(toggle) test.eq(state().mode,"dev")
    test.eq(send(toggle).error,"duplicate") test.eq(state().mode,"dev")
    accepted(request("keyboard_hide")) test.advance(600)
    test.falsy(state().keyboard)
    test.eq(send(request("keyboard_toggle_layout")).error,"unavailable")
    test.eq(test.logs("error"),{})
  end)
  test.it(style.." typed Prek progress moves the existing drawer and cancels to its original state",function()
    load(style)
    local r=request("dashboard","begin")
    accepted(r)
    local origin=state().dashboard_y
    r.phase,r.ordinal,r.dy="update",2,-80
    accepted(r)
    test.truthy(math.abs(state().dashboard_y-origin)>1)
    test.eq(send(r).error,"out_of_order")
    r.ordinal=4 test.eq(send(r).error,"out_of_order")
    r.phase,r.ordinal="cancel",3 accepted(r)
    test.advance(600) test.falsy(state().dashboard)
    r.phase,r.ordinal="end",4 test.eq(send(r).error,"retired")
    test.eq(test.logs("error"),{})
  end)
  test.it(style.." typed Prek workspace preview retains partial return, commit and frozen output",function()
    load(style)
    local r=request("workspace_preview","begin")
    accepted(r)
    r.phase,r.ordinal,r.dx="update",2,-80 accepted(r)
    test.near(state().preview.offset,-80,1) test.eq(state().switches,{})
    r.phase,r.ordinal="end",3 accepted(r)
    test.advance(600) test.eq(state().switches,{}) test.falsy(state().preview.active)
    r=request("workspace_preview","begin") accepted(r)
    r.phase,r.ordinal,r.dx="update",2,-400 accepted(r)
    test.eq(state().preview.target,2) test.eq(state().switches,{})
    local original=r.output
    r.output="unrelated-output" r.ordinal=3
    test.falsy(send(r)) test.near(state().preview.offset,-400,1)
    r.output=original r.phase="end" accepted(r)
    test.advance(600) test.eq(state().switches,{2}) test.falsy(state().preview.active)
    test.eq(test.logs("error"),{})
  end)
  test.it(style.." typed Prek progress expires and authentication cancels without committing",function()
    load(style)
    local r=request("workspace_preview","begin") accepted(r)
    r.phase,r.ordinal,r.dx="update",2,-400 accepted(r)
    test.advance(1100) test.eq(state().switches,{}) test.falsy(state().preview.active)
    r=request("workspace_preview","begin") accepted(r)
    r.phase,r.ordinal,r.dx="update",2,-400 accepted(r)
    test.ipc("session","open") test.advance(600)
    test.eq(state().switches,{}) test.falsy(state().preview.active)
    test.eq(send(request("apps")).error,"blocked")
    test.eq(test.logs("error"),{})
  end)
  test.it(style.." typed Prek progress has a hard lifetime despite keepalive updates",function()
    load(style)
    local r=request("workspace_preview","begin") accepted(r)
    for ordinal=2,6 do
      r.phase,r.ordinal,r.dx="update",ordinal,-400 accepted(r)
      test.advance(400)
    end
    r.ordinal=7 test.eq(send(r).error,"retired")
    test.advance(600) test.eq(state().switches,{}) test.falsy(state().preview.active)
    test.eq(test.logs("error"),{})
  end)
  test.it(style.." typed Prek desktop sidebar progress follows its horizontal drawer axis",function()
    load(style,1200,800)
    local r=request("notifications","begin") accepted(r)
    local origin=state().sidebar_x
    r.phase,r.ordinal,r.dx="update",2,-80 accepted(r)
    test.truthy(math.abs(state().sidebar_x-origin)>1)
    r.phase,r.ordinal="cancel",3 accepted(r)
    test.advance(600) test.falsy(state().sidebar)
    test.eq(test.logs("error"),{})
  end)
  test.it(style.." typed Prek refuses a busy native drawer before selecting another page",function()
    load(style)
    test.truthy(test.ipc("prek_native_pan")) test.truthy(state().settings)
    test.eq(send(request("notifications","begin")).error,"busy")
    test.truthy(state().settings) test.falsy(state().notifications)
    test.eq(test.logs("error"),{})
  end)
end
test.it("typed Prek controls reject malformed, future and mixed requests before changing UI",function()
  load("material")
  for _,payload in ipairs {"{",string.rep("x",2049),"[]","null","1"} do
    test.eq(test.ipc("prek-control",payload).error,"invalid_request")
  end
  for _,change in ipairs {
    {version=2},{ordinal=0},{ordinal=1.5},{epoch="bad"},{gesture="bad"},
    {action="unlock"},{phase="oops"},{dx="100"},{dx=1000001},{unknown=true},{output="bad\noutput"},
  } do
    local r=request("dashboard")
    for key,value in pairs(change) do r[key]=value end
    test.eq(send(r).error,"invalid_request")
  end
  test.falsy(state().dashboard)
  test.eq(test.logs("error"),{})
end)
