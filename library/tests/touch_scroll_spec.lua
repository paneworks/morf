local test=morf.test
local SOURCE=[[
  local ui=require("morf.ui")
  local skin=require("lib.kit.skin")
  local w=require("lib.kit.widgets")
  skin.define("plain",{skins={
    Press=function() return {background=ui.Rect{anchors={fill=true},color="#444444"}} end,
    Range=function() return {track=ui.Item{anchors={fill=true}}} end,
  }})
  skin.use("plain")
  local clicks,pages,value=0,0,0
  local root,flick=w.scroll_view {id="page",x=20,y=20,width=320,height=300,clip=true,
    ui.Item {width=320,height=1000,
      w.push {id="button",x=30,y=120,width=150,height=40,on_clicked=function() clicks=clicks+1 end},
      w.slider {id="slider",x=30,y=200,width=200,height=30,from=0,to=1,
        on_moved=function(v) value=v end},
    },
  }
  ui.Item {width=400,height=400,root,
    on_panned=function(phase,dx,dy)
      if phase=="begin" then return math.abs(dx)>math.abs(dy) end
      if phase=="end" then pages=pages+1 end
    end,
  }
  morf.ipc.state=function() return {offset=flick.content_y,clicks=clicks,pages=pages,value=value} end
]]
local function load()
  test.load {source=SOURCE,size={400,400}} test.advance(100)
end
local function drag(x,y,dx,dy)
  test.touch("down",0,x,y)
  for i=1,10 do test.advance(20) test.touch("move",0,x+dx*i/10,y+dy*i/10) end
  test.touch("up",0,x+dx,y+dy) test.advance(100)
end
test.it("touch scrolls blank content repeatedly and clamps at both ends",function()
  load()
  drag(25,270,0,-200) test.near(test.ipc("state").offset,200,1)
  drag(25,270,0,-200) test.near(test.ipc("state").offset,400,1)
  drag(25,270,0,-900) test.near(test.ipc("state").offset,700,1)
  drag(25,40,0,900) test.near(test.ipc("state").offset,0,1)
end)
test.it("dragging from a button scrolls without clicking but an ordinary tap clicks",function()
  load()
  test.touch("down",0,100,160) test.touch("up",0,100,160)
  test.eq(test.ipc("state").clicks,1)
  drag(100,160,0,-100)
  test.near(test.ipc("state").offset,100,1) test.eq(test.ipc("state").clicks,1)
end)
test.it("a horizontal page swipe passes through while slider drags keep their input",function()
  load()
  drag(50,60,180,0)
  test.eq(test.ipc("state").pages,1) test.eq(test.ipc("state").offset,0)
  drag(70,230,120,0)
  test.eq(test.ipc("state").pages,1) test.truthy(test.ipc("state").value>.5)
  test.eq(test.logs("error"),{})
end)
