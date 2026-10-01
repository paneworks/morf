-- Reusable screenshot acquisition, compositor geometry and output actions.
-- The shell owns appearance, hotkeys and preferences. Commands use argv;
-- images stay in a session-private directory until explicitly saved/uploaded.
local morf=require("morf")
local M={}
local next_id=0
local live_folders={}
local run
function M.expand(path)
  local home=morf.fs.home()
  return tostring(path or ""):gsub("^~/",function() return home.."/" end):gsub("%$HOME",function() return home end)
end
function M.kind()
  if morf.env("HYPRLAND_INSTANCE_SIGNATURE") then return "hyprland" end
  if morf.env("SWAYSOCK") then return "sway" end
  if morf.env("NIRI_SOCKET") then return "niri" end
  local desktop=(morf.env("XDG_CURRENT_DESKTOP") or ""):lower()
  return desktop:find("kde",1,true) and "kde" or "generic"
end
function M.windows_from(kind,data,workspaces,outputs)
  local found={}
  if kind=="hyprland" then
    local visible={}
    for _,m in ipairs(outputs or {}) do
      if m.activeWorkspace then visible[m.activeWorkspace.id]=true end
      if m.specialWorkspace and m.specialWorkspace.id~=0 then visible[m.specialWorkspace.id]=true end
    end
    for _,c in ipairs(data or {}) do
      if c.mapped and not c.hidden and c.at and c.size and c.workspace and visible[c.workspace.id] and c.size[1]>0 and c.size[2]>0 then
        found[#found+1]={x=c.at[1],y=c.at[2],w=c.size[1],h=c.size[2],z=c.focusHistoryID or 99,title=c.title or "Window"}
      end
    end
  elseif kind=="sway" then
    local function walk(node,visible)
      if node.type=="workspace" then visible=node.visible~=false end
      for _,child in ipairs(node.nodes or {}) do walk(child,visible) end
      for _,child in ipairs(node.floating_nodes or {}) do walk(child,visible) end
      local r=node.rect
      if visible and node.visible~=false and r and r.width>0 and r.height>0 and
        #(node.nodes or {})==0 and #(node.floating_nodes or {})==0 and (node.app_id or node.window or node.pid) then
        found[#found+1]={x=r.x,y=r.y,w=r.width,h=r.height,z=-#found,title=node.name or "Window"}
      end
    end
    walk(data or {},true)
  elseif kind=="niri" then
    local by_workspace={}
    for _,ws in ipairs(workspaces or {}) do if ws.is_active then by_workspace[ws.id]=ws.output end end
    for _,c in ipairs(data or {}) do
      local layout=c.layout local output=outputs and outputs[by_workspace[c.workspace_id]]
      local rect=output and output.logical local pos=layout and layout.tile_pos_in_workspace_view
      if rect and pos and layout.window_size then
        local offset=layout.window_offset_in_tile or {0,0} local size=layout.window_size
        found[#found+1]={x=rect.x+pos[1]+offset[1],y=rect.y+pos[2]+offset[2],w=size[1],h=size[2],
          z=c.is_focused and 0 or #found+1,title=c.title or "Window"}
      end
    end
  end
  table.sort(found,function(a,b) return a.z<b.z end)
  return found
end
local function query(argv,cb)
  run(argv,{},function(result)
    if not result or not result.ok then cb(nil) return end
    local fine,data=pcall(morf.json.decode,result.stdout or "") cb(fine and data or nil)
  end)
end
function M.windows(cb)
  local kind=M.kind()
  if kind=="hyprland" then
    local hypr=require("lib.hyprland")
    hypr.json("monitors",function(monitors)
      hypr.json("clients",function(clients) cb(M.windows_from(kind,clients,nil,monitors)) end)
    end)
  elseif kind=="sway" then query({"swaymsg","-t","get_tree"},function(tree) cb(M.windows_from(kind,tree)) end)
  elseif kind=="niri" then
    query({"niri","msg","--json","windows"},function(windows)
      query({"niri","msg","--json","workspaces"},function(workspaces)
        query({"niri","msg","--json","outputs"},function(outputs) cb(M.windows_from(kind,windows,workspaces,outputs)) end)
      end)
    end)
  else cb({}) end
end
function M.window_at(windows,x,y)
  for _,r in ipairs(windows or {}) do if x>=r.x and y>=r.y and x<r.x+r.w and y<r.y+r.h then return r end end
end
function M.desktop_layout(screens,factor)
  local left,top,right,bottom
  for _,screen in ipairs(screens) do
    local x,y=screen.x or 0,screen.y or 0
    local w,h=screen.width or 0,screen.height or 0
    if w>0 and h>0 then
      left=left and math.min(left,x) or x top=top and math.min(top,y) or y
      right=right and math.max(right,x+w) or x+w bottom=bottom and math.max(bottom,y+h) or y+h
    end
  end
  if not left then return nil,"No monitors available" end
  factor=factor or 1
  local width,height=math.ceil((right-left)*factor),math.ceil((bottom-top)*factor)
  if width>16384 or height>16384 or width*height>67108864 then return nil,"The combined desktop is too large to edit; capture one monitor" end
  local result={x=left,y=top,width=right-left,height=bottom-top,pixels_w=width,pixels_h=height,scale=factor,outputs={}}
  for _,screen in ipairs(screens) do
    if (screen.width or 0)>0 and (screen.height or 0)>0 then
    result.outputs[#result.outputs+1]={name=screen.name,x=math.floor(((screen.x or 0)-left)*factor),
      y=math.floor(((screen.y or 0)-top)*factor),w=math.ceil(screen.width*factor),h=math.ceil(screen.height*factor)}
    end
  end
  return result
end
function M.binding_plan(binding,previous,flavour)
  local function parts(value)
    local out={} for token in tostring(value):gmatch("[^+]+") do
      local part=token:match("^%s*(.-)%s*$")
      if not part:match("^[%w_:]+$") then return nil end
      out[#out+1]=part
    end
    if #out==0 or #out>5 then return nil end
    for i=1,#out-1 do if out[i]~="SUPER" and out[i]~="CTRL" and out[i]~="ALT" and out[i]~="SHIFT" then return nil end end
    return out
  end
  local keys,old=parts(binding),parts(previous or "Print")
  if not keys or not old then return nil,"Invalid key binding" end
  local command="/usr/bin/morf ipc call capture open"
  if flavour=="lua" then
    local next=table.concat(keys," + ") local prev=table.concat(old," + ")
    local bind=('hl.bind("%s", hl.dsp.exec_cmd("%s"))'):format(next,command)
    local unbind=('hl.unbind("%s")\n'):format(prev)
    if next~=prev then unbind=unbind..('hl.unbind("%s")\n'):format(next) end
    return {text=unbind..bind.."\n",binding=next,bind=bind}
  end
  local function conf(tokens) local copy={} for i=1,#tokens-1 do copy[#copy+1]=tokens[i] end return table.concat(copy," ")..","..tokens[#tokens] end
  return {text=("unbind = %s\nunbind = %s\nbind = %s,exec,%s\n"):format(conf(old),conf(keys),conf(keys),command),binding=table.concat(keys," + "),
    unbind=conf(old),new_unbind=conf(keys),bind=conf(keys)..",exec,"..command}
end
function M.rebind(binding,previous,include_file,cb)
  local lib=require("lib.hyprland_config")
  if not lib.available() then cb(false,"Key rebinding requires Hyprland; use your compositor's settings") return end
  lib.flavour(function(flavour)
    local plan,why=M.binding_plan(binding,previous,flavour)
    if not plan then cb(false,why) return end
    if include_file and include_file~="" then
      local path=M.expand(include_file) local parent=path:match("^(.*)/[^/]+$") if parent then morf.fs.mkdir(parent) end
      local ok,error=morf.fs.write(path,plan.text) if not ok then cb(false,error or "Cannot write binding file") return end
    end
    local hypr=require("lib.hyprland")
    if flavour=="lua" then hypr.eval(plan.text,function(reply,error) cb(reply and reply:match("^%s*ok%s*$")~=nil,error or reply) end)
    else hypr.keyword("unbind",plan.unbind,function()
      hypr.keyword("unbind",plan.new_unbind,function()
        hypr.keyword("bind",plan.bind,function(ok,error) cb(ok,error) end)
      end)
    end) end
  end)
end
function M.session(options)
  options=options or {} next_id=next_id+1
  local runtime=morf.env("XDG_RUNTIME_DIR") or morf.state_path("capture-tmp")
  local root=runtime.."/morf"
  morf.fs.mkdir(root)
  local screen=(morf.screens or {})[1] or {}
  local identity=(screen.name or "headless"):gsub(".",function(c) return string.format("%02x",string.byte(c)) end)
  local prefix="capture-"..tostring(morf.process_id).."-"..identity.."-"
  -- Clean a previous runtime's abandoned scratch data, never another live output.
  for _,entry in ipairs(morf.fs.list(root) or {}) do
    local pid=entry.name:match("^capture%-(%d+)%-")
    if entry.is_dir and pid and not live_folders[entry.path] and (not morf.fs.is_dir("/proc/"..pid) or entry.name:sub(1,#prefix)==prefix) then
      pcall(morf.fs.remove,entry.path,{recursive=true})
    end
  end
  local folder=root.."/"..prefix..tostring(morf.time.now_ms()).."-"..next_id
  morf.fs.mkdir(folder)
  live_folders[folder]=true
  local s={folder=folder,files={},closed=false,sequence=0}
  function s.path(name) local path=folder.."/"..name s.files[path]=true return path end
  function s.remove(path)
    if path:sub(1,7)=="memory:" then return end
    pcall(morf.fs.remove,path) s.files[path]=nil
    -- A worker can finish writing after close removed its directory.
    -- Remove the newly empty scratch directory again after its last result.
    if s.closed then pcall(morf.fs.remove,folder) end
  end
  function s.close()
    s.closed=true live_folders[folder]=nil
    if s.native_preview then s.native_preview.close() s.native_preview=nil end
    for path in pairs(s.files) do pcall(morf.fs.remove,path) end
    s.files={} pcall(morf.fs.remove,folder)
  end
  function s.snapshot(output,cb)
    s.sequence=s.sequence+1
    local path=s.path("original-"..s.sequence..".png")
    local function done(ok,value)
      if s.closed then s.remove(path) return end
      cb(ok,ok and {source=path,width=value.width,height=value.height,output=output} or value)
    end
    if M.kind()=="kde" or options.backend=="spectacle" then
      run({"spectacle","--background","--nonotify","--fullscreen","--output",path},{},function(result)
        if not result or not result.ok then done(false,result and result.stderr or "Spectacle failed") return end
        local info,why=morf.image.info(path)
        if not info then done(false,why) return end
        if output and #(morf.screens or {})>1 then
          local layout=M.desktop_layout(morf.screens)
          local screen
          for _,candidate in ipairs(morf.screens) do if candidate.name==output then screen=candidate break end end
          if layout and screen then
            local sx,sy=info.width/layout.width,info.height/layout.height
            local x,y=math.floor((screen.x-layout.x)*sx),math.floor((screen.y-layout.y)*sy)
            s.render(path,{{"crop",x,y,math.ceil(screen.width*sx),math.ceil(screen.height*sy)}},function(ok,cropped)
              s.remove(path)
              if ok then local size=morf.image.info(cropped) cb(true,{source=cropped,width=size.width,height=size.height,output=output}) else cb(false,cropped) end
            end)
            return
          end
        end
        done(true,info)
      end)
    else
      local ok,why=pcall(morf.screencopy.save,{path=path,output=output,include_cursor=options.cursor==true,on_done=done})
      if not ok then done(false,tostring(why)) end
    end
  end
  function s.desktop(screens,cb)
    local usable={}
    for _,screen in ipairs(screens) do
      if (screen.width or 0)>0 and (screen.height or 0)>0 then usable[#usable+1]=screen end
    end
    screens=usable
    if #screens==0 then cb(false,"No monitors available") return end
    local frames,paths={},{}
    local index=0 local factor=1
    local function compose()
      local layout,why=M.desktop_layout(screens,factor)
      if not layout then cb(false,why) return end
      local ops={} local i=0
      local function resize_next()
        if s.closed then return end
        i=i+1
        if i>#frames then
          s.compose(layout.pixels_w,layout.pixels_h,ops,function(ok,path)
            for _,old in ipairs(paths) do s.remove(old) end
            cb(ok,ok and {source=path,width=layout.pixels_w,height=layout.pixels_h,desktop=layout} or path)
          end)
          return
        end
        local output=layout.outputs[i] local frame=frames[i]
        if frame.width==output.w and frame.height==output.h then
          ops[#ops+1]={"overlay",frame.source,output.x,output.y} resize_next()
        else s.render(frame.source,{{"resize",output.w,output.h,"exact"}},function(ok,path)
          if not ok then cb(false,path) return end
          paths[#paths+1]=path ops[#ops+1]={"overlay",path,output.x,output.y} resize_next()
        end) end
      end
      resize_next()
    end
    local function capture_next()
      if s.closed then return end
      index=index+1
      if index>#screens then compose() return end
      local screen=screens[index]
      s.snapshot(screen.name,function(ok,frame)
        if not ok then cb(false,frame) return end
        frames[#frames+1]=frame paths[#paths+1]=frame.source
        factor=math.max(factor,frame.width/screen.width,frame.height/screen.height)
        capture_next()
      end)
    end
    if M.kind()=="kde" then
      s.snapshot(nil,function(ok,frame)
        if ok then frame.desktop=M.desktop_layout(screens,1) end cb(ok,frame)
      end)
    else capture_next() end
  end
  -- Preview frames never touch disk on engines supporting native sessions.
  -- One source per capture, one worker at a time (the controller coalesces).
  function s.preview(source,ops,cb)
    if not morf.image.preview then return s.render(source,ops,cb) end
    if s.closed then return end
    if s.preview_source~=source then
      if s.native_preview then s.native_preview.close() end
      s.native_preview=morf.image.preview(source) s.preview_source=source
    end
    local ok,queued,why=pcall(s.native_preview.render,ops,function(good,result)
      if s.closed then return end
      cb(good,good and result.path or result)
    end)
    if not ok or not queued then cb(false,ok and why or queued) end
  end
  local function render(fn,options,cb)
    s.sequence=s.sequence+1 local path=s.path("render-"..s.sequence..".png")
    options.output=path options.on_done=function(good,result)
      if s.closed then s.remove(path) return end
      if not good then s.remove(path) end
      cb(good,good and path or result)
    end
    local ok,queued,why=pcall(fn,options)
    if not ok or not queued then s.remove(path) cb(false,ok and why or queued) end
  end
  function s.render(source,ops,cb)
    render(morf.image.process,{source=source,ops=ops},cb)
  end
  function s.compose(width,height,ops,cb)
    render(morf.image.compose,{width=width,height=height,ops=ops},cb)
  end
  return s
end
-- Exactly one completion even when a command cannot be spawned.
run=function(argv,options,cb)
  local completed=false
  local function done(result)
    if completed then return end completed=true cb(result)
  end
  local ok,child=pcall(morf.run,argv,options,done)
  if not ok or not child then done({ok=false,stderr=ok and "Could not start "..argv[1] or tostring(child)}) end
  return ok and child or nil
end
function M.copy(path,cb)
  -- wl-copy owns the clipboard beyond a shell reload; pass the path as $0.
  return run({"sh","-c",'exec wl-copy --type image/png < "$0"',path},{},function(result)
    if cb then cb(result and result.ok==true,result and result.stderr or "Clipboard unavailable") end
  end)
end
function M.save(path,target,cb)
  target=M.expand(target)
  local completed=false
  local function done(ok,value)
    if completed then return end completed=true
    if cb then cb(ok,ok and target or tostring(value or "Could not save image")) end
  end
  local ok,queued,why=pcall(morf.image.process,{source=path,output=target,ops={},on_done=done})
  if not ok or not queued then done(false,ok and why or queued) end
  return ok and queued or nil
end
-- Shared filesystem model for a shell-owned picker. No external GUI required.
function M.directory(path)
  path=M.expand(path):gsub("/+$","")
  if path=="" then path="/" end
  if path:sub(1,1)~="/" then return nil,"Use an absolute folder path or ~/" end
  local entries,why=morf.fs.list(path,{follow=true})
  if not entries then return nil,why or "Cannot read folder" end
  local folders={}
  if path~="/" then folders[1]={name="..",path=path:match("^(.*)/[^/]+$") or "/",is_dir=true} end
  for _,entry in ipairs(entries) do
    if entry.is_dir then folders[#folders+1]={name=entry.name,path=entry.path,is_dir=true} end
  end
  table.sort(folders,function(a,b) if a.name==".." then return true elseif b.name==".." then return false end return a.name:lower()<b.name:lower() end)
  return {path=path,entries=folders}
end
function M.upload(path,endpoint,cb)
  endpoint=endpoint or "https://litterbox.catbox.moe/resources/internals/api.php"
  if not endpoint:match("^https://") then cb(false,"Upload endpoint must use HTTPS") return end
  local form='fileToUpload=@"'..path:gsub('\\','\\\\'):gsub('"','\\"')..'"'
  return run({"curl","--fail","--silent","--show-error","--max-time","60",
    "--form-string","reqtype=fileupload","--form-string","time=72h","--form",form,endpoint},{max_output=4096},function(result)
    local url=result and result.ok and (result.stdout or ""):gsub("%s+$","") or ""
    local ok=url:match("^https://[^%s]+$")~=nil
    if ok then morf.clipboard.set(url) end
    cb(ok,ok and url or result and result.stderr or "Upload failed")
  end)
end
return M
