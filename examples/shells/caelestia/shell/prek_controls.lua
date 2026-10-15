-- Typed controller receiver for Prek. This is ordinary same-user shell IPC;
-- it supplies no broker identity, rendered geometry or recognizer ownership.
-- The eventual native adapter must establish those before enabling routing.
local morf=require("morf")
local M={}
local DISCRETE={dashboard=true,notifications=true,quick_settings=true,
  apps=true,web=true,keyboard_show=true,keyboard_hide=true,keyboard_toggle_layout=true}
local CONTINUOUS={dashboard=true,notifications=true,quick_settings=true,workspace_preview=true}
local FIELDS={version=true,output=true,epoch=true,gesture=true,ordinal=true,
  action=true,phase=true,dx=true,dy=true,vx=true,vy=true}
local function hex(value)
  return type(value)=="string" and #value==32 and not value:find("[^0-9a-f]")
end
local function blocked()
  if morf.session_lock_state()~="unlocked" then return true end
  for _,name in ipairs {"polkit","keyring","authsteps","session"} do
    local controller=package.loaded[name]
    if controller and controller.drawer and controller.drawer.open:get() then return true end
  end
  local capture=package.loaded.capture
  if capture and capture.editor and capture.editor.active:get() then return true end
  local switcher=package.loaded["themes.switcher"]
  return switcher and switcher.busy:get() or false
end
local function close_others(keep)
  for _,drawer in ipairs(require("drawer").all) do
    if drawer~=keep and drawer.name~="keyboard" then drawer.set(false) end
  end
end
local function drawer_for(action)
  if action=="dashboard" then return require("dashboard").drawer end
  return require("sidebar").drawer
end
local function select_drawer(action)
  if action=="dashboard" then return end
  local sidebar=require("sidebar")
  sidebar.select(action=="notifications" and "notifications" or "settings")
  if action=="quick_settings" then require("utilities").request("") end
end
local function horizontal(drawer) return drawer.edge=="left" or drawer.edge=="right" end
local function move(drawer,r) drawer.drag_by(horizontal(drawer) and r.dx or r.dy) end
local function discrete(action)
  if action=="keyboard_show" then return require("keyboard").show("full") end
  if action=="keyboard_hide" then require("keyboard").set(false) return true end
  if action=="keyboard_toggle_layout" then
    local keyboard=require("keyboard")
    if not keyboard.active() then return false end
    keyboard.keys.cancel()
    keyboard.keys.mode:set(keyboard.keys.mode:get()=="full" and "dev" or "full")
    keyboard.keys.reset()
    return true
  end
  if action=="apps" or action=="web" then
    local launcher=require("launcher")
    close_others(launcher.drawer)
    require("menus").open(action)
    launcher.set_query("")
    launcher.drawer.set(true)
    return true
  end
  local drawer=drawer_for(action)
  select_drawer(action)
  close_others(drawer)
  drawer.set(true)
  return drawer.is_open()
end

