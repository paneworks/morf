-- Regressions for controls at their ends and in narrow, uneven-sized rows.
local test=morf.test
local function load(style,source)
  test.load("../shell/init.lua",{size={520,340},env={CAELESTIA_STYLE=style},source=[[
    local ui=require("morf.ui")
    local kit=require("kit")
    local P=require("themes.layouts.page")
    morf.surface.height=340
  ]]..source})
  test.advance(800)
end
local function inside(node,box,message)
  test.truthy(node.x>=box.x-.01 and node.x+node.width<=box.x+box.width+.01
    and node.y>=box.y-.01 and node.y+node.height<=box.y+box.height+.01,message)
end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." media seeking follows the hand and commits only on release",function()
    load(style,[[
      local value=morf.signal("geometry.seek",.2)
      local seeks=0
      ui.Item {x=24,y=24,width=400,height=80,
        kit.media_progress {id="direct-seek",width=360,value=function() return value:get() end,
          active=function() return true end,playing=function() return true end,
          seek=function(v) seeks=seeks+1 value:set(v) end}}
      morf.ipc.state=function() return {value=value:get(),seeks=seeks} end
    ]])
    local box=test.get("direct-seek")
    local y=box.y+box.height/2
    test.press(box.x+box.width*.2,y)
    local x=box.x+box.width*.8
    test.move(x,y) test.advance(16)
    local handle=test.get("media-progress-handle")
    test.near(handle.x+handle.width/2,x,1,"Seeking thumb lags behind the hand")
    test.eq(test.ipc("state").seeks,0)
    test.release(x,y) test.advance(400)
    test.near(test.ipc("state").value,.8,.01)
    test.eq(test.ipc("state").seeks,1)
    test.eq(#test.logs("warn"),0)
  end)
  test.it(style.." slider thumbs follow the pointer without a trailing animation",function()
    load(style,[[
      local value=morf.signal("geometry.value",.2)
      ui.Item {x=24,y=24,width=400,height=80,
        kit.slider {id="direct",width=360,label=false,
          value=function() return value:get() end,set=function(v) value:set(v) end}}
      morf.ipc.value=function() return value:get() end
    ]])
    local track=test.get("direct-track")
    local y=track.y+track.height/2
    test.press(track.x+track.width*.2,y)
    for _,position in ipairs {.8,.35,1,0} do
      local x=track.x+track.width*position
      test.move(x,y) test.advance(16)
      test.near(test.ipc("value"),position,.01)
      local handle=test.get("direct-handle")
      test.near(handle.x+handle.width/2,x,1,"Thumb lags behind the pointer")
    end
    test.release(track.x,y) test.advance(400)
    test.eq(#test.logs("error"),0)
    test.eq(#test.logs("warn"),0)
  end)
  test.it(style.." buttons without an explicit width keep their labels visible",function()
    load(style,[[
      local label=morf.signal("geometry.label","Apply")
      ui.Item {x=24,y=24,width=460,height=60,
        kit.pill {id="natural",label=function() return label:get() end,icon="check"}}
      morf.ipc.label=function(v) label:set(v) end
    ]])
    local small=test.get("natural").width
    test.truthy(small>=60,"Intrinsic button has no usable width")
    test.ipc("label","Apply changes") test.advance(500)
    test.truthy(test.get("natural").width>small,"Intrinsic button does not follow its label")
    test.eq(#test.logs("warn"),0)
  end)
  test.it(style.." slider and media thumbs stay inside their control at both endpoints",function()
    load(style,[[
      local value=morf.signal("geometry.value",0)
      ui.Column {x=24,y=24,width=180,gap=32,
        kit.slider {id="plain",width=180,height=26,label=false,value=function() return value:get() end,set=function(v) value:set(v) end},
        kit.media_progress {id="seek",width=180,value=function() return value:get() end,seek=function(v) value:set(v) end},
      }
      morf.ipc.value=function(v) value:set(tonumber(v)) end
    ]])
    for _,value in ipairs {0,1} do
      test.ipc("value",tostring(value)) test.advance(1800)
      inside(test.get("plain-handle"),test.get("plain"),"Slider thumb crosses its edge")
      inside(test.get("media-progress-handle"),test.get("seek"),"Seek thumb crosses its edge")
    end
    test.eq(#test.logs("error"),0)
  end)
  test.it(style.." plain sliders track a bound width and map clicks onto the resized rail",function()
    load(style,[[
      local width,value=morf.signal("geometry.width",240),morf.signal("geometry.value",.5)
      ui.Item {x=24,y=24,width=400,height=80,
        kit.slider {id="resizable",width=function() return width:get() end,label=false,
          value=function() return value:get() end,set=function(v) value:set(v) end}}
      morf.ipc.width=function(v) width:set(tonumber(v)) end
      morf.ipc.value=function() return value:get() end
    ]])
    for _,width in ipairs {160,340} do
      test.ipc("width",tostring(width)) test.advance(800)
      test.near(test.get("resizable").width,width,.01)
      local track=test.get("resizable-track")
      test.click(track.x+track.width*.25,track.y+track.height/2) test.advance(800)
      test.near(test.ipc("value"),.25,.01)
      inside(test.get("resizable-handle"),test.get("resizable"),"Resized thumb overflows")
    end
    test.eq(#test.logs("error"),0)
  end)
  test.it(style.." page buttons fill an uneven row and keep long labels inside each button",function()
    load(style,[[
      ui.Item {x=24,y=24,width=320,height=60,
        P.buttons {width=320,
          {id="first",label="A very long action",icon="settings"},
          {id="middle",label="Headphones"},
          {id="last",label="Last"}}}
    ]])
    local first,middle,last=test.get("first"),test.get("middle"),test.get("last")
    test.near(last.x+last.width,first.x+320,.01,"Button row leaves a trailing gap")
    test.near(middle.x-first.x-first.width,8,.01)
    test.near(last.x-middle.x-middle.width,8,.01)
    local by_handle={}
    for _,node in ipairs(test.nodes()) do by_handle[node.handle]=node end
    for _,node in ipairs(test.nodes()) do
      if node.element=="Text" and node.visible and node.opacity>0 then
        local parent=node.parent and by_handle[node.parent]
        local clipped=false
        while parent do
          clipped=clipped or parent.clip
          if parent.id=="first" or parent.id=="middle" or parent.id=="last" then
            if not clipped then inside(node,parent,"Button caption overlaps a neighbour") end
            break
          end
          parent=parent.parent and by_handle[parent.parent]
        end
      end
    end
    if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(style.."-compact-buttons.png") end
  end)
  test.it(style.." page buttons honor disabled state and become usable when enabled",function()
    load(style,[[
      local enabled=morf.signal("geometry.enabled",false)
      local calls=0
      ui.Item {x=24,y=24,width=200,height=60,P.button {id="action",width=180,label="Apply",
        enabled=function() return enabled:get() end,on_clicked=function() calls=calls+1 end}}
      morf.ipc.enable=function() enabled:set(true) end
      morf.ipc.calls=function() return calls end
    ]])
    test.click("action") test.eq(test.ipc("calls"),0,"Disabled button accepted a click")
    test.ipc("enable") test.advance(100) test.click("action") test.eq(test.ipc("calls"),1)
  end)
  test.it(style.." scrollbar thumbs fit even when the viewport is shorter than their minimum",function()
    load(style,[[
      ui.Item {x=24,y=24,width=30,height=18,
        kit.widgets.scroll_bar {id="scroll",width=12,height=18,orientation="vertical",value=1,size=.1}}
    ]])
    local box=test.get("scroll")
    for _,node in ipairs(test.nodes()) do
      if node.parent==box.handle and node.element=="Rect" and node.visible then
        inside(node,box,"Scrollbar thumb is taller than its viewport")
      end
    end
  end)
end
