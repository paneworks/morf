local test = morf.test
local function load()
  test.load("../shell/init.lua", {size={520,340},env={CAELESTIA_STYLE="tsugumori"},source=[[
    local ui, kit, theme = require("morf.ui"), require("kit"), require("theme")
    morf.surface.height=340
    local level=morf.signal("meters.level",.42)
    local function value() return level:get() end
    local function set(v) level:set(v) end
    ui.Rect {width=520,height=340,color=function() return theme.color.surface end,
      kit.card {x=32,y=24,width=456,height=290,
        kit.heading {id="meter-title",x=20,y=16,text="Levels",width=400},
        ui.Item {x=20,y=60,width=416,height=52,kit.slider {id="volume",width=416,value=value,set=set,icon="volume_up"}},
        ui.Item {x=20,y=120,width=416,height=52,kit.slider {id="brightness",width=416,value=value,set=set,icon="brightness_medium"}},
        ui.Item {x=20,y=190,width=416,height=30,kit.slider {id="compact",width=416,height=22,value=value,set=set}},
        ui.Item {x=20,y=240,width=416,height=34,kit.media_progress {width=416,value=value}},
      },
    }
    morf.ipc.value=function(v) if v then set(tonumber(v)) end return value() end
  ]]})
  test.advance(1200)
end

test.it("instrument sliders follow dragging immediately and clamp at either end",function()
  load()
  local area=test.get("volume")
  local x,y=area.x,area.y+area.height/2
  test.press(x+area.width/2,y)
  test.move(x+area.width-7,y) test.advance(16)
  test.near(test.ipc("value"),1,.001)
  local grip=test.get("volume-handle")
  test.near(grip.x+grip.width/2,x+area.width-7,.1)
  test.move(x-20,y) test.advance(16)
  test.near(test.ipc("value"),0,.001)
  test.release(x-20,y) test.advance(200)
  test.near(test.get("volume-level").width,0,.01)
  test.click(x+area.width-1,y) test.advance(200)
  test.near(test.ipc("value"),1,.001)
  test.eq(test.get("volume-value").text,"100%")
  test.wheel(0,1,{x=x+area.width/2,y=y}) test.advance(200)
  test.near(test.ipc("value"),.95,.001)
  test.leave() test.advance(400)
  test.truthy(test.settle(500)<100)
  test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
end)

test.it("readings ease together and compact sliders keep their labels clear",function()
  load()
  local start=test.get("brightness-level").width
  test.ipc("value",".85") test.advance(48)
  local fill,grip,track=test.get("brightness-level"),test.get("brightness-handle"),test.get("brightness-track")
  test.truthy(fill.width>start and fill.width<track.width*.85, ("start=%s fill=%s target=%s"):format(start,fill.width,track.width*.85))
  test.near(fill.x+fill.width,grip.x+grip.width/2,.1)
  test.advance(200)
  test.near(test.get("brightness-level").width,track.width*.85,.1)
  for _,v in ipairs {0,1} do
    test.ipc("value",tostring(v)) test.advance(200)
    local handle,label=test.get("compact-handle"),test.get("compact-value")
    test.truthy(handle.x+handle.width<label.x)
    test.near(test.get("media-progress-fill").width,416*v,.1)
  end
  test.ipc("value",".42") test.advance(250)
  if morf.env("MORF_THEME_SNAPSHOTS")=="1" then
    test.snapshot("meters-rest.png")
    test.move("brightness") test.advance(200)
    test.snapshot("meters-hover.png")
  end
  test.eq(#test.logs("error"),0) test.eq(#test.logs("warn"),0)
end)
