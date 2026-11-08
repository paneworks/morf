-- A finger a touchscreen reports as two contacts: a short one, lifted, and a
-- new one landing a few milliseconds later a little further on. Under a pan
-- the tap waits a moment before it clicks; a landing that then moves is the
-- same finger going on, while separate taps, double taps and taps where no
-- pan reaches stay what they were.
--
--     morf test examples/demos/tests/split_touch_spec.lua
local test=morf.test
local SOURCE=[[
  local ui=require("morf.ui")
  morf.surface.width,morf.surface.height=400,600
  local log,raw={},{}
  local function note(text) log[#log+1]=text end
  local function tile(name,x,y,extra)
    local props={x=x,y=y,width=200,height=200,
      on_pressed=function() note("press:"..name) end,
      on_clicked=function() note("click:"..name) end,
      on_double_clicked=function() note("double:"..name) end,
      on_touch_pressed=function(id) raw[#raw+1]="tp:"..id end,
      on_touch_released=function(id) raw[#raw+1]="tr:"..id end,
      on_touch_canceled=function(id) raw[#raw+1]="tc:"..id end}
    for k,v in pairs(extra or {}) do props[k]=v end
    return ui.MouseArea(props)
  end
  ui.Item {width=400,height=600,
    on_edge_panned=function(edge) return edge=="bottom" end,
    ui.Item {id="panel",width=400,height=400,
      on_panned=function(phase,dx)
        if phase=="begin" then note("pan:begin") end
        if phase=="end" then note("pan:end:"..math.floor(dx+0.5)) end
      end,
      tile("a",0,0),tile("b",200,0),tile("c",0,200,{on_dragged=function() end}),
    },
    tile("d",0,400),
  }
  morf.ipc.log=function() local s=table.concat(log,","); log={}; return s end
  morf.ipc.raw=function() local s=table.concat(raw,","); raw={}; return s end
]]
local function load() test.load {source=SOURCE,size={400,600}} test.advance(50) end
local function log() return test.ipc("log") end
local function tap(x,y,id) test.touch("down",id or 0,x,y) test.advance(40) test.touch("up",id or 0,x,y) end

test.it("a finger reported as two contacts pans from where it first landed and clicks nothing",function()
  load()
  test.touch("down",0,50,100) test.advance(8)
  test.touch("move",0,56,100) test.advance(8)
  test.touch("up",0,56,100) test.advance(8)
  test.touch("down",1,74,100) test.advance(8)
  for i=1,10 do test.touch("move",1,74+18*i,100) test.advance(8) end
  test.touch("up",1,254,100) test.advance(300)
  test.eq(log(),"press:a,pan:begin,pan:end:204")
  test.eq(test.ipc("raw"),"tp:0,tr:0,tp:1,tc:1")
end)

test.it("a tap under a pan clicks once its finger has not landed again for the gap",function()
  load()
  tap(50,100)
  test.eq(log(),"press:a")
  test.advance(30)
  test.eq(log(),"","the click came before the gap was out")
  test.advance(60)
  test.eq(log(),"click:a")
end)

test.it("a tap the touchscreen cancels in its gap clicks nothing",function()
  load()
  tap(50,100) test.advance(20)
  test.touch("cancel",0,50,100) test.advance(200)
  test.eq(log(),"press:a")
  test.eq(test.ipc("raw"),"tp:0,tr:0")
end)

test.it("a quick double tap on one spot is still a double click",function()
  load()
  tap(50,100) test.advance(30) tap(50,100) test.advance(300)
  test.eq(log(),"press:a,click:a,press:a,click:a,double:a")
  tap(50,100) test.advance(150) tap(52,100) test.advance(300)
  test.eq(log(),"press:a,click:a,press:a,click:a,double:a")
end)

test.it("two quick taps on two tiles are two clicks, and so is a landing that rests",function()
  load()
  tap(50,100) test.advance(20) tap(250,100) test.advance(300)
  test.eq(log(),"press:a,click:a,press:b,click:b")
  tap(50,100) test.advance(20)
  test.touch("down",1,250,100) test.advance(200)
  test.eq(log(),"press:a,click:a,press:b")
  test.touch("up",1,250,100) test.advance(300)
  test.eq(log(),"click:b")
end)

test.it("a tap where no pan reaches, or on what keeps its own drags, clicks at once",function()
  load()
  tap(50,450)
  test.eq(log(),"press:d,click:d")
  tap(50,300)
  test.eq(log(),"press:c,click:c")
  test.click(50,100)
  test.eq(log(),"press:a,click:a")
end)

test.it("an edge pull holds only the taps that land by its edge",function()
  load()
  tap(50,590)
  test.eq(log(),"press:d")
  test.advance(80)
  test.eq(log(),"click:d")
end)
