-- Specialized ranges should have the same stable grip and direct drag
-- response as the main volume slider.
local test=morf.test
local cases={
  {name="discrete_slider",right=40},
  {name="log_slider",right=64},
  {name="volume",right=46},
  {name="brightness",right=46},
  {name="vertical_slider",vertical=true},
  {name="fader",vertical=true,fader=true},
}
local function load(case)
  test.load("../shell/init.lua",{size={440,360},env={CAELESTIA_STYLE="material"},source=([[
    local ui=require("morf.ui")
    local kit=require("kit")
    morf.surface.width,morf.surface.height=440,360
    local value=morf.signal("material.range.value",1.5)
    ui.Item {width=420,height=340,
      kit.widgets[%q] {id="range",x=50,y=50,width=%d,height=%d,from=1,to=2,
        step=.01,value=function() return value:get() end,
        on_moved=function(v) value:set(v) end}}
    morf.ipc.value=function(v) value:set(1+tonumber(v)) end
  ]]):format(case.name,case.fader and 76 or (case.vertical and 60 or 320),case.vertical and 260 or 64)})
  test.advance(700)
end
local function grip(case)
  for _,node in ipairs(test.nodes()) do
    if case.fader and node.element=="Item" and node.visible and math.abs(node.width-44)<.01 and math.abs(node.height-20)<.01 then return node end
    if node.element=="Rect" and node.visible and node.opacity>0 then
      if case.vertical and not case.fader and node.width>=30 and node.height<=4.01 then return node end
      if not case.vertical and node.height>=30 and node.width<=4.01 then return node end
    end
  end
  error("Missing slider grip")
end
for _,case in ipairs(cases) do
  test.it("material "..case.name.." follows the pointer with a stable grip",function()
    load(case)
    local box=test.get("range")
    local before=grip(case)
    local function point(position)
      if case.vertical then return box.x+(case.fader and 48 or box.width/2),box.y+2+(box.height-30)*position end
      return box.x+2+(box.width-case.right-4)*position,box.y+(case.name=="log_slider" and 16 or box.height/2)
    end
    test.press(point(.5))
    for _,position in ipairs {.8,.3,.9} do
      local x,y=point(position)
      test.move(x,y) test.advance(16)
      local handle=grip(case)
      test.near(handle.x+handle.width/2,x,1,"Grip trails horizontal movement")
      test.near(handle.y+handle.height/2,y,1,"Grip trails vertical movement")
      test.near(case.vertical and handle.height or handle.width,
        case.vertical and before.height or before.width,.01,"Grip changes thickness while held")
    end
    test.release(point(.9)) test.advance(500)
    test.eq(test.logs("error"),{}) test.eq(test.logs("warn"),{})
  end)
  test.it("material "..case.name.." keeps short track ends balanced",function()
    load(case)
    for _,value in ipairs {.04,.08,.92,.96} do
      test.ipc("value",tostring(value)) test.advance(700)
      for _,node in ipairs(test.nodes()) do
        if node.element=="Rect" and node.visible and node.opacity>0 then
          if case.vertical and node.height>4.01 and node.height<16 then
            test.truthy(node.width<=node.height+.01,"Short vertical track becomes a wide sliver")
          elseif not case.vertical and node.width>4.01 and node.width<16 then
            test.truthy(node.height<=node.width+.01,"Short horizontal track becomes a tall sliver")
          end
        end
      end
    end
    test.eq(test.logs("error"),{})
  end)
end

test.it("material range sliders let either handle follow the hand independently",function()
  test.load("../shell/init.lua",{size={420,200},env={CAELESTIA_STYLE="material"},source=[[
    local ui=require("morf.ui")
    local kit=require("kit")
    morf.surface.width,morf.surface.height=420,200
    ui.Item {width=420,height=200,
      kit.widgets.range_slider {id="range",x=50,y=70,width=320,height=64,first=.2,second=.8}}
  ]]})
  test.advance(700)
  local function handles()
    local out={}
    for _,node in ipairs(test.nodes()) do
      if node.element=="Rect" and node.visible and node.height==40 and node.width<=4.01 then out[#out+1]=node end
    end
    table.sort(out,function(a,b) return a.x<b.x end)
    test.eq(#out,2)
    return out
  end
  local box=test.get("range")
  local function point(position) return box.x+2+(box.width-4)*position,box.y+box.height-22 end
  for index,positions in ipairs {{.2,.4},{.8,.6}} do
    test.press(point(positions[1]))
    local x,y=point(positions[2])
    test.move(x,y) test.advance(16)
    local bars=handles()
    test.near(bars[index].x+bars[index].width/2,x,1)
    test.near(bars[index].width,4,.01)
    local other=index==1 and 2 or 1
    test.near(bars[other].x+bars[other].width/2,(point(index==1 and .8 or .4)),1)
    test.release(x,y) test.advance(500)
  end
  test.eq(test.logs("error"),{}) test.eq(test.logs("warn"),{})
end)