function M.attach(root)
  local seen,count,active={},0,nil
  local settling,native_token,watch
  local function authorized(token)
    local guard=rawget(_G,"__morf_prek_authorized")
    return type(token)=="string" and #token==32 and type(guard)=="function" and guard(token)==true
  end
  local function output() return (morf.screens[1] or {}).name end
  local responding
  local function reply(status,error)
    return {version=1,output=output(),status=status,error=error,
      epoch=responding and responding.epoch,gesture=responding and responding.gesture,
      ordinal=responding and responding.ordinal}
  end
  local function finish(canceled,vx,vy)
    local current=active
    active=nil
    if not current then return end
    current.lease:cancel()
    current.deadline:cancel()
    if current.preview then
      settling=current
      current.preview.finish(canceled,vx or 0,function()
        return not blocked() and (not current.token or authorized(current.token))
      end)
    else current.drawer.end_drag((horizontal(current.drawer) and vx or vy) or 0,canceled) end
  end
  local function cancel_settling()
    if not settling then return false end
    settling.preview.cancel()
    settling=nil
    return true
  end
  local function refresh()
    local changed=false
    if active and active.token and not authorized(active.token) then finish(true) changed=true end
    if settling then
      if not settling.preview.state.active then settling=nil
      elseif blocked() or (settling.token and not authorized(settling.token)) then
        cancel_settling() changed=true
      end
    end
    if not active and not settling and watch then watch:cancel() watch=nil end
    return changed
  end
  local function watch_native()
    if not watch then watch=morf.timer(25,refresh,true) end
  end
  local function lease()
    if active.lease then active.lease:cancel() end
    active.lease=morf.timer(500,function() finish(true) end,false)
  end
  local function receive(payload,token)
    if token and not authorized(token) then return reply("rejected","unauthorized") end
    if token and token~=native_token then
      if active and active.token then finish(true) end
      if settling and settling.token then cancel_settling() end
      native_token=token
    end
    refresh()
    if type(payload)~="string" or #payload>2048 then return reply("rejected","invalid_request") end
    local ok,r=pcall(morf.json.decode,payload)
    if not ok or type(r)~="table" then return reply("rejected","invalid_request") end
    for key in pairs(r) do if not FIELDS[key] then return reply("rejected","invalid_request") end end
    if r.version~=1 or not hex(r.epoch) or not hex(r.gesture)
      or type(r.output)~="string" or #r.output<1 or #r.output>256 or r.output:find("%c")
      or type(r.ordinal)~="number" or r.ordinal%1~=0 or r.ordinal<1 or r.ordinal>2147483647
      or not (DISCRETE[r.action] or CONTINUOUS[r.action])
      or not ({discrete=true,begin=true,update=true,["end"]=true,cancel=true})[r.phase] then
      return reply("rejected","invalid_request")
    end
    for _,field in ipairs {"dx","dy","vx","vy"} do
      local value=r[field]
      if type(value)~="number" or value~=value or math.abs(value)>1000000 then
        return reply("rejected","invalid_request")
      end
    end
    -- A broadcast reaches every screen. Only the frozen named screen may act;
    -- focus changes do not choose a replacement output.
    if r.output~=output() then return nil end
    responding=r
    if blocked() then finish(true) cancel_settling() return reply("rejected","blocked") end
    local key=r.epoch..":"..r.gesture
    if r.phase=="discrete" or r.phase=="begin" then
      if r.ordinal~=1 or (not token and seen[key]) then return reply("rejected","duplicate") end
      if active then return reply("rejected","busy") end
      if not token and count>=4096 then return reply("rejected","capacity") end
      -- Remember even declined/failed controller calls: uncertain effects are
      -- never replayed. No eviction can revive an old identity in this runtime.
      if not token then seen[key]=true count=count+1 end
      if r.phase=="discrete" then
        if not DISCRETE[r.action] then return reply("rejected","unsupported_phase") end
        return discrete(r.action) and reply("accepted") or reply("rejected","unavailable")
      end
      if not CONTINUOUS[r.action] then return reply("rejected","unsupported_phase") end
      local current={key=key,action=r.action,ordinal=1,token=token}
      if r.action=="workspace_preview" then
        local gestures=require("phone_gestures")
        if not gestures.workspace_controller then
          gestures.workspace_controller=require("workspace_gesture").new(root)
          gestures.workspace_preview=gestures.workspace_controller.state
        end
        current.preview=gestures.workspace_controller
        if current.preview.state.active then return reply("rejected","busy") end
        close_others(nil)
        if not current.preview.begin() then return reply("rejected","unavailable") end
        current.preview.update(r.dx)
      else
        current.drawer=drawer_for(r.action)
        if current.drawer.manual then return reply("rejected","busy") end
        select_drawer(r.action)
        close_others(current.drawer)
        if not current.drawer.begin_drag() then return reply("rejected","unavailable") end
        move(current.drawer,r)
      end
      active=current
      if token then watch_native() end
      lease()
      current.deadline=morf.timer(2000,function() finish(true) end,false)
      return reply("accepted")
    end
    if not active or active.key~=key or active.action~=r.action or active.token~=token then return reply("rejected","retired") end
    if r.ordinal~=active.ordinal+1 then return reply("rejected","out_of_order") end
    active.ordinal=r.ordinal
    if r.phase=="cancel" then finish(true)
    else
      if active.preview then active.preview.update(r.dx) else move(active.drawer,r) end
      if r.phase=="end" then finish(false,r.vx,r.vy) else lease() end
    end
    return reply("accepted")
  end
  morf.ipc["prek-control"]=function(payload)
    responding=nil
    local ok,value=pcall(receive,payload)
    if ok then return value end
    -- A callback failure has uncertain effects. Cancel the owned animation;
    -- retain its deduplication identity and return a refusal without retrying.
    pcall(finish,true)
    return reply("rejected","controller_failed")
  end
  morf.ipc["prek-native-control"]=function(payload,token)
    responding=nil
    if not authorized(token) then return reply("rejected","unauthorized") end
    local ok,value=pcall(receive,payload,token)
    if ok then return value end
    pcall(finish,true) pcall(cancel_settling)
    return reply("rejected","controller_failed")
  end
  morf.ipc["prek-native-refresh"]=refresh
  morf.ipc["prek-status"]=function(name)
    if name~=output() then return nil end
    local gestures=require("phone_gestures")
    local keyboard=require("keyboard")
    local sidebar=require("sidebar")
    return {version=1,output=output(),recognizer_ownership=false,
      blocked=blocked(),identities=count,active=active~=nil,
      dashboard=require("dashboard").drawer.open:get(),
      sidebar=sidebar.drawer.open:get(),notifications=sidebar.showing("notifications"),
      quick_settings=sidebar.showing("settings"),
      launcher=require("launcher").drawer.open:get(),menu=require("menus").source:get(),
      keyboard=keyboard.active(),keyboard_layout=keyboard.keys.mode:get(),
      workspace_preview=gestures.workspace_preview and gestures.workspace_preview.active or false}
  end
  -- Authentication can appear without another progress message. Keep this
  -- check dormant while idle rather than adding a permanent polling timer.
  morf.effect("caelestia.prek.controls.blocked",function()
    local inhibited=blocked()
    if inhibited then finish(true) cancel_settling() end
  end)
end
return M
