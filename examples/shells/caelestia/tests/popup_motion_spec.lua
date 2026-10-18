-- The caelestia themes' popup motion through lib.kit.popup: a menu unfolds
-- from its button and a bottom sheet rises, sampled on the virtual clock as
-- they open, rest, close and leave.
local test=morf.test
local function load(style)
  test.load("../shell/init.lua",{size={520,650},env={CAELESTIA_STYLE=style},source=[[
    local ui=require("morf.ui")
    local kit=require("kit")
    morf.surface.width,morf.surface.height=520,650
    local root=ui.Item {width=520,height=650}
    local opener=kit.widgets.push {id="opener",x=20,y=20,width=100,height=30,label="Menu"}
    ui.reparent(opener,root)
    local popups={
      menu=kit.widgets.menu {id="menu",width=200,items={{label="One"},{label="Two"}}},
      sheet=kit.widgets.bottom_sheet {id="sheet",root=root,width=400,height=200,text="Sheet"},
    }
    morf.ipc.open=function(name) popups[name].open(name=="menu" and opener or nil) end
    morf.ipc.close=function(name) popups[name].close("closed") end
    morf.ipc.pose=function(name)
      local n=popups[name].node
      return {s=n.scale,sx=n.scale_x,sy=n.scale_y,tx=n.translate_x,ty=n.translate_y,op=n.opacity,
        ox=n.transform_origin_x,oy=n.transform_origin_y}
    end
  ]]})
  test.advance(500)
end
-- Each pose field the samples name, and the box, against the popup's node.
local function expect(name,want,label)
  local pose=test.ipc("pose",name)
  for key,value in pairs(want) do
    if key=="box" then
      local node=test.get(name)
      test.near(node.y,value[1],1e-3,label..": y")
      test.near(node.height,value[2],1e-3,label..": height")
    else
      test.near(pose[key],value,1e-4,("%s: %s = %s"):format(label,key,tostring(pose[key])))
    end
  end
end
local function samples(name,list)
  local now=0
  for _,sample in ipairs(list) do
    test.advance(sample[1]-now) now=sample[1]
    expect(name,sample[2],("%s at %d ms"):format(name,now))
  end
end
local CASES={
  material={
    menu={open={{0,{sy=0.55,s=1,op=0,ox=0,oy=0,box={54,39.6}}},{16,{sy=0.59394,op=0.39758}},
      {48,{sy=0.75517,op=0.67859}},{96,{sy=0.95241,op=0.87914}}},
      rest={sy=1,op=1,box={54,72}},close={{48,{sy=0.79483,op=0.32141}},{96,{sy=0.59759,op=0.12086}}}},
    sheet={open={{0,{ty=256,op=1,box={481,200}}},{16,{ty=233.04184}},{48,{ty=151.67619}},{96,{ty=51.09521}}},
      rest={ty=0,op=1,box={225,200}},close={{48,{ty=104.32379,op=1}},{96,{ty=204.90480}}}},
  },
  tsugumori={
    menu={open={{0,{sy=0.08,sx=1,op=0,ox=0,oy=0,box={54,5.76}}},{16,{sy=0.28497,op=0.24367}},
      {48,{sy=0.56809,op=0.60563}},{96,{sy=0.79723,op=0.89837}}},
      rest={sy=1,op=1,box={54,72}},close={{48,{sy=0.51191,op=0.39437}},{96,{sy=0.28277,op=0.10163}}}},
    sheet={open={{0,{ty=240,op=1,box={465,200}}},{16,{ty=186.52875}},{48,{ty=112.67170}},{96,{ty=52.89548}}},
      rest={ty=0,op=1,box={225,200}},close={{48,{ty=127.32830,op=1}},{96,{ty=187.10452}}}},
  },
}
for _,style in ipairs {"material","tsugumori"} do
  for _,name in ipairs {"menu","sheet"} do
    local case=CASES[style][name]
    test.it(style.." "..name.." comes in from its theme's pose and leaves back to it",function()
      load(style)
      test.ipc("open",name)
      samples(name,case.open)
      test.settle(2000)
      expect(name,case.rest,"at rest")
      test.ipc("close",name)
      samples(name,case.close)
      test.truthy(test.find {id=name,visible=true},"gone before its exit played")
      test.advance(300)
      test.falsy(test.find {id=name,visible=true},"still in the layer after its linger")
      test.eq(test.logs("error"),{})
    end)
  end
end
