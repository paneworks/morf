-- Long or changing captions must leave room for their icons and fit the
-- button at compact sizes, in every skin.
local test=morf.test
local function load(style,kind,natural)
  test.load("../shell/init.lua",{size={460,260},env={CAELESTIA_STYLE=style=="default" and "material" or style},source=[[
    local ui=require("morf.ui")
    local kit=require("kit")
    morf.surface.width,morf.surface.height=460,260
  ]]..(style=="default" and [[
    kit=require("lib.kit.skins.default").make {variant="dark"}
    package.loaded.kit=kit
  ]] or "")..([[
    local width=morf.signal("caption.width",140)
    local label=morf.signal("caption.label","A longer action caption")
    local calls=0
    ui.Item {width=460,height=260,
      kit.widgets[%q] {id="action",x=40,y=80,%s height=44,
        icon="settings",label=function() return label:get() end,
        on_clicked=function() calls=calls+1 end}}
    morf.ipc.width=function(v) width:set(tonumber(v)) end
    morf.ipc.label=function(v) label:set(v) end
    morf.ipc.calls=function() return calls end
  ]]):format(kind,natural and "" or "width=function() return width:get() end,")})
  test.advance(700)
end
local function visible_caption(style,words)
  if style=="tsugumori" then words=words:upper() end
  for _,node in ipairs(test.nodes()) do
    if node.element=="Text" and node.text==words and node.visible and node.opacity>0 then return node end
  end
  error("Missing resolved caption: "..words)
end
for _,style in ipairs {"material","tsugumori","default"} do
  for _,kind in ipairs {"suggested","destructive","tonal","outlined","copy","loading",
    "disclosure_button","segment","chip_filter","chip_input","tag","hold_button"} do
    test.it(style.." "..kind.." keeps changing captions within the button",function()
      load(style,kind)
      for _,width in ipairs {140,240,110} do
        test.ipc("width",tostring(width)) test.advance(500)
        local box,label=test.get("action"),visible_caption(style,"A longer action caption")
        test.truthy(label.x>=box.x+5 and label.x+label.width<=box.x+box.width-5,
          "Caption exceeds the button's side padding")
        test.truthy(label.width>=8,"No room left for the caption")
      end
      test.ipc("label","Updated action") test.advance(500)
      visible_caption(style,"Updated action")
      if kind=="copy" then
        test.click("action") test.advance(500)
        visible_caption(style,"Copied")
        test.eq(test.ipc("calls"),1)
        test.advance(1600)
        visible_caption(style,"Updated action")
      end
      test.eq(test.logs("error"),{}) test.eq(test.logs("warn"),{})
    end)
  end
  test.it(style.." natural button width follows its caption without a ghost gap",function()
    load(style,"copy",true)
    local wide=test.get("action").width
    test.ipc("label","Copy") test.advance(500)
    test.truthy(test.get("action").width<wide-20,"Button width stays at "..test.get("action").width.." after shrinking from "..wide)
    test.ipc("label","") test.advance(500)
    local box=test.get("action")
    local icon=test.find {text="settings",visible=true}
    test.truthy(icon)
    test.near(icon.x+icon.width/2,box.x+box.width/2,.1,"Empty caption leaves its gap behind")
    test.ipc("label","A longer action caption") test.advance(500)
    test.near(test.get("action").width,wide,.1)
    test.eq(test.logs("error"),{}) test.eq(test.logs("warn"),{})
  end)
end
