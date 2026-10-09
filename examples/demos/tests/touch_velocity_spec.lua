local test=morf.test
local SOURCE=[[
  local ui=require("morf.ui")
  local last={}
  ui.Item {width=400,height=600,ui.MouseArea {width=400,height=600,on_panned=function(phase,dx,dy,vx,vy)
    last={phase=phase,dx=dx,dy=dy,vx=vx,vy=vy}
    return true
  end}}
  morf.ipc.state=function() return last end
]]
local function load() test.load {source=SOURCE,size={400,600}} end
local function sample(phase,x,time)
  test.touch(phase,0,x,200,{time_ms=time%4294967296})
end

test.it("batched touch input keeps the device speed despite delayed frame delivery",function()
  for _,delay in ipairs {0,150} do
    load()
    sample("down",20,1000)
    for i=1,5 do
      test.advance(delay)
      sample("move",20+i*16,1000+i*16)
    end
    test.advance(delay)
    sample("up",100,1090)
    local s=test.ipc("state")
    test.eq(s.phase,"end") test.near(s.vx,1000,.01) test.near(s.vy,0,.01)
    test.near(s.dx,80,.01)
  end
end)

test.it("release after holding the finger still does not fling",function()
  load()
  sample("down",20,1000)
  sample("move",40,1020) sample("move",60,1040)
  sample("up",60,1100)
  test.near(test.ipc("state").vx,0,.01)
end)

test.it("touch velocity survives the Wayland timestamp wrapping",function()
  load()
  local start=4294967280
  sample("down",20,start)
  for i=1,5 do sample("move",20+i*16,start+i*16) end
  sample("up",100,start+90)
  test.near(test.ipc("state").vx,1000,.01)
  test.eq(test.logs("error"),{})
end)

test.it("release coordinates participate in the gesture before it ends",function()
  load()
  sample("down",20,1000)
  sample("move",40,1020) sample("move",60,1040)
  sample("up",80,1060)
  local s=test.ipc("state")
  test.near(s.dx,60,.01) test.near(s.vx,1000,.01)
end)
