-- Controls remain readable and usable when their available space changes.
local test=morf.test
local function load(style,source)
  test.load("../shell/init.lua",{size={520,340},env={CAELESTIA_STYLE=style=="default" and "material" or style},source=[[
    local ui=require("morf.ui")
    local kit=require("kit")
    morf.surface.height=340
  ]]..(style=="default" and [[kit=require("lib.kit.skins.default").make {variant="dark"} ]] or "")..source})
  test.advance(600)
end
local function inside(node,box,message)
  test.truthy(node.x>=box.x-.01 and node.x+node.width<=box.x+box.width+.01
    and node.y>=box.y-.01 and node.y+node.height<=box.y+box.height+.01,message)
end
local function caption(style,text) return style=="tsugumori" and text:upper() or text end
for _,style in ipairs {"material","tsugumori","default"} do
  test.it(style.." text field labels and hints fit beside the character counter",function()
    load(style,[[
      local label=morf.signal("compact.label","A long label for this preference")
      local field,input=kit.widgets.entry {id="entry",width=220,height=82,
        inset={12,24,12,28},text="text",max_length=9999,
        label=function() return label:get() end,supporting="A long hint explaining the expected value"}
      ui.Item {x=24,y=24,width=220,height=82,field}
      morf.ipc.label=function() label:set("An updated label") end
    ]])
    local box=test.get("entry-field")
    local label=test.find {text=caption(style,"A long label for this preference"),visible=true}
    test.truthy(label,"Field label did not evaluate its binding")
    inside(label,box,"Field label escapes the well")
    for _,count in ipairs {4,1000} do
      if count==1000 then test.click("entry") test.key("a","Ctrl") test.type(string.rep("a",1000)) test.advance(500) end
      local hint=test.find {text="A long hint explaining the expected value",visible=true}
      local count_text=style=="tsugumori" and ("%03d/%03d"):format(count,9999)
        or (style=="default" and (count.." / 9999") or (count.."/9999"))
      local counter=test.find {text=count_text,visible=true}
      test.truthy(hint,"Missing supporting text") test.truthy(counter,"Missing character count "..count_text)
      inside(hint,box,"Supporting text escapes the field")
      test.truthy(hint.x+hint.width+6<=counter.x,"Hint overlaps its counter")
    end
    test.ipc("label") test.advance(500)
    test.truthy(test.find {text=caption(style,"An updated label"),visible=true})
    if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(style.."-compact-field.png") end
    test.eq(test.logs("error"),{}) test.eq(test.logs("warn"),{})
  end)
  test.it(style.." menu captions fit beside checkmarks and follow their bindings",function()
    load(style,[[
      local title=morf.signal("compact.menu","A very long preference name")
      local on=morf.signal("compact.checked",true)
      ui.Item {x=24,y=24,width=180,height=40,
        kit.widgets.check_menu_item {id="menu",width=180,height=40,icon="settings",
          label=function() return title:get() end,checked=function() return on:get() end,
          on_toggled=function(v) on:set(v) end}}
      morf.ipc.title=function() title:set("Renamed preference") end
      morf.ipc.checked=function() return on:get() end
    ]])
    local label=test.find {text=caption(style,"A very long preference name"),visible=true}
    test.truthy(label,"Menu did not evaluate its caption binding")
    local box=test.get("menu")
    inside(label,box,"Menu caption crosses its row")
    test.truthy(label.x+label.width<=box.x+box.width-26,"Caption overlaps the checkmark")
    test.click("menu") test.falsy(test.ipc("checked"))
    test.ipc("title") test.advance(200)
    test.truthy(test.find {text=caption(style,"Renamed preference"),visible=true})
    if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot(style.."-compact-menu.png") end
    test.eq(test.logs("error"),{}) test.eq(test.logs("warn"),{})
  end)
  test.it(style.." horizontal scrollbars show the visible fraction and move across their rail",function()
    load(style,[[
      local value=morf.signal("compact.scroll",0)
      local width=morf.signal("compact.width",180)
      ui.Item {x=24,y=24,width=300,height=30,
        kit.widgets.scroll_bar {id="scroll",width=function() return width:get() end,height=12,
          orientation="horizontal",size=.25,value=function() return value:get() end,
          on_moved=function(v) value:set(v) end}}
      morf.ipc.value=function(v) value:set(tonumber(v)) end
      morf.ipc.width=function(v) width:set(tonumber(v)) end
    ]])
    for _,width in ipairs {180,260,18} do
      test.ipc("width",tostring(width))
      for _,value in ipairs {0,.5,1} do
        test.ipc("value",tostring(value)) test.advance(400)
        local box=test.get("scroll")
        local thumb
        for _,node in ipairs(test.nodes()) do
          if node.parent==box.handle and node.element=="Rect" and node.visible and node.height>1 then thumb=node end
        end
        test.truthy(thumb,"Missing scrollbar thumb")
        inside(thumb,box,"Horizontal thumb exceeds its rail")
        local length=math.min(width,math.max(24,width*.25))
        test.near(thumb.width,length,.1,"Scrollbar uses the vertical extent")
        test.near(thumb.x,box.x+(width-length)*value,.1)
      end
    end
    test.eq(test.logs("error"),{}) test.eq(test.logs("warn"),{})
  end)
