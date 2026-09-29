local test=morf.test
local function load()
 test.load("../shell/init.lua",{size={800,600},env={CAELESTIA_STYLE="tsugumori"},source=[[
  local ui=require("morf.ui")
  local kit=require("kit")
  morf.surface.height=600
  local value=morf.signal("level",0.5)
  local calls=0
  local panel=require("tabbed").new {id="sample",width=600,height=function() return 400 end,tabs={
   {key="one",name="First",build=function() return ui.Item {width=500,height=200,
     kit.pill {id="control",label="Action",width=150,on_clicked=function() calls=calls+1 end},
     ui.Item {y=70,kit.slider {id="level",width=200,value=function() return value:get() end,set=function(v) value:set(v) end}},
   } end},
   {key="two",name="Second",build=function() return kit.text {text="Second page"} end},
   {key="three",name="Third",build=function() return kit.text {text="Third page"} end},
  }}
  ui.Item {width=800,height=600,panel.content}
  panel.shown(true)
  morf.ipc.select=panel.select
  morf.ipc.calls=function() return calls end
 ]]})
 test.advance(100)
end
test.it("Tsugumori buttons and sliders share reversible hover feedback",function()
 load()
 local before=test.get("control")
 test.move("control") test.advance(100)
 local middle=test.get("control-wash")
 test.truthy(middle.opacity>0 and middle.opacity<=0.055)
 test.near(test.get("control").x,before.x,0.01)
 test.near(test.get("control").width,before.width,0.01)
 test.click("control")
 test.eq(test.ipc("calls"),1)
 test.leave() test.advance(400)
 test.near(test.get("control-wash").opacity,0,0.001)
 test.move("level") test.advance(100)
 test.truthy(test.get("level-wash").opacity>0)
 test.leave() test.advance(400)
 test.near(test.get("level-wash").opacity,0,0.001)
 test.eq(#test.logs("warn"),0)
end)
test.it("rapid tab changes reveal only the latest page and release the cover",function()
 load()
 test.ipc("select","two") test.advance(90)
 test.truthy(test.get("sample-page-curtain").visible)
 test.truthy(test.get("sample-page-one").visible)
 test.ipc("select","three") test.advance(700)
 test.truthy(test.get("sample-page-three").visible)
 test.falsy(test.get("sample-page-two").visible)
 test.falsy(test.get("sample-page-curtain").visible)
 test.click("sample-tab-one") test.advance(700)
 test.truthy(test.get("sample-page-one").visible)
 test.click("control") test.eq(test.ipc("calls"),1)
 test.eq(#test.logs("error"),0)
 test.eq(#test.logs("warn"),0)
end)
