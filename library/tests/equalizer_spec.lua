local test=morf.test
local eq=require("lib.equalizer")
local native=morf.audio.equalizer_curve
test.it("EQ native prescription validates curves and preserves stereo",function()
  local c=native {enabled=true,compensation=true,strength=50,per_ear=true,
    left={60,60,60,60,60,60,60,60},right={0,0,0,0,0,0,0,0}}
  test.eq(c.left,{6,6,6,6,6,6,6,6}) test.eq(c.right,{0,0,0,0,0,0,0,0})
  test.eq(#c.response_left,160) test.truthy(c.preamp < -6)
  test.raises(function() native {left={1,2}} end,"eight")
  test.raises(function() native {strength=101} end,"strength")
  test.raises(function() native {per_ear=1} end,"boolean")
  c=native {enabled=false,bands={12,12,12,12,12,12,12,12}}
  test.eq(c.preamp,0) test.eq(c.left,{0,0,0,0,0,0,0,0})
end)
test.it("EQ graph uses two native chains and a non-default smart sink",function()
  local c=native {bands={3,0,0,0,0,0,0,-2}}
  local config=eq.graph(c,"test-owner")
  local args=config["context.modules"][5].args
  test.eq(#args["filter.graph"].nodes,18) test.eq(#args["filter.graph"].links,16)
  test.eq(args["filter.graph"].inputs,{"l_preamp:In","r_preamp:In"})
  test.eq(args["filter.graph"].outputs,{"l_band_8:Out","r_band_8:Out"})
  test.eq(args["capture.props"]["filter.smart"],true)
  test.eq(args["capture.props"]["priority.session"],0)
  test.eq(args["capture.props"]["morf.equalizer.owner"],"test-owner")
  test.eq(#eq.controls(c).params,36)
  test.eq(eq.classify {active_port="analog-output-headphones"},"headphones")
  test.eq(eq.classify {properties={["device.form_factor"]="speaker"}},"speakers")
  test.eq(eq.classify {name="bluez_output.fixture",description="Bluetooth speaker"},"speakers")
end)
local HOST=[[
  local calls,child,exits={},nil,0
  local owner
  local missing=false
  local version="wireplumber 0.5.14"
  morf.run=function(argv,options,callback)
    calls[#calls+1]=argv
    local result={ok=true,stdout=""}
    if argv[1]=="wireplumber" then result.stdout=version
    elseif argv[1]=="pw-dump" then
      result.stdout=morf.json.encode(missing and {} or {{id=54,info={props={
        ["node.name"]="morf.equalizer.input",["morf.equalizer.owner"]=owner}}}})
    end
    local timer=morf.timer(1,function() callback(result) end,false)
    return {kill=function() timer:cancel() end}
  end
  morf.spawn=function(options)
    local config=morf.json.decode(morf.fs.read(options.command[3]))
    owner=config["context.modules"][5].args["capture.props"]["morf.equalizer.owner"]
    calls[#calls+1]=options.command
    child={kill=function() exits=exits+1 end}
    return child
  end
  local eq=require("lib.equalizer").new {name="test.equalizer"}
  local function curve(v) return morf.audio.equalizer_curve {bands={v,0,0,0,0,0,0,0}} end
  morf.ipc.start=function() eq.start(curve(2)) end
  morf.ipc.stop=eq.stop
  morf.ipc.missing=function() missing=true end
  morf.ipc.unsupported=function() version="wireplumber 0.4.17" end
  morf.ipc.update=function(v) eq.update(curve(tonumber(v))) end
  morf.ipc.state=function() return {calls=calls,exits=exits,status=eq.status:get(),error=eq.error:get()} end
]]
test.it("EQ owns one process and coalesces live control updates",function()
  test.load {source=HOST}
  test.ipc("start") test.advance(150)
  test.eq(test.ipc("state").status,"on")
  local count=#test.ipc("state").calls
  test.ipc("update","4") test.advance(20) test.ipc("update","5") test.advance(20) test.ipc("update","6")
  test.advance(150)
  local state=test.ipc("state") test.eq(#state.calls,count+1)
  local command=state.calls[#state.calls]
  test.eq(command[1],"pw-cli") test.eq(command[2],"set-param")
  test.contains(command[5],"l_band_1:Gain")
  for _,call in ipairs(state.calls) do test.ne(call[1],"systemctl") test.ne(call[1],"wpctl") end
  test.ipc("stop") test.advance(200)
  test.eq(test.ipc("state").exits,1) test.eq(test.ipc("state").status,"off")
  count=#test.ipc("state").calls test.advance(60000) test.eq(#test.ipc("state").calls,count)
end)
test.it("EQ cancels discovery on disable and reports a missing graph",function()
  test.load {source=HOST}
  test.ipc("missing") test.ipc("start") test.advance(30) test.ipc("stop")
  test.advance(5000) test.eq(test.ipc("state").status,"off")
  test.ipc("start") test.advance(4000)
  test.eq(test.ipc("state").status,"failed") test.truthy(test.ipc("state").error~="")
  test.eq(test.ipc("state").exits,2)
end)
test.it("EQ rejects an unsupported session manager without starting a filter",function()
  test.load {source=HOST}
  test.ipc("unsupported") test.ipc("start") test.advance(200)
  local state=test.ipc("state") test.eq(state.status,"failed")
  test.eq(#state.calls,1) test.eq(state.exits,0) test.contains(state.error,"0.5")
end)
