local test=morf.test
local SOURCE=[[
  local ui=require("morf.ui")
  local fixture='<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="700"><rect width="1200" height="700" fill="#37474f"/><rect x="100" y="100" width="600" height="400" fill="#607d8b"/></svg>'
  local exports,copies,uploads,closed,saves,binding=0,0,0,0,0,""
  local render_ms,inflight,maxflight,last_ops,hover_updates=20,0,0,{},0
  local backend=require("lib.util.capture")
  backend.windows=function(cb) cb({{x=100,y=100,w=600,h=400,z=0}}) end
  backend.session=function()
    return {snapshot=function(_,cb) cb(true,{source=fixture,width=1200,height=700}) end,
      render=function(_,ops,cb)
        exports=exports+1 inflight=inflight+1 maxflight=math.max(maxflight,inflight) last_ops=ops
        morf.timer(render_ms,function() inflight=inflight-1 cb(true,fixture) end,false)
      end,
      remove=function() end,close=function() closed=closed+1 end}
  end
  backend.copy=function(_,cb) copies=copies+1 cb(true) end
  backend.save=function(_,_,cb) saves=saves+1 cb(true) end
  backend.rebind=function(next,_,_,cb) binding=next cb(true) end
  backend.upload=function(_,_,cb) uploads=uploads+1 cb(true) end
  local editor=require("capture_editor")
  morf.effect("fixture.hover",function() editor.hovered:get() hover_updates=hover_updates+1 end)
  morf.ipc.render_delay=function(ms) render_ms=tonumber(ms) end
  morf.surface.height=morf.screens[1].height
  ui.Item {width=morf.screens[1].width,height=morf.screens[1].height,editor.node}
  morf.ipc.start=function(target) editor.start(target or "region",function() end) end
  morf.ipc.begin=function(x,y) editor.begin(tonumber(x),tonumber(y)) end
  morf.ipc.move=function(x,y) editor.move(tonumber(x),tonumber(y)) end
  morf.ipc.finish=function(x,y) editor.finish(tonumber(x),tonumber(y)) end
  morf.ipc.wheel=function(steps) editor.wheel(tonumber(steps)) end
  morf.ipc.key=editor.key
  morf.ipc.option=function(key,value) require("config").set("capture."..key,value=="true" and true or value=="false" and false or value) end
  morf.ipc.picker=function(kind,path) editor.pick_open(kind,path,function() end) end
  morf.ipc.state=function() local d=editor.document return {active=editor.active:get(),busy=editor.busy:get(),phase=editor.phase:get(),
    tool=d and d.tool or "",items=d and #d.items or 0,exports=exports,copies=copies,uploads=uploads,closed=closed,saves=saves,binding=binding,picker=editor.picker:get(),
    crop=d and d.crop or false,maxflight=maxflight,last_ops=last_ops,hover_updates=hover_updates,
    status=editor.status:get(),hint=editor.hint:get(),style=editor.current_style(),
    marks=d and d.items or {},draft=d and d.draft or false} end
]]
local function load(style,w,h,select)
  test.load("../shell/init.lua",{source=SOURCE,size={w or 1200,h or 800},env={CAELESTIA_STYLE=style,CAELESTIA_DRY_RUN="1"}})
  test.ipc("start") test.advance(300)
  if select~=false then test.ipc("key",115,"s","") test.advance(250) end
