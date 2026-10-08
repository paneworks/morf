-- Unavailable actions must look unavailable as a whole, remain legible and
-- keep their geometry when re-enabled. Caller-owned opacity stays intact.
local test=morf.test
local function load(style,kind,opacity)
  test.load("../shell/init.lua",{size={440,180},env={CAELESTIA_STYLE=style=="default" and "material" or style},source=[[
    local ui=require("morf.ui")
    local kit=require("kit")
    morf.surface.width,morf.surface.height=440,180
  ]]..(style=="default" and [[
    kit=require("lib.kit.skins.default").make {variant="dark"}
    package.loaded.kit=kit
  ]] or "")..([[
    local enabled=morf.signal("button.enabled",false)
    local opacity=morf.signal("button.opacity",.7)
    local clicks=0
    ui.Item {width=440,height=180,
      kit.widgets[%q] {id="action",x=40,y=50,width=180,height=40,
        label="Apply",icon="settings",%s
        enabled=function() return enabled:get() end,
        on_clicked=function() clicks=clicks+1 end}}
    morf.ipc.enable=function() enabled:set(true) end
    morf.ipc.fade=function() opacity:set(.3) end
    morf.ipc.clicks=function() return clicks end
  ]]):format(kind,opacity and "opacity=function() return opacity:get() end," or "")})
  test.advance(700)
end
for _,style in ipairs {"material","tsugumori","default"} do
  for _,kind in ipairs {"push","suggested","destructive","outlined","copy","loading","menu_item","checkbox","switch","chip_input"} do
    test.it(style.." disabled "..kind.." dims as a whole without changing size",function()
      load(style,kind)
      local box=test.get("action")
      test.truthy(box.opacity>=.35 and box.opacity<=.6,"Disabled action has no consistent visual cue")
      test.click("action")
      test.eq(test.ipc("clicks"),0,"Disabled action activated")
      test.ipc("enable") test.advance(300)
      local enabled=test.get("action")
      test.eq(enabled.opacity,1)
      test.near(enabled.width,box.width,.1) test.near(enabled.height,box.height,.1)
      test.click("action") test.eq(test.ipc("clicks"),1)
      test.eq(test.logs("error"),{}) test.eq(test.logs("warn"),{})
    end)
  end
  test.it(style.." preserves a layout's explicit opacity binding",function()
    load(style,"suggested",true)
    test.near(test.get("action").opacity,.7,.001)
    test.ipc("enable") test.advance(300)
    test.near(test.get("action").opacity,.7,.001)
    test.ipc("fade") test.advance(300)
    test.near(test.get("action").opacity,.3,.001)
  end)
end
