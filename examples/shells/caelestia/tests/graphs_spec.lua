-- Both skins use the same native graph calculations with their existing nodes.
local test=morf.test
local HOST=[[
  local ui=require("morf.ui")
  local data=morf.signal("graph-reference",{10,15,20})
  local specs={}
  local path=ui.Path
  ui.Path=function(props)
    specs[#specs+1]=props return path(props)
  end
  local graphs=require("themes.layouts.views.graphs").new(5)
  local graph=graphs.graph {id="native-graph",width=100,height=40,bottom=10,top=20,
    columns=2,rows=2,color=function() return morf.color("#65b8b4") end,
    first=function() return data:get() end}
  ui.Path=path
  ui.Item {width=120,height=60,graph}
  -- A series path draws its channel natively: the outline is what the
  -- engine's plot makes of the samples with the path's own plot settings.
  local function outline(props)
    local plot=props.plot() plot.width=props.width plot.height=props.height
    local d=morf.geometry.plot(data:get(),plot)
    d=d:gsub("[%d%.%-]+",function(n) return ("%.1f"):format(tonumber(n)) end)
    return (d:gsub("%s+"," "):gsub("%s+$",""))
  end
  morf.ipc.paths=function()
    return {grid=specs[1].d,fill=outline(specs[2]),line=outline(specs[3]),
      same=specs[2].series~=nil and specs[2].series==specs[3].series}
  end
  morf.ipc.sample=function() data:set({10,10,15,20,10}) end
]]
for _,style in ipairs {"material","tsugumori"} do
  test.it(style.." graphs preserve native line, fill, grid and sample positions",function()
    test.load("../shell/init.lua",{source=HOST,size={120,60},env={CAELESTIA_STYLE=style,CAELESTIA_DRY_RUN="1"}})
    test.advance(1)
    local paths=test.ipc("paths")
    test.truthy(paths.same,"line and fill do not draw one channel")
    test.eq(paths.grid,"M50.0 0 V40.0 M0 20.0 H100.0")
    test.eq(paths.line,"M50.0 39.0 L75.0 20.0 L100.0 1.0")
    test.eq(paths.fill,paths.line.." L100.0 40.0 L50.0 40.0 Z")
    test.ipc("sample") test.advance(1)
    test.eq(test.ipc("paths").line,"M0.0 39.0 L25.0 39.0 L50.0 20.0 L75.0 1.0 L100.0 39.0")
    test.eq(#test.logs("error"),0)
  end)
end
