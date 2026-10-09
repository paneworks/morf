local test=morf.test
local SOURCE=[[
  local ui=require("morf.ui")
  morf.surface.width,morf.surface.height=400,600
  local phases,edges,clicks,drags={}, {},0,0
  local panel
  panel=ui.Item {id="panel",width=400,height=600,
    on_panned=function(phase,dx,dy,vx,vy)
      phases[#phases+1]={phase=phase,dx=dx,dy=dy,vx=vx,vy=vy}
      if phase=="update" then panel.translate_y=dy end
    end,
    on_edge_panned=function(edge,phase,dx,dy)
      if edge~="bottom" then return false end
      edges[#edges+1]={edge=edge,phase=phase,dx=dx,dy=dy}
    end,
    ui.MouseArea {width=400,height=200,on_clicked=function() clicks=clicks+1 end},
    ui.MouseArea {y=200,width=400,height=200,on_dragged=function() drags=drags+1 end},
    ui.MouseArea {y=400,width=400,height=200},
  }
  morf.ipc.state=function() return {phases=phases,edges=edges,clicks=clicks,drags=drags,y=panel.translate_y} end
]]
local function load() test.load("counter.lua",{source=SOURCE,size={400,600}}) end
local function state() return test.ipc("state") end
test.it("an owned pan tracks surface displacement through hold, reversal and release",function()
  load()
  test.touch("down",0,100,100)
  test.advance(40) test.touch("move",0,100,170)
  test.eq(state().y,70)
  test.advance(1000) test.eq(state().y,70)
  test.touch("move",0,100,140)
  test.eq(state().y,40)
  test.advance(500) test.touch("up",0,100,140)
  local s=state()
  test.eq(s.phases[1].phase,"begin")
  test.eq(s.phases[#s.phases].phase,"end")
  test.eq(s.phases[#s.phases].vy,0)
  test.eq(s.clicks,0)
end)
test.it("a tap remains a click and a child slider retains its drag",function()
  load()
  test.touch("down",0,100,100) test.touch("up",0,100,100)
  test.eq(state().clicks,1) test.eq(#state().phases,0)
  test.swipe({100,300},{200,300})
  test.truthy(state().drags>0) test.eq(#state().phases,0)
end)
test.it("edge pan continues past its strip and cancellation never commits",function()
  load()
  test.touch("down",0,200,596)
  test.advance(30) test.touch("move",0,200,430)
  test.advance(500) test.touch("move",0,200,510)
  test.touch("cancel",0,200,510)
  local s=state()
  test.eq(s.edges[1].phase,"begin") test.eq(s.edges[#s.edges].phase,"cancel")
  test.eq(s.edges[#s.edges].dy,-86) test.eq(#s.phases,0)
end)
test.it("a second finger cancels an owned pan without a release action",function()
  load()
  test.touch("down",0,100,100) test.touch("move",0,100,170)
  test.touch("down",1,250,150)
  test.touch("up",0,100,170) test.touch("up",1,250,150)
  local s=state()
  test.eq(s.phases[#s.phases].phase,"cancel") test.eq(s.clicks,0)
end)

local SCROLL=[[
  local ui=require("morf.ui")
  morf.surface.width,morf.surface.height=400,600
  local phases={}
  local flick=ui.Flickable {id="scroll",width=400,height=500,content_y=100,
    ui.MouseArea {width=400,height=1000},
  }
  ui.Item {width=400,height=600,
    on_panned=function(phase) phases[#phases+1]=phase end,flick,
  }
  morf.ipc.state=function() return {offset=flick.content_y,phases=phases} end
  morf.ipc.start=function() flick.content_y=0 end
]]
test.it("scrollable content keeps vertical motion and yields at its boundary",function()
  test.load("counter.lua",{source=SCROLL,size={400,600}})
  test.touch("down",0,200,200) test.touch("move",0,200,250)
  test.eq(state().offset,50) test.eq(#state().phases,0)
  test.touch("up",0,200,250)
  test.ipc("start")
  test.touch("down",0,200,200) test.touch("move",0,200,250)
  test.eq(state().offset,0) test.eq(state().phases,{"begin","update"})
  test.touch("up",0,200,250)
  test.eq(state().phases,{"begin","update","end"})
end)
test.it("a vertical scroller yields a horizontal pan to its container",function()
  test.load("counter.lua",{source=SCROLL,size={400,600}})
  test.touch("down",0,200,200) test.touch("move",0,100,200)
  test.eq(state().offset,100) test.eq(state().phases,{"begin","update"})
  test.touch("up",0,100,200)
end)
