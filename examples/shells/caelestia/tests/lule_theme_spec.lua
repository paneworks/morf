local test=morf.test
local HOST=[[
  local ui=require("morf.ui")
  local W,H=tonumber(morf.env("TEST_WIDTH")),tonumber(morf.env("TEST_HEIGHT"))
  morf.surface.height=H
  local studio,calls={},{}
  local colors={} for i=1,16 do colors[i]=i%2==0 and "#90bacc" or "#26333a" end
  local files={} for i=1,7 do files[i]={path="/wallpapers/"..i..".png",name="Wallpaper "..i..".png"} end
  local initial={scheme={wallpaper="/wallpapers/1.png",colors=colors,background="#152026",foreground="#dfeef5",cursor="#90bacc"},
    selected="/wallpapers/1.png",mode="dark",method="pigment",busy=false,active=false,
    message="Choose a wallpaper, then apply its colors.",failed=false,folder="/wallpapers",files=files,
    page=1,browsing=false,preview="",preview_error=""}
  for key,value in pairs(initial) do studio[key]=morf.signal("lule.fixture."..key,value) end
  local function record(name,value) calls[#calls+1]={name=name,value=value or ""} end
  function studio.select(path) record("select",path) studio.selected:set(path) studio.browsing:set(false) return true end
  function studio.browse() record("browse") studio.browsing:set(not studio.browsing:get()) return true end
  function studio.set_folder(path) record("folder",path) studio.folder:set(path) return true end
  function studio.shuffle() record("shuffle") studio.selected:set("/wallpapers/2.png") return true end
  function studio.step(delta) record("step",delta) return true end
  function studio.apply()
    record("apply",{path=studio.selected:get(),mode=studio.mode:get(),method=studio.method:get()})
    studio.busy:set(true) return true
  end
  function studio.random_apply() studio.shuffle() return studio.apply() end
  function studio.copy(value) record("copy",value) end
  package.loaded.lule_studio=studio
  package.loaded.dashboard={drawer={set=function(on) studio.active:set(on) end}}
  local model
  local ok,models=pcall(require,"lule_model")
  if ok then local create=models.new models.new=function(...) model=create(...) return model end end
  local page=require("lule_page")
  local node=page.page or page.build(page.WIDTH,function() return page.HEIGHT end)
  if page.resize then page.resize(W,H) end
  ui.Item {width=W,height=H,visible=function() return studio.active:get() end,
    ui.Rect {width=W,height=H,color=function() return require("theme").color.surface end},node}
  morf.ipc.show=function(on) studio.active:set(on=="yes") end
  morf.ipc.finish=function(ok)
    studio.busy:set(false) studio.failed:set(ok~="yes") studio.message:set(ok=="yes" and "Applied" or "Preview hook failed")
  end
  morf.ipc.action=function(key,value) return model[key](value) end
  morf.ipc.state=function() return {calls=calls,mode=studio.mode:get(),method=studio.method:get(),busy=studio.busy:get(),
    selected=studio.selected:get(),page=studio.page:get(),folder=studio.folder:get(),active=studio.active:get()} end
]]
local function load(style,w,h)
  test.load("../shell/init.lua",{source=HOST,size={w or 960,h or (style=="material" and 489 or 680)},env={
    CAELESTIA_STYLE=style,TEST_WIDTH=tostring(w or 960),TEST_HEIGHT=tostring(h or (style=="material" and 489 or 680)),CAELESTIA_DRY_RUN="1"}})
  test.advance(100)
end
local function state() return test.ipc("state") end
local function open() test.ipc("show","yes") test.advance(2400) end
local function shot(name) if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(name..".png") end end
local function reach(id)
  local scroll=test.find {id="lule-scroll"}
  if not scroll then return end
  local item=test.get(id)
  if item.y<scroll.y or item.y+item.height>scroll.y+scroll.height then
    test.wheel(0,item.y-scroll.y-30,{x=scroll.x+scroll.width-2,y=scroll.y+30}) test.advance(300)
  end
end
local function click(id) reach(id) test.click(id) end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." Lule preserves preview, palette, folder and application controls",function()
    load(style) open() shot(style.."-lule-workspace")
    click("lule-browse") test.advance(200) shot(style.."-lule-browser")
    click("lule-files-next") test.eq(state().page,2)
    click("lule-file-2") test.eq(state().selected,"/wallpapers/6.png")
    click("lule-color-1") test.eq(state().calls[#state().calls],{name="copy",value="#90bacc"})
    click("lule-folder") test.key("a","Ctrl") test.type("/new wallpapers")
    click("lule-use-folder") test.eq(state().folder,"/new wallpapers")
    click("lule-mode-light") click("lule-method-tonal")
    click("lule-apply") test.truthy(state().busy)
    test.eq(state().calls[#state().calls],{name="apply",value={path="/wallpapers/6.png",mode="light",method="tonal"}})
    local count=#state().calls
    click("lule-random-apply") click("lule-mode-dark") test.eq(#state().calls,count) test.eq(state().mode,"light")
    test.ipc("finish","no") test.advance(100)
    test.eq(test.get("lule-status").text,"Preview hook failed")
    click("lule-random-apply") test.eq(state().calls[#state().calls].name,"apply")
    test.eq(state().selected,"/wallpapers/2.png")
    test.ipc("finish","yes")
    click("lule-folder") test.key("Escape") test.falsy(state().active)
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
end
test.it("Lule model validates options and ignores hidden or busy page actions",function()
  load("tsugumori")
  test.falsy(test.ipc("action","apply")) test.falsy(test.ipc("action","set_mode","light"))
  test.eq(#state().calls,0)
  open()
  test.falsy(test.ipc("action","set_mode","invalid")) test.falsy(test.ipc("action","set_method","invalid"))
  test.eq(state().mode,"dark") test.eq(state().method,"pigment")
  test.truthy(test.ipc("action","apply"))
  test.falsy(test.ipc("action","set_folder","/other")) test.falsy(test.ipc("action","shuffle"))
  test.eq(#state().calls,1)
end)
test.it("Tsugumori compact Lule keeps Apply visible and reveals scrolled titles",function()
  load("tsugumori",430,360) open()
  local apply=test.get("lule-apply")
  test.truthy(apply.x>=0 and apply.x+apply.width<=430 and apply.y+apply.height<=360)
  local paper=test.get("lule-wallpaper-card") local colors=test.get("lule-colors-card")
  test.truthy(colors.y>=paper.y+paper.height)
  reach("lule-colors-heading") test.advance(200)
  test.truthy(test.get("lule-colors-heading-text").text~="COLORS")
  test.advance(2200) test.eq(test.get("lule-colors-heading-text").text,"COLORS")
  shot("lule-compact-palette")
  click("lule-cursor") test.eq(state().calls[#state().calls],{name="copy",value="#90bacc"})
  click("lule-method-tonal") test.eq(state().method,"tonal")
  local method=test.get("lule-method-tonal")
  test.truthy(method.width>100 and method.x+method.width<=430)
  shot("lule-compact-methods")
  test.ipc("show","no") test.advance(100) test.ipc("show","yes") test.advance(200)
  test.truthy(test.get("lule-wallpaper-heading-text").text~="WALLPAPER")
  test.eq(test.get("lule-scroll").y,0)
  test.truthy(test.get("lule-wallpaper-card").y<30)
  test.eq(#test.logs("error"),0)
end)
test.it("Lule studio never executes wallpaper hooks in preview mode",function()
  test.load("../shell/init.lua",{size={200,100},env={CAELESTIA_DRY_RUN="1"},source=[[
    local calls=0
    package.loaded["lib.lule"]={watch=function() return morf.signal("dry.scheme",{wallpaper="/fixture.png"}) end,
      generate=function() calls=calls+1 return true end}
    local studio=require("lule_studio")
    require("morf.ui").Item {width=200,height=100}
    morf.ipc.apply=studio.apply
    morf.ipc.state=function() return {calls=calls,busy=studio.busy:get(),failed=studio.failed:get(),message=studio.message:get()} end
  ]]})
  test.falsy(test.ipc("apply"))
  local s=state() test.eq(s.calls,0) test.falsy(s.busy) test.truthy(s.failed)
  test.eq(s.message,"Applying wallpapers is disabled in this preview.")
end)
