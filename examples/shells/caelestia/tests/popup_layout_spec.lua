-- Generated popup contents fit the same padded bounds as their background.
local test=morf.test
local function load(style,source)
  test.load("../shell/init.lua",{size={520,650},env={CAELESTIA_STYLE=style=="default" and "material" or style},source=[[
    local ui=require("morf.ui")
    local kit=require("kit")
    morf.surface.width,morf.surface.height=520,650
  ]]..(style=="default" and [[
    kit=require("lib.kit.skins.default").make {variant="dark"}
    package.loaded.kit=kit
  ]] or "")..[[
    local root=ui.Item {width=520,height=650}
  ]]..source})
  test.advance(500)
  test.ipc("open") test.advance(900)
end
local function inside(node,box,pad)
  pad=pad or 0
  test.truthy(node.x>=box.x+pad-.1 and node.x+node.width<=box.x+box.width-pad+.1,
    "Popup content crosses its horizontal padding")
  test.truthy(node.y>=box.y-.1 and node.y+node.height<=box.y+box.height+.1,
    "Popup content crosses its vertical bounds")
end
for _,style in ipairs {"material","tsugumori","default"} do
  test.it(style.." menu rows stay inside the padded background",function()
    load(style,[[
      local menu=kit.widgets.menu {id="menu",root=root,width=220,padding=12,behavior={},items={
        {label="A long menu item caption",icon="folder"},
        {label="Enabled option",checked=true}}}
      morf.ipc.open=menu.open
    ]])
    local box=test.get("menu")
    for i=1,2 do inside(test.get("menu-item-"..i),box,12) end
    test.eq(test.logs("error"),{})
  end)
  test.it(style.." dialog actions stack on narrow widths and return to a row",function()
    load(style,[[
      local width=morf.signal("popup.width",250)
      local dialog=kit.widgets.dialog {id="dialog",root=root,width=function() return width:get() end,
        padding=10,behavior={},title="Preferences",body="A narrow dialog keeps each action reachable.",buttons={
          {label="Cancel"},{label="Reset"},{label="Apply",suggested=true}}}
      morf.ipc.open=dialog.open
      morf.ipc.is_open=dialog.is_open
      morf.ipc.width=function(v) width:set(tonumber(v)) end
    ]])
    for _,width in ipairs {250,470,250} do
      test.ipc("width",tostring(width)) test.advance(500)
      local box=test.get("dialog")
      local first,last=test.get("dialog-button-1"),test.get("dialog-button-3")
      for i=1,3 do inside(test.get("dialog-button-"..i),box,30) end
      if width==250 then
        test.truthy(last.y>=first.y+2*(first.height+8)-.1,"Narrow actions did not stack")
        test.near(first.x,last.x,.1)
        test.near(first.width,box.width-60,.1)
      else
        test.near(first.y,last.y,.1,"Wide dialog actions still stack")
        test.near(last.x+last.width,box.x+box.width-30,.1)
      end
    end
    test.click("dialog-button-3") test.advance(500)
    test.falsy(test.ipc("is_open"))
    test.eq(test.logs("error"),{})
  end)
  test.it(style.." unavailable dialog actions cannot activate or leave an empty row",function()
    load(style,[[
      local enabled=morf.signal("popup.enabled",false)
      local optional=morf.signal("popup.optional",false)
      local calls=0
      local dialog=kit.widgets.dialog {id="dialog",root=root,width=400,behavior={},title="Update",buttons={
        {id="optional",label="Optional",visible=function() return optional:get() end},
        {id="apply",label="Apply",enabled=function() return enabled:get() end,
          on_clicked=function() calls=calls+1 end}}}
      morf.ipc.open=dialog.open
      morf.ipc.enable=function() enabled:set(true) end
      morf.ipc.state=function() return {open=dialog.is_open(),calls=calls} end
    ]])
    test.falsy(test.get("optional").visible,"Hidden action is still drawn")
    test.click("apply")
    test.truthy(test.ipc("state").open,"Disabled action closed the dialog")
    test.eq(test.ipc("state").calls,0)
    test.ipc("enable") test.advance(100) test.click("apply")
    test.eq(test.ipc("state").calls,1)
    test.falsy(test.ipc("state").open)
    test.eq(test.logs("error"),{})
  end)
end
