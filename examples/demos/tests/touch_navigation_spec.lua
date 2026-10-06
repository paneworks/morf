local test = morf.test
local SOURCE = [[
  local ui = require("morf.ui")
  local swipes, clicks, drags = {}, 0, 0
  morf.surface.width, morf.surface.height = 400, 600
  morf.ipc.state = function() return {swipes=swipes, clicks=clicks, drags=drags} end
  ui.Item { width=400, height=600, on_swiped=function(direction) swipes[#swipes+1]=direction end,
    ui.MouseArea { id="passive", width=400, height=200, on_clicked=function() clicks=clicks+1 end },
    ui.MouseArea { id="slider", y=200, width=400, height=200, on_dragged=function() drags=drags+1 end },
    ui.MouseArea { id="card", y=400, width=400, height=200,
      on_swiped=function(direction) swipes[#swipes+1]="card:"..direction end },
  }
]]
local function load() test.load("counter.lua", {source=SOURCE,size={400,600}}) end
local function state() return test.ipc("state") end

test.it("a finger swiping passive panel content reaches its container without a click", function()
  load()
  test.swipe({320,100}, {100,100})
  test.eq(state().swipes, {"left"}) test.eq(state().clicks, 0)
  test.touch("down",0,100,100) test.touch("up",0,100,100)
  test.eq(state().clicks, 1)
end)
test.it("a child drag or swipe keeps ownership", function()
  load()
  test.swipe({320,300}, {100,300})
  test.eq(state().swipes, {}) test.truthy(state().drags>0)
  test.swipe({320,500}, {100,500})
  test.eq(state().swipes, {"card:left"})
end)
test.it("a canceled touch does not navigate", function()
  load()
  test.touch("down",0,320,100)
  test.advance(30) test.touch("move",0,200,100)
  test.touch("cancel",0,200,100)
  test.eq(state().swipes,{})
end)

test.it("a deliberate slow touch pull navigates, while a short movement does not", function()
  load()
  test.swipe({320,100}, {100,100}, {duration=900})
  test.eq(state().swipes, {"left"})
  test.swipe({100,100}, {115,100}, {duration=900})
  test.eq(state().swipes, {"left"})
end)