end
local function tool(id) test.click("capture-editor-draw") test.advance(220) test.click("capture-tool-"..id) end
local function more(id) test.click("capture-editor-more") test.advance(220) test.click("capture-editor-"..id) end
for _,style in ipairs {"material","tsugumori"} do

  test.it(style.." selects at screen size before showing any editing controls",function()
    load(style,1200,800,false)
    test.eq(test.ipc("state").phase,"selecting")
    test.falsy(test.get("capture-editor-toolbar").visible)
    test.falsy(test.get("capture-tools-palette").visible)
    test.eq(test.get("capture-editor-canvas").width,1200)
    if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(style.."-capture-select-first.png") end
    test.press(200,150) test.move(850,500)
    test.falsy(test.get("capture-editor-toolbar").visible)
    test.release(850,500) test.advance(300)
    test.eq(test.ipc("state").phase,"editing") test.eq(test.ipc("state").crop.w,650)
    test.truthy(test.get("capture-editor-toolbar").visible)
    test.falsy(test.get("capture-tools-palette").visible)
    test.truthy(test.get("capture-editor-toolbar").width<400)
    test.eq(test.get("capture-editor-canvas").width,1200)
    if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(style.."-capture-floating-menu.png") end
    more("reselect") test.advance(250)
    test.eq(test.ipc("state").phase,"selecting") test.falsy(test.get("capture-editor-toolbar").visible)
    test.click(120,120) test.advance(300)
    test.eq(test.ipc("state").crop.w,600) test.eq(test.ipc("state").crop.h,400)
    test.eq(test.ipc("state").phase,"editing")
    test.click("capture-editor-draw") test.advance(250)
    if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(style.."-capture-tools-popup.png") end
    test.key("Escape") test.advance(100)
    test.click(20,20) test.falsy(test.ipc("state").active)
    test.eq(#test.logs("error"),0)
  end)
  test.it(style.." ignores export while choosing and accepts the screen shortcut",function()
    load(style,1200,800,false)
    test.ipc("key",99,"c","ctrl") test.advance(100)
    test.eq(test.ipc("state").copies,0) test.eq(test.ipc("state").phase,"selecting")
    test.key("s","ctrl") test.advance(100)
    test.eq(test.ipc("state").phase,"selecting")
    test.key("s") test.advance(250)
    test.eq(test.ipc("state").phase,"editing") test.truthy(test.get("capture-editor-toolbar").visible)
    test.click("capture-editor-copy") test.advance(100) test.eq(test.ipc("state").copies,1)
  end)
  test.it(style.." editor draws, resizes, undoes and copies the rendered selection",function()
    load(style)
    tool("arrow") test.ipc("begin",100,100) test.ipc("move",400,200)
    local draft=test.get("capture-editor-draft")
    test.truthy(draft.width<500 and draft.height<200,"the stroke allocated a fullscreen preview")
    if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(style.."-capture-editor.png") end
    test.ipc("finish",400,200)
    test.advance(150) test.eq(test.ipc("state").items,1)
    test.click("capture-editor-undo") test.advance(150) test.eq(test.ipc("state").items,0)
    test.click("capture-editor-redo") test.advance(150) test.eq(test.ipc("state").items,1)
    tool("select") test.ipc("begin",500,400) test.ipc("move",1000,650) test.ipc("finish",1000,650)
    test.advance(150) test.eq(test.ipc("state").crop.w,500)
    for _,role in ipairs {"nw","n","ne","w","e","sw","s","se"} do test.truthy(test.get("capture-handle-"..role).visible) end
    test.click("capture-editor-copy") test.advance(100)
    test.eq(test.ipc("state").copies,1) test.falsy(test.ipc("state").active)
    test.eq(test.ipc("state").closed,1) test.eq(#test.logs("error"),0)
  end)
  test.it(style.." editor cancels immediately and upload requires an explicit click",function()
    load(style) more("upload") test.advance(30)
    test.eq(test.ipc("state").uploads,0) test.truthy(test.get("capture-upload-confirm").visible)
    test.click("capture-upload-back") more("upload") test.click("capture-upload-confirm-send")
    test.advance(100) test.eq(test.ipc("state").uploads,1) test.falsy(test.ipc("state").active)
    test.ipc("start") test.advance(300) test.ipc("key",115,"s","") test.advance(250) test.click("capture-editor-copy")
    test.ipc("key",65307,"","") test.advance(100)
    test.falsy(test.ipc("state").active) test.eq(test.ipc("state").copies,0)
    test.eq(#test.logs("error"),0)
  end)
  test.it(style.." editor fits a compact screen and exposes preferences",function()
    load(style,800,650)
    local toolbar=test.get("capture-editor-toolbar")
    test.truthy(toolbar.x>=0 and toolbar.x+toolbar.width<=800)
    test.truthy(toolbar.y>=0 and toolbar.y+toolbar.height<=650)
    more("settings") test.advance(30)
    test.truthy(test.get("capture-editor-preferences").visible)
    test.click("capture-setting-blur-plus")
    test.click("capture-setting-close") more("cancel")
    test.falsy(test.ipc("state").active) test.eq(#test.logs("error"),0)
  end)
  test.it(style.." save dialog can cancel and save without an external dialog",function()
    load(style)
    test.ipc("option","save_dialog","true") test.ipc("option","copy_on_save","false")
    test.click("capture-editor-save") test.advance(100)
    test.truthy(test.get("capture-file-picker").visible) test.eq(test.ipc("state").saves,0)
    test.key("Escape") test.advance(50)
    test.falsy(test.get("capture-file-picker").visible) test.falsy(test.ipc("state").busy)
    test.truthy(test.ipc("state").active)
    test.click("capture-editor-save") test.advance(100) test.click("capture-picker-accept") test.advance(100)
    test.eq(test.ipc("state").saves,1) test.falsy(test.ipc("state").active)
    test.eq(#test.logs("error"),0)
  end)
  test.it(style.." toolbar wraps on a phone-sized display and key rebinding uses modifiers",function()
    load(style,360,560)
    test.click("capture-editor-draw") test.advance(220)
    for _,id in ipairs {"capture-tool-zoom","capture-fill","capture-width-plus","capture-editor-copy","capture-editor-save"} do
      local b=test.get(id)
      test.truthy(b.x>=0 and b.x+b.width<=360 and b.y>=0 and b.y+b.height<=560,id.." outside screen")
    end
    if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(style.."-capture-compact.png") end
    test.click("capture-editor-draw") test.advance(220)
    more("settings") test.click("capture-setting-rebind")
    test.key("Print","ctrl+shift") test.advance(80)
    test.eq(test.ipc("state").binding,"CTRL + SHIFT + Print")
    test.click("capture-setting-browse") test.advance(80)
    test.truthy(test.get("capture-file-picker").visible)
    test.key("Escape") test.advance(30) test.falsy(test.get("capture-file-picker").visible)
    test.eq(#test.logs("error"),0)
  end)

  test.it(style.." commits the release position and preserves the chosen crop on empty clicks",function()
    load(style,1200,800,false)
    test.press(200,150) test.release(850,500) test.advance(250)
    local crop=test.ipc("state").crop
    test.eq(crop.w,650) test.eq(crop.h,350)
    test.click(220,200) test.key("z","ctrl") test.advance(150)
    local state=test.ipc("state")
    test.eq(state.crop.x,200) test.eq(state.crop.y,150) test.eq(state.crop.w,650)
    test.eq(state.exports,0)
    more("reselect")
    test.move(120,120) local hovered=test.ipc("state").hover_updates
    test.move(140,140) test.move(160,160)
    test.eq(test.ipc("state").hover_updates,hovered)
    test.key("Escape") test.eq(#test.logs("error"),0)
  end)
  test.it(style.." throttles continuous effect previews and never overlaps workers",function()
    load(style) tool("arrow") tool("pen") tool("blur")
    test.advance(250) test.eq(test.ipc("state").exports,0)
    test.ipc("render_delay",180)
    test.press(100,100)
    for i=1,6 do test.move(100+i*20,100+i*10) test.advance(40) end
    test.truthy(test.ipc("state").exports>0,"continuous motion starved the effect preview")
    for i=7,12 do test.move(100+i*20,100+i*10) test.advance(40) end
    test.release(360,240) test.advance(800)
    local state=test.ipc("state") local op=state.last_ops[1]
    test.eq(state.maxflight,1) test.truthy(state.exports<6)
    test.eq(op[1],"blur_region") test.eq(op[2],100) test.eq(op[3],100) test.eq(op[4],260) test.eq(op[5],140)
    test.key("Escape") test.eq(#test.logs("error"),0)
  end)
  test.it(style.." exports typed text without requiring Enter and locks edits during export",function()
    load(style) tool("text") test.click(100,100) test.type("Capture text")
    test.ipc("render_delay",250)
    test.click("capture-editor-copy")
    test.truthy(test.ipc("state").busy)
    local crop=test.ipc("state").crop local handle=test.get("capture-handle-se")
    test.drag({handle.x+8,handle.y+8},{600,400},{steps=2})
    test.eq(test.ipc("state").crop.w,crop.w)
    test.advance(350)
    local state=test.ipc("state")
    test.eq(state.copies,1) test.falsy(state.active)
    if state.last_ops[1][1]=="annotations" then test.eq(state.last_ops[1][2][1].text,"Capture text")
    else test.truthy(state.last_ops[1][2]:find("Capture text",1,true)) end
    test.eq(#test.logs("error"),0)
  end)

  test.it(style.." closes settings and upload independently without leaking shortcuts",function()
    load(style) more("settings")
    test.key("c","ctrl") test.advance(100) test.eq(test.ipc("state").copies,0)
    test.key("Escape") test.falsy(test.get("capture-editor-preferences").visible)
    test.truthy(test.ipc("state").active)
    more("upload") test.key("Escape")
    test.falsy(test.get("capture-upload-confirm").visible) test.truthy(test.ipc("state").active)
    more("settings") test.click(20,20)
    test.falsy(test.get("capture-editor-preferences").visible) test.truthy(test.ipc("state").active)
    test.key("Escape") test.falsy(test.ipc("state").active) test.eq(#test.logs("error"),0)
  end)

  test.it(style.." waits for an in-flight preview before exporting and can cancel that wait",function()
    load(style) tool("arrow") test.ipc("render_delay",250)
    test.press(100,100) test.move(400,200) test.release(400,200) test.advance(100)
    test.click("capture-editor-copy") test.truthy(test.ipc("state").busy)
    test.advance(600)
    local state=test.ipc("state") test.eq(state.copies,1) test.eq(state.maxflight,1) test.falsy(state.active)
    test.ipc("start") test.advance(300) test.key("s") tool("arrow")
    test.press(100,100) test.move(400,200) test.release(400,200) test.advance(100)
    test.click("capture-editor-copy") test.key("Escape") test.advance(600)
    state=test.ipc("state") test.eq(state.copies,1) test.falsy(state.active) test.eq(#test.logs("error"),0)
  end)

  test.it(style.." renders numbered steps immediately and retains text when switching tools",function()
    load(style) tool("step") test.click(100,100) test.advance(30)
    local state=test.ipc("state")
    test.eq(state.items,1) test.truthy(state.exports>0)
    test.eq(state.last_ops[1][2][1].type,"step")
    tool("text") test.click(160,160) test.type("Keep this text")
    tool("arrow") test.advance(30)
    state=test.ipc("state") test.eq(state.items,2) test.eq(state.marks[2].text,"Keep this text")
    test.key("Escape") test.eq(#test.logs("error"),0)
  end)

  test.it(style.." dismisses tools with a click before drawing and ignores horizontal scrolling",function()
    load(style) tool("pen")
    test.click("capture-editor-draw") test.advance(220)
    test.click(100,100) test.advance(100)
    test.falsy(test.get("capture-tools-palette").visible)
    test.truthy(test.ipc("state").active) test.eq(test.ipc("state").items,0)
    local width=test.ipc("state").style.width
    test.ipc("wheel",0) test.eq(test.ipc("state").style.width,width)
    test.press(100,100) test.move(130,160) test.move(200,120)
    local draft=test.ipc("state").draft
    test.eq(draft.points[3].x,130) test.eq(draft.points[3].y,160)
    test.release(240,180) test.advance(30) test.eq(test.ipc("state").items,1)
    test.key("Escape") test.eq(#test.logs("error"),0)
  end)

  test.it(style.." selected marks show their bounds and support style changes with undo",function()
    load(style) tool("rect") test.press(100,100) test.move(250,200) test.release(250,200) test.advance(40)
    tool("select") test.click(100,150)
    test.truthy(test.get("capture-editor-selected-mark").visible)
    local original=test.ipc("state").marks[1].color
    test.click("capture-editor-draw") test.advance(220) test.click("capture-colour-4")
    test.eq(test.ipc("state").marks[1].color,"#66bb6a")
    test.key("Escape") test.key("z","ctrl") test.advance(40)
    test.eq(test.ipc("state").marks[1].color,original)
    test.key("Escape") test.eq(#test.logs("error"),0)
  end)

  test.it(style.." keeps export errors visible while hovering controls",function()
    local source=SOURCE:gsub('copies=copies%+1 cb%(true%)','copies=copies+1 cb(false,"Clipboard unavailable")',1)
    test.load("../shell/init.lua",{source=source,size={1200,800},env={CAELESTIA_STYLE=style,CAELESTIA_DRY_RUN="1"}})
    test.ipc("start") test.advance(300) test.key("s") test.advance(250)
    test.click("capture-editor-copy") test.advance(100)
    local b=test.get("capture-editor-save") test.move(b.x+b.width/2,b.y+b.height/2)
    test.eq(test.ipc("state").status,"Clipboard unavailable")
    test.eq(test.get("capture-editor-feedback-text").text,"Clipboard unavailable")
    test.key("Escape") test.eq(#test.logs("error"),0)
  end)

end

test.it("selection keeps native coordinates on an offset, scaled desktop",function()
  local source=SOURCE:gsub('local ui=require%("morf.ui"%)',[[
    morf.screens={{name="active",x=0,y=0,width=600,height=350},{name="left",x=-600,y=-100,width=600,height=450}}
    local ui=require("morf.ui")
  ]],1):gsub('backend.windows=function%(cb%) cb%(%{%{x=100,y=100,w=600,h=400,z=0%}%}%) end',
    'backend.windows=function(cb) cb({{x=100,y=100,w=300,h=200,z=0}}) end',1)
  source=source:gsub('return %{%s*snapshot=function',[[return {desktop=function(_,cb)
    cb(true,{source=fixture,width=2400,height=900,desktop={x=-600,y=-100,width=1200,height=450}})
    end,snapshot=function]],1)
  test.load("../shell/init.lua",{source=source,size={600,350},env={CAELESTIA_STYLE="tsugumori",CAELESTIA_DRY_RUN="1"}})
  test.ipc("start") test.advance(300)
  local canvas=test.get("capture-editor-canvas") local image=test.get("capture-editor-image")
  test.eq(canvas.x,-600) test.eq(canvas.y,-100) test.eq(canvas.width,1200)
  test.eq(image.x,0) test.eq(image.y,0) test.eq(image.width,600) test.eq(image.height,350)
  test.press(50,40) test.move(400,260) test.release(400,260) test.advance(300)
  local state=test.ipc("state")
  test.eq(state.phase,"editing") test.eq(state.crop.x,1300) test.eq(state.crop.y,280)
  test.eq(state.crop.w,700) test.eq(state.crop.h,440)
  more("reselect") test.key("s") test.advance(300)
  state=test.ipc("state")
  test.eq(state.crop.x,1200) test.eq(state.crop.y,200) test.eq(state.crop.w,1200) test.eq(state.crop.h,700)
  test.key("Escape") test.falsy(test.ipc("state").active) test.eq(#test.logs("error"),0)
end)

test.it("preparing a slow monitor preview times out and late completion stays cancelled",function()
  local source=SOURCE:gsub('render_ms,inflight,maxflight,last_ops,hover_updates=20,','render_ms,inflight,maxflight,last_ops,hover_updates=11000,',1)
    :gsub('{source=fixture,width=1200,height=700}',
      '{source=fixture,width=1200,height=700,desktop={x=-1200,y=0,width=2400,height=1400}}',1)
  test.load("../shell/init.lua",{source=source,size={1200,800},env={CAELESTIA_STYLE="material",CAELESTIA_DRY_RUN="1"}})
  test.ipc("start") test.advance(10050)
  test.falsy(test.ipc("state").active) test.eq(test.ipc("state").closed,1)
  test.advance(2000)
  test.falsy(test.ipc("state").active) test.eq(test.ipc("state").closed,1) test.eq(#test.logs("error"),0)
end)

test.it("native monitor previews retain their base image and export the same colours",function()
  test.load("../shell/init.lua",{size={800,650},env={CAELESTIA_STYLE="material",CAELESTIA_DRY_RUN="1"},source=[[
    local ui=require("morf.ui") local backend=require("lib.util.capture")
    morf.screens={{name="active",x=0,y=0,width=800,height=650},{name="left",x=-800,y=0,width=800,height=650}}
    backend.kind=function() return "generic" end backend.windows=function(cb) cb({}) end
    morf.screencopy.save=function(options)
      local colour=options.output=="active" and "#ff0000" or "#0000ff"
      morf.image.process {source=('<svg xmlns="http://www.w3.org/2000/svg" width="800" height="650"><rect width="800" height="650" fill="%s"/></svg>'):format(colour),
        output=options.path,ops={},on_done=options.on_done}
    end
    local factory=backend.session local owned,last_source,base,copied
    backend.session=function(options)
      owned=factory(options) local render=owned.preview or owned.render
      owned.preview=function(source,ops,cb) last_source=source render(source,ops,cb) end
      return owned
    end
    backend.copy=function(path,cb)
      local info=morf.image.info(path)
      copied={w=info.width,h=info.height,green=morf.image.pixel(path,30,30):hex(),red=morf.image.pixel(path,700,500):hex()}
      cb(true)
    end
    local editor=require("capture_editor")
    morf.surface.height=650 ui.Item {width=800,height=650,editor.node}
    editor.start("region",function() end)
    morf.ipc.fixture=function(action)
      if action=="draw" then
        base=editor.preview:get() editor.select_screen() editor.choose("rect") editor.style("color","#00ff00") editor.style("filled",true)
        editor.begin(820,20) editor.move(900,80) editor.finish(900,80)
      elseif action=="again" then
        editor.choose("arrow") editor.begin(1100,100) editor.move(1200,150) editor.finish(1200,150)
      elseif action=="copy" then editor.export("copy") end
      if copied then return {copied=copied,closed=not morf.fs.exists(owned.folder),active=editor.active:get()} end
      if not editor.active:get() then return false end
      local info,why=morf.image.info(editor.preview:get()) assert(info,editor.preview:get()..": "..tostring(why))
      return {status=editor.status:get(),w=info.width,h=info.height,green=morf.image.pixel(editor.preview:get(),30,30):hex(),
        memory=editor.preview:get():sub(1,7)=="memory:",base=base and morf.fs.exists(base),worker_width=last_source and morf.image.info(last_source).width}
    end
  ]]})
  test.wait(function() return test.ipc("fixture") end,5000)
  test.ipc("fixture","draw")
  local preview=test.wait(function() local s=test.ipc("fixture")
    test.truthy(not s or s.status=="",s and s.status or "")
    return s and s.green=="#00ff00" and s end,5000)
  test.eq(preview.w,800) test.eq(preview.h,650) test.eq(preview.worker_width,800) test.truthy(preview.base) test.truthy(preview.memory)
  test.ipc("fixture","again") test.advance(200)
  test.truthy(test.ipc("fixture").base)
  test.ipc("fixture","copy")
  local saved=test.wait(function() local s=test.ipc("fixture") return s and s.copied and s end,5000)
  test.eq(saved.copied.w,800) test.eq(saved.copied.h,650)
  test.eq(saved.copied.green,"#00ff00") test.eq(saved.copied.red,"#ff0000")
  test.truthy(saved.closed) test.falsy(saved.active) test.eq(#test.logs("error"),0)
end)

test.it("updated configuration keeps rendering with an older installed annotation library",function()
  local source=SOURCE:gsub('local editor=require%("capture_editor"%)','require("lib.util.annotation").viewport_ops=nil local editor=require("capture_editor")',1)
  test.load("../shell/init.lua",{source=source,size={1200,800},env={CAELESTIA_STYLE="material",CAELESTIA_DRY_RUN="1"}})
  test.ipc("start") test.advance(300) test.key("s") tool("arrow")
  test.press(100,100) test.move(400,200) test.release(400,200) test.advance(150)
  local state=test.ipc("state") test.eq(state.items,1) test.eq(state.last_ops[#state.last_ops][1],"crop")
  test.click("capture-editor-copy") test.advance(100) test.eq(test.ipc("state").copies,1)
  test.falsy(test.ipc("state").active) test.eq(#test.logs("error"),0)
end)
