-- A control's visible thumb must follow the input geometry after a resize,
-- and a seek must stay under the hand while playback updates are animated.
local test=morf.test
local function load(source)
  test.load {size={440,320},source=[[
    local ui=require("morf.ui")
    local kit=require("lib.kit.skins.default").make {variant="dark"}
    morf.surface.width,morf.surface.height=440,320
  ]]..source}
  test.advance(500)
end
local function clean()
  test.eq(test.logs("error"),{}) test.eq(test.logs("warn"),{})
end
test.it("default sliders resize their rails and keep pointer mapping aligned",function()
  load([[
    local width=morf.signal("range.width",240)
    local value=morf.signal("range.value",.5)
    ui.Item {width=400,height=80,
      kit.slider {id="slider",width=function() return width:get() end,label=false,
        value=function() return value:get() end,set=function(v) value:set(v) end}}
    morf.ipc.width=function(v) width:set(tonumber(v)) end
    morf.ipc.value=function() return value:get() end
  ]])
  for _,width in ipairs {160,340} do
    test.ipc("width",tostring(width)) test.advance(100)
    local box=test.get("slider")
    test.near(box.width,width,.01)
    local x,y=box.x+width*.75,box.y+box.height/2
    test.click(x,y) test.advance(16)
    local knob=test.get("slider-handle")
    test.near(knob.x+knob.width/2,x,1)
    test.near(test.ipc("value"),.75,.05)
    test.truthy(knob.x>=box.x and knob.x+knob.width<=box.x+box.width)
  end
  clean()
end)
test.it("default vertical sliders follow a bound height",function()
  load([[
    local height=morf.signal("range.height",160)
    local value=morf.signal("range.value",.5)
    ui.Item {width=100,height=300,
      kit.widgets.vertical_slider {id="vertical",width=40,height=function() return height:get() end,
        value=function() return value:get() end,on_moved=function(v) value:set(v) end}}
    morf.ipc.height=function(v) height:set(tonumber(v)) end
    morf.ipc.value=function() return value:get() end
  ]])
  for _,height in ipairs {100,240} do
    test.ipc("height",tostring(height)) test.advance(100)
    local box=test.get("vertical")
    test.near(box.height,height,.01)
    test.click(box.x+box.width/2,box.y+height*.25) test.advance(16)
    test.near(test.ipc("value"),.75,.08)
  end
  clean()
end)
test.it("default media seeking follows the hand immediately and commits on release",function()
  load([[
    local value=morf.signal("range.value",.2)
    local calls=0
    ui.Item {width=400,height=80,
      kit.media_progress {id="seek",width=360,value=function() return value:get() end,
        playing=function() return true end,
        seek=function(v) calls=calls+1 value:set(v) end}}
    morf.ipc.state=function() return {calls=calls,value=value:get()} end
  ]])
  local box=test.get("seek")
  local y=box.y+box.height/2
  test.press(box.x+box.width*.2,y)
  for _,position in ipairs {.8,.35,.65} do
    local x=box.x+box.width*position
    test.move(x,y) test.advance(16)
    local knob=test.get("media-progress-handle")
    test.near(knob.x+knob.width/2,x,1,"Seek handle trails the pointer")
    test.eq(test.ipc("state").calls,0)
  end
  test.release(box.x+box.width*.65,y) test.advance(100)
  test.eq(test.ipc("state").calls,1)
  test.near(test.ipc("state").value,.65,.02)
  clean()
end)
test.it("default media bars and compact slider heights can be bound",function()
  load([[
    local width,height=morf.signal("range.width",240),morf.signal("range.height",26)
    local value=morf.signal("range.value",.5)
    ui.Column {width=400,gap=20,
      kit.slider {id="slider",width=240,height=function() return height:get() end,label=false,
        value=function() return value:get() end,set=function(v) value:set(v) end},
      kit.media_progress {id="seek",width=function() return width:get() end,
        value=function() return value:get() end,seek=function(v) value:set(v) end}}
    morf.ipc.resize=function() width:set(340) height:set(44) end
  ]])
  test.near(test.get("slider").height,34,.01)
  test.ipc("resize") test.advance(100)
  test.near(test.get("slider").height,52,.01)
  local box=test.get("seek")
  test.near(box.width,340,.01)
  local x,y=box.x+box.width*.8,box.y+box.height/2
  test.press(x,y) test.advance(16)
  local knob=test.get("media-progress-handle")
  test.near(knob.x+knob.width/2,x,1)
  test.release(x,y)
  clean()
end)