end
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." media seek bars resize without losing seeking or overflowing",function()
    load(style,[[
      local width,value=morf.signal("compact.width",240),morf.signal("compact.seek",.4)
      ui.Item {x=24,y=24,width=400,height=80,
        kit.media_progress {id="seek",width=function() return width:get() end,
          value=function() return value:get() end,seek=function(v) value:set(v) end}}
      morf.ipc.width=function(v) width:set(tonumber(v)) end
      morf.ipc.value=function(v) if v then value:set(tonumber(v)) end return value:get() end
    ]])
    for _,width in ipairs {160,340} do
      test.ipc("width",tostring(width)) test.advance(400)
      local box=test.get("seek")
      test.near(box.width,width,.01)
      for _,value in ipairs {0,.25,1} do
        test.ipc("value",tostring(value)) test.advance(400)
        inside(test.get("media-progress-handle"),box,"Resized seek thumb crosses its control")
      end
      test.click(box.x+width*.75,box.y+box.height/2) test.advance(400)
      test.near(test.ipc("value"),.75,.01)
    end
    test.eq(test.logs("error"),{}) test.eq(test.logs("warn"),{})
  end)
end
test.it("material outlined fields keep their notch fitted to a short caption",function()
  load("material",[[
    local field=kit.widgets.url {id="address",width=260,height=60,label="Website",text="https://example.com",
      inset={16,24,44,6}}
    ui.Item {x=24,y=24,width=260,height=60,field}
  ]])
  local label
  for _,node in ipairs(test.nodes()) do
    if node.element=="Text" and node.text=="Website" and node.visible and node.opacity>0 then label=node end
  end
  test.truthy(label)
  test.truthy(label.width>20 and label.width<100,"Short caption stretched the outline notch across the field")
  test.eq(test.logs("error"),{}) test.eq(test.logs("warn"),{})
end)
test.it("material compact slider readings never cross the icon or their control",function()
  load("material",[[
    local width,value=morf.signal("compact.width",180),morf.signal("compact.value",.6)
    ui.Item {x=24,y=24,width=400,height=80,
      kit.slider {id="scale",width=function() return width:get() end,icon="zoom_in",
        value=function() return value:get() end,set=function(v) value:set(v) end,
        reading=function(v) return ("%.2f×"):format(.5+v*2.5) end}}
    morf.ipc.width=function(v) width:set(tonumber(v)) end
    morf.ipc.value=function(v) if v then value:set(tonumber(v)) end return value:get() end
  ]])
  for _,width in ipairs {120,160,180,240,360} do
    test.ipc("width",tostring(width))
    for _,value in ipairs {0,.25,.5,.6,.75,1} do
      test.ipc("value",tostring(value)) test.advance(500)
      local label,box=test.get("scale-value"),test.get("scale")
      inside(label,box,"Reading outside a "..width.." px slider at "..value)
      local icon=test.find {text="zoom_in",visible=true}
      if icon then
        inside(icon,box,"Icon outside the slider")
        test.truthy(icon.x+icon.width+4<=label.x or label.x+label.width+4<=icon.x,
          "Icon and reading overlap at width "..width..", value "..value)
      end
    end
    local track=test.get("scale-track")
    test.truthy(track.width>=12,"The readout leaves no usable slider travel")
    test.click(track.x+track.width*.75,track.y+track.height/2) test.advance(500)
    local handle=test.get("scale-handle")
    test.near(handle.x+handle.width/2,track.x+track.width*.75,1)
    if morf.env("MORF_THEME_SNAPSHOTS")=="1" then test.snapshot("material-compact-slider-"..width..".png") end
  end
  test.eq(test.logs("error"),{}) test.eq(test.logs("warn"),{})
end)
