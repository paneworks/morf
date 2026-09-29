local test=morf.test
local HOST=[[
  local ui=require("morf.ui")
  local theme,session=require("theme"),require("themes.session")
  local switcher=require("themes.switcher")
  local completed,failed
  morf.on_reload_completed=function(fn) completed=fn end
  morf.on_reload_failed=function(fn) failed=fn end
  morf.broadcast=function() return false end
  morf.reload=function() end
  local card=require("kit").card {id="sample-card",x=18,y=50,width=480,height=170,
    require("kit").heading {id="sample-title",text="Appearance",x=18,y=20,width=360},
    require("kit").text {text="The same workspace, a different face.",x=18,y=75,width=440},
  }
  local content=ui.Item {width=520,height=260,
    require("kit").text {text="SHELL / APPEARANCE",x=18,y=15,width=440},card}
  local drawer=require("drawer").new {name="sample",edge="top",width=520,height=260,content=content}
  if not session.restoring then drawer.set(true) end
  theme.motion.entries({{node=card}},true)
  local model={drawers={drawer},desk=function() return 0,0,640,400 end,overlays={},triggers={},
    bar=ui.Item {width=1,height=1},
    rail={node=ui.Item {width=1,height=1},shape=ui.SdfShape {width=1,height=1,opacity=0}},
    levels={node=ui.Item {width=1,height=1},shape=ui.SdfShape {width=1,height=1,opacity=0}}}
  morf.surface.height=400
  ui.Item {width=640,height=400,
    ui.Rect {width=640,height=400,color="#1b202b"},require("themes").view("frame").build(model)}
  switcher.start(function() return nil end)
  morf.ipc.change=switcher.request
  morf.ipc.saved=session.snapshot
  morf.ipc.complete=function() completed() end
  morf.ipc.fail=function() failed() end
  morf.ipc.state=function() return {busy=switcher.busy:get(),corner=drawer.shape.bottom_left_radius} end
]]
local function load(style,seed)
  local prefix=""
  if seed then prefix=([[
    local native=morf.reloadable
    local bank=native("caelestia.theme.session",morf.json.decode(%q))
    morf.reloadable=function(name,initial) if name=="caelestia.theme.session" then return bank end return native(name,initial) end
  ]]):format(morf.json.encode(seed)) end
  test.load("../shell/init.lua",{source=prefix..HOST,size={640,400},env={CAELESTIA_STYLE=style,
    CAELESTIA_APPEARANCE=morf.env("XDG_CACHE_HOME").."/transition-appearance.json"}})
end
local function shot(name)
  if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(name..".png") end
end
for _,pair in ipairs {{"material","tsugumori"},{"tsugumori","material"}} do
  test.it(pair[1].." keeps the frame visible and morphs into "..pair[2],function()
    load(pair[1]) test.advance(1000)
    shot(pair[1].."-before")
    test.ipc("change",pair[2]) test.advance(100)
    local cover=test.get("theme-cover-sample")
    test.truthy(cover.x+cover.width>60 and cover.x<60)
    test.near(test.get("frame").opacity,1,.001)
    shot(pair[1].."-covering")
    test.advance(150)
    local seed=test.ipc("saved")
    test.truthy(seed.switching)
    load(pair[1],seed) test.advance(1)
    test.near(test.get("sample-card").opacity,1,.001)
    test.near(test.get("sample-card").width,480,.1)
    test.near(test.ipc("state").corner,pair[1]=="material" and 25 or 2,.001)
    shot(pair[1].."-covered-new")
    test.ipc("complete") test.advance(100)
    test.near(test.get("frame").opacity,1,.001)
    local radius=test.ipc("state").corner
    test.truthy(radius>2 and radius<25)
    shot(pair[1].."-revealing")
    test.advance(400)
    test.falsy(test.ipc("state").busy)
    test.falsy(test.get("theme-cover-sample").visible)
    test.near(test.ipc("state").corner,pair[2]=="material" and 25 or 2,.001)
    shot(pair[1].."-after")
    test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
  end)
end
test.it("font changes dissolve and a failed switch clears its cover",function()
  load("tsugumori") test.advance(1000)
  test.ipc("change","font:Goku") test.advance(100)
  local cover=test.get("theme-cover-sample")
  test.truthy(cover.opacity>0 and cover.opacity<1)
  test.near(cover.x,60,.1)
  test.advance(150)
  local seed=test.ipc("saved")
  test.eq(seed.transition.mode,"dissolve")
  load("tsugumori",seed) test.advance(1) test.ipc("complete") test.advance(100)
  test.truthy(test.get("theme-cover-sample").opacity<1)
  test.advance(400) test.falsy(test.ipc("state").busy)
  test.ipc("change","material") test.advance(100)
  local before=test.get("theme-cover-sample").x
  test.ipc("fail") test.advance(70)
  test.truthy(test.get("theme-cover-sample").x<before,"cancelled wipe did not reverse")
  test.advance(200)
  test.falsy(test.ipc("state").busy)
  test.near(test.get("frame").opacity,1,.001)
  test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
end)
