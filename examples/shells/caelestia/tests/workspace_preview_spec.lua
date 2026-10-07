local test=morf.test
local HOST=[[
  local ui=require("morf.ui")
  local make_image=ui.Image local pictures={}
  ui.Image=function(props)
    local node=make_image(props)
    if props.id and props.id:find("phone-workspace-wallpaper-",1,true)==1 then pictures[props.id]=node end
    return node
  end
  -- Match the real fullscreen shell: zero means automatic sizing.
  morf.surface.width,morf.surface.height=0,0
  package.loaded.bar={desk=function() return 0,60,1116,2424 end}
  local clients={
    {workspace=1.0,class="terminal",title="Current",width=1000,height=2200,x=50,y=100,mapped=true},
    {workspace=2.0,class="editor",title="Next",width=900,height=2100,x=80,y=120,mapped=true}}
  local data=morf.state {clients=clients,workspaces={},monitors={{name="DSI-1",x=0,y=0,width=1116,scale=1}}}
  package.loaded["lib.integrations.hyprland"]={state=data,options=function(_,callback)
    callback({["general:border_size"]={int=3},["decoration:rounding"]={int=20},
      ["general:col.active_border"]={gradient="fff0c5d6 0deg"},
      ["general:col.inactive_border"]={gradient="ff523843 0deg"}})
  end}
  local switches,releases,pending={},{},{}
  package.loaded.services={output=function() return "DSI-1" end,workspace={
    active=function() return 1.0 end,go=function(id) switches[#switches+1]=tostring(id) end}}
  morf.windows={{app_id="terminal",title="Current",identifier="current"},
    {app_id="editor",title="Next",identifier="next"}}
  morf.screencopy={capture_window=function(id,callback) pending[#pending+1]={id=id,callback=callback} end,
    release=function(source) releases[#releases+1]=source end}
  local root=ui.Item {width=1116,height=2484}
  local preview=require("workspace_gesture").new(root)
  morf.ipc.begin=preview.begin
  morf.ipc.update=function(dx) preview.update(tonumber(dx)) end
  morf.ipc.finish=function() preview.finish(false) end
  morf.ipc.cancel=preview.cancel
  morf.ipc.wallpapers=function()
    return {pictures["phone-workspace-wallpaper-0"].source,pictures["phone-workspace-wallpaper-1"].source}
  end
  morf.ipc.studio=function() require("lule_studio") end
  morf.ipc.churn=function() for i=1,5000 do local scratch=string.rep(tostring(i),100) end end
  morf.ipc.frame=function()
    local item=table.remove(pending,1)
    if item then item.callback({source="memory:"..item.id}) end
  end
  morf.ipc.state=function() return {releases=releases,switches=switches,pending=#pending} end
]]
test.it("workspace previews follow newly applied Lule images while the studio is loaded",function()
  local root=morf.env("XDG_CACHE_HOME").."/workspace-wallpaper"
  local first,second=root.."/first.svg",root.."/second.svg"
  for path,color in pairs {[first]="#ffaa66",[second]="#6688ff"} do
    morf.fs.write(path,'<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><path fill="'..color..'" d="M0 0h16v16H0z"/></svg>')
  end
  local function apply(image)
    morf.fs.write(root.."/colors.json",morf.json.encode {wallpaper=image,theme="dark",colors={"#111111","#ffaa66"}})
  end
  apply(first)
  test.load("../shell/init.lua",{source=HOST,size={1116,2484},
    env={CAELESTIA_DRY_RUN="1",CAELESTIA_WALLPAPER="",LULE_A=root}})
  test.ipc("begin") test.eq(test.ipc("wallpapers"),{first,first})
  test.ipc("studio")
  for i=1,4 do test.ipc("churn") test.advance(20) end
  apply(second)
  test.wait(function() return test.ipc("wallpapers")[1]==second end,2000)
  test.eq(test.ipc("wallpapers"),{second,second})
  test.ipc("cancel") test.ipc("begin")
  test.eq(test.ipc("wallpapers"),{second,second})
  test.eq(test.logs("error"),{})
end)
test.it("automatic fullscreen dimensions retain window geometry and release captures on cancel",function()
  test.load("../shell/init.lua",{source=HOST,size={1116,2484},env={CAELESTIA_DRY_RUN="1"}})
  test.ipc("begin")
  local card=test.get("phone-workspace-window-0-1")
  local wallpaper=test.get("phone-workspace-wallpaper-0")
  test.near(wallpaper.y,0,1)
  test.near(wallpaper.width,1116,1) test.near(wallpaper.height,2484,1)
  test.near(card.width,1000,1) test.near(card.height,2200,1)
  local border=test.get("phone-workspace-border-0-1")
  test.near(border.width,1006,1) test.near(border.height,2206,1)
  test.near(border.x,card.x-3,1) test.near(border.y,card.y-3,1)
  test.ipc("frame")
  test.ipc("update","-500")
  test.near(test.get("phone-workspace-window-0-1").x,card.x-500,1)
  test.ipc("cancel")
  test.ipc("frame") -- A frame arriving after cancellation must be released too.
  local state=test.ipc("state")
  test.eq(state.releases,{"memory:current","memory:next"})
  test.eq(state.switches,{})
  test.eq(#test.find_all("phone-workspace-window-0-1"),0)
end)
test.it("floating-point workspace IDs become integer compositor commands on release",function()
  test.load("../shell/init.lua",{source=HOST,size={1116,2484},env={CAELESTIA_DRY_RUN="1"}})
  test.ipc("begin") test.ipc("update","-600")
  test.eq(test.ipc("state").switches,{})
  test.ipc("finish") test.advance(400)
  test.eq(test.ipc("state").switches,{"2"})
end)
