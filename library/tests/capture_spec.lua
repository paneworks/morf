local test=morf.test
local function check(name,body)
  test.it(name,function()
    test.load {source='local C=require("lib.capture")\n'..body..'\nmorf.ipc.ok=function() return true end'}
    test.truthy(test.ipc("ok")) test.eq(#test.logs("error"),0)
  end)
end
check("desktop coordinates support negative origins, scale and disabled outputs",[[
  local d=C.desktop_layout({{name="left",x=-100,y=0,width=100,height=80},{name="right",x=0,y=-20,width=120,height=100},{name="off",width=0}},1.5)
  assert(d.x==-100 and d.y==-20 and d.pixels_w==330 and d.pixels_h==150)
  assert(#d.outputs==2 and d.outputs[2].x==150 and d.outputs[1].y==30)
  assert(not C.desktop_layout({}))
  assert(not C.desktop_layout({{width=16000,height=16000}},2))
]])
check("window picking filters hidden workspaces and favours the front window",[[
  local windows=C.windows_from("hyprland",{
    {mapped=true,at={10,20},size={100,80},workspace={id=1},focusHistoryID=1},
    {mapped=true,at={15,25},size={100,80},workspace={id=1},focusHistoryID=0,title="front"},
    {mapped=true,at={0,0},size={200,200},workspace={id=9}},
    {mapped=true,hidden=true,at={0,0},size={200,200},workspace={id=1}},
  },nil,{{activeWorkspace={id=1}}})
  assert(#windows==2 and C.window_at(windows,20,30).title=="front")
  local n=C.windows_from("niri",{{workspace_id=1,is_focused=true,layout={tile_pos_in_workspace_view={4,5},window_offset_in_tile={2,3},window_size={40,30}}}},
    {{id=1,is_active=true,output="DP"}},{DP={logical={x=-100,y=20}}})
  assert(#n==1 and n[1].x==-94 and n[1].y==28)
  local s=C.windows_from("sway",{nodes={{type="workspace",visible=false,nodes={{app_id="hidden",rect={x=0,y=0,width=50,height=50}}}},
    {type="workspace",visible=true,nodes={{app_id="visible",rect={x=10,y=20,width=50,height=50}}}}}})
  assert(#s==1 and s[1].x==10)
]])
check("rebinding generates valid Lua and conf without interpolating shell text",[[
  local lua=C.binding_plan("SUPER + SHIFT + s","Print","lua")
  assert(lua.text:find('hl.unbind("Print")',1,true) and lua.text:find('hl.bind("SUPER + SHIFT + s"',1,true))
  assert(lua.text:find('hl.unbind("SUPER + SHIFT + s")',1,true))
  local conf=C.binding_plan("CTRL + Print","SHIFT + Print","hyprlang")
  assert(conf.unbind=="SHIFT,Print" and conf.bind:find("CTRL,Print,exec,",1,true))
  assert(not C.binding_plan('Print\"); bad()',"Print","lua"))
]])
check("file picker expands folders and export failures finish once",[[
  local directory=C.directory(morf.fs.home())
  assert(directory and directory.path==morf.fs.home())
  assert(not C.directory("relative"))
  local calls=0
  C.save("missing","/tmp/capture-unsupported.extension",function(ok) assert(not ok) calls=calls+1 end)
  assert(calls==1)
  morf.run=function() return nil end
  C.copy("missing",function(ok) assert(not ok) calls=calls+1 end)
  C.upload("missing","http://invalid",function(ok) assert(not ok) calls=calls+1 end)
  assert(calls==3)
]])
test.it("native annotation export renders vectors, text and effects then crops and cleans up",function()
  test.load {source=[[
    local C=require("lib.capture") local A=require("lib.annotation")
    local session=C.session() local d=A.new(160,100)
    local base='<svg xmlns="http://www.w3.org/2000/svg" width="160" height="100"><rect width="160" height="100" fill="#202020"/></svg>'
    d.choose("rect") d.style("color","#00ff00") d.style("filled",true) d.begin(20,20) d.update(80,60) d.finish()
    d.choose("text") d.style("color","#ffffff") d.style("width",20) d.begin(20,65) d.finish("Test")
    d.choose("blur") d.begin(90,20) d.update(120,60) d.finish()
    d.choose("pixelate") d.begin(120,20) d.update(150,60) d.finish()
    d.choose("zoom") d.begin(90,65) d.update(150,95) d.finish()
    d.set_crop(10,10,140,85)
    local done=false local error local result
    session.render(base,A.ops(d,true),function(ok,path) done=true if ok then result=path else error=path end end)
    morf.ipc.state=function()
      if not done then return false end
      if error then return {error=tostring(error)} end
      local info=morf.image.info(result) local green=morf.image.pixel(result,20,20):hex()
      local text=false
      for x=10,60,2 do for y=55,78,2 do local color=morf.image.pixel(result,x,y):hex() if color=="#ffffff" then text=true end end end
      return {w=info.width,h=info.height,green=green,text=text,path=result}
    end
    morf.ipc.close=function() local folder=session.folder session.close() return not morf.fs.exists(folder) end
  ]]}
  local state=test.wait(function() return test.ipc("state") end,5000,"export did not finish")
  test.eq(state.error,nil) test.eq(state.w,140) test.eq(state.h,85) test.eq(state.green,"#00ff00") test.truthy(state.text)
  test.truthy(test.ipc("close")) test.eq(#test.logs("error"),0)
end)
test.it("desktop acquisition stitches differently scaled monitors without changing coordinates",function()
  test.load {source=[[
    local C=require("lib.capture") C.kind=function() return "generic" end
    local session=C.session() local frame=false local resizes=0
    local render=session.render
    session.render=function(source,ops,cb) if ops[1] and ops[1][1]=="resize" then resizes=resizes+1 end render(source,ops,cb) end
    morf.screencopy.save=function(options)
      local size=options.output=="left" and 16 or 8
      local colour=options.output=="left" and "red" or "blue"
      morf.image.process {source=('<svg xmlns="http://www.w3.org/2000/svg" width="%d" height="%d"><rect width="100%%" height="100%%" fill="%s"/></svg>'):format(size,size,colour),
        output=options.path,ops={},on_done=options.on_done}
    end
    session.desktop({{name="left",x=-8,y=0,width=8,height=8},{name="right",x=0,y=0,width=8,height=8}},function(ok,result)
      frame=ok and result or {error=tostring(result)}
    end)
    morf.ipc.state=function()
      if not frame then return false end
      if frame.error then return frame end
      return {w=frame.width,h=frame.height,left=morf.image.pixel(frame.source,2,2):hex(),
        right=morf.image.pixel(frame.source,20,2):hex(),x=frame.desktop.x,scale=frame.desktop.scale,resizes=resizes}
    end
    morf.ipc.close=function() session.close() return not morf.fs.exists(session.folder) end
  ]]}
  local s=test.wait(function() return test.ipc("state") end,5000)
  test.eq(s.error,nil) test.eq(s.w,32) test.eq(s.h,16) test.eq(s.x,-8) test.eq(s.scale,2) test.eq(s.resizes,1)
  test.eq(s.left,"#ff0000") test.eq(s.right,"#0000ff") test.truthy(test.ipc("close"))
end)
test.it("KDE full-desktop acquisition crops the requested monitor",function()
  test.load {source=[[
    local C=require("lib.capture") C.kind=function() return "kde" end
    morf.screens={{name="left",x=-8,y=0,width=8,height=8},{name="right",x=0,y=0,width=8,height=8}}
    morf.run=function(argv,_,done)
      morf.image.process {source='<svg xmlns="http://www.w3.org/2000/svg" width="32" height="16"><rect width="32" height="16" fill="red"/><rect x="16" width="16" height="16" fill="blue"/></svg>',
        output=argv[#argv],ops={},on_done=function(ok) done({ok=ok}) end}
      return {kill=function() end}
    end
    local session=C.session() local frame=false
    session.snapshot("right",function(ok,result) frame=ok and result or {error=tostring(result)} end)
    morf.ipc.state=function()
      if not frame then return false end
      if frame.error then return frame end
      return {w=frame.width,h=frame.height,colour=morf.image.pixel(frame.source,1,1):hex()}
    end
    morf.ipc.close=function() session.close() return true end
  ]]}
  local s=test.wait(function() return test.ipc("state") end,5000)
  test.eq(s.error,nil) test.eq(s.w,16) test.eq(s.h,16) test.eq(s.colour,"#0000ff") test.ipc("close")
end)
check("upload uses HTTPS multipart and copies only a valid returned URL",[[
  local copied=nil local calls=0
  morf.clipboard.set=function(value) copied=value end
  morf.run=function(argv,options,cb)
    assert(argv[1]=="curl" and argv[#argv]=="https://upload.example/api")
    assert(table.concat(argv," "):find("time=72h",1,true))
    assert(table.concat(argv," "):find('fileToUpload=@"/tmp/a b.png"',1,true))
    calls=calls+1 cb({ok=true,stdout=calls==1 and "https://images.example/shot.png\n" or "not a URL"})
    return {kill=function() end}
  end
  local completed=0
  C.upload("/tmp/a b.png","https://upload.example/api",function(ok,url)
    assert(ok and url=="https://images.example/shot.png") completed=completed+1
  end)
  assert(completed==1 and copied=="https://images.example/shot.png") copied=nil
  C.upload("/tmp/a b.png","https://upload.example/api",function(ok) assert(not ok) completed=completed+1 end)
  assert(completed==2 and copied==nil)
]])

test.it("cancelled image workers remove late files and scratch directories",function()
  test.load {source=[[
    local C=require("lib.capture") local process=morf.image.process local done,calls=false,0
    morf.image.process=function(request)
      local callback=request.on_done
      request.on_done=function(...) callback(...) done=true end
      return process(request)
    end
    local session=C.session() local folder=session.folder
    session.render('<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><rect width="64" height="64" fill="red"/></svg>',{},function() calls=calls+1 end)
    session.close()
    morf.ipc.state=function() return done and {calls=calls,exists=morf.fs.exists(folder)} or false end
  ]]}
  local state=test.wait(function() return test.ipc("state") end,5000)
  test.eq(state.calls,0) test.falsy(state.exists) test.eq(#test.logs("error"),0)
end)
