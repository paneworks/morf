-- Managed PipeWire smart filter. One owner per shell; no audio-service
-- restarts, default-device changes, system config fragments, or idle polling.
-- DSP runs in PipeWire; prescription/response calculations run in Rust.
local morf=require("morf")
local M={frequencies={250,500,1000,2000,3000,4000,6000,8000}}
M.INPUT="morf.equalizer.input"
M.OUTPUT="morf.equalizer.output"
M.SMART="morf.equalizer"
function M.controls(curve)
  local params={}
  for _,ear in ipairs {"l","r"} do
    params[#params+1]=ear.."_preamp:Mult" params[#params+1]=10^(curve.preamp/20)
    for i,gain in ipairs(curve[ear=="l" and "left" or "right"]) do
      params[#params+1]=ear.."_band_"..i..":Gain" params[#params+1]=gain
    end
  end
  return {params=params}
end
function M.graph(curve,owner)
  local nodes,links={},{ }
  for _,ear in ipairs {"l","r"} do
    nodes[#nodes+1]={type="builtin",name=ear.."_preamp",label="linear",control={Mult=10^(curve.preamp/20),Add=0}}
    for i,hz in ipairs(M.frequencies) do
      nodes[#nodes+1]={type="builtin",name=ear.."_band_"..i,
        label=i==1 and "bq_lowshelf" or i==8 and "bq_highshelf" or "bq_peaking",
        control={Freq=hz,Q=1,Gain=curve[ear=="l" and "left" or "right"][i]}}
      links[#links+1]={output=ear..(i==1 and "_preamp" or "_band_"..(i-1))..":Out",input=ear.."_band_"..i..":In"}
    end
  end
  return {
    ["context.properties"]={["log.level"]=0},
    ["context.spa-libs"]={["audio.convert.*"]="audioconvert/libspa-audioconvert",["support.*"]="support/libspa-support"},
    ["context.modules"]={
      {name="libpipewire-module-rt",flags={"ifexists","nofail"}},
      {name="libpipewire-module-protocol-native"},
      {name="libpipewire-module-client-node"},
      {name="libpipewire-module-adapter"},
      {name="libpipewire-module-filter-chain",args={
        ["node.description"]="Morf Equalizer",["media.name"]="Morf Equalizer",
        ["filter.graph"]={nodes=nodes,links=links,inputs={"l_preamp:In","r_preamp:In"},outputs={"l_band_8:Out","r_band_8:Out"}},
        ["audio.channels"]=2,["audio.position"]={"FL","FR"},["audio.rate"]=48000,
        ["capture.props"]={["node.name"]=M.INPUT,["media.class"]="Audio/Sink",
          ["filter.smart"]=true,["filter.smart.name"]=M.SMART,["node.virtual"]=true,
          ["morf.equalizer.owner"]=owner,["priority.session"]=0},
        ["playback.props"]={["node.name"]=M.OUTPUT,["node.passive"]=true,["media.role"]="DSP",
          ["application.id"]=M.SMART,["application.name"]="Morf Equalizer",["media.name"]="Morf Equalizer"},
      }},
    },
  }
end
function M.classify(sink)
  if not sink then return "speakers" end
  local props=sink.properties or {}
  if props["device.form_factor"]=="speaker" or props["device.form_factor"]=="hifi" then return "speakers" end
  local text=table.concat({sink.name or "",sink.description or "",sink.active_port or "",
    sink.icon_name or "",props["device.form_factor"] or "",props["device.icon_name"] or ""}," "):lower()
  for _,term in ipairs {"headphone","headset","earbud","earphone","airpod","a2dp","bluez"} do
    -- Explicit speaker descriptions override Bluetooth's generic profile.
    if term=="a2dp" or term=="bluez" then
      if text:find("speaker",1,true) or text:find("soundbar",1,true) then return "speakers" end
    end
    if text:find(term,1,true) then return "headphones" end
  end
  return "speakers"
end
function M.new(options)
  local e={status=morf.signal(options.name..".status","off"),error=morf.signal(options.name..".error","")}
  local process,pending,query,command,node,token=nil,nil,nil,nil,nil,0
  local running=false
  local latest,attempts
  local owner=tostring(morf.process_id)..":"..options.name
  local path=options.path or morf.state_path("equalizer-runtime.conf")
  local function cancel()
    token=token+1
    if pending then pending:cancel() pending=nil end
    if query then query:kill() query=nil end
    if command then command:kill() command=nil end
  end
  local function fail(message)
    running=false cancel() node=nil
    if process then process:kill() process=nil end
    e.error:set(tostring(message):gsub("%s+"," "):sub(1,240)) e.status:set("failed")
  end
  local function discover()
    if not running or query then return end
    local generation=token
    query=morf.run({"pw-dump"},{timeout_ms=2000,max_output=4*1024*1024},function(result)
      if generation~=token or not running then return end
      query=nil
      local ok,objects=pcall(morf.json.decode,result.stdout or "")
      if result.ok and not result.truncated and ok and type(objects)=="table" then
        for _,object in ipairs(objects) do
          local props=object.info and object.info.props or {}
          if props["node.name"]==M.INPUT and props["morf.equalizer.owner"]==owner then
            node=object.id break
          end
        end
      end
      if node then e.status:set("on") e.error:set("") e.update(latest)
      else
        attempts=attempts+1
        if attempts>=15 then fail("PipeWire did not create the equalizer. Check WirePlumber and filter-chain support.")
        else pending=morf.timer(200,function() pending=nil discover() end,false) end
      end
    end)
  end
  function e.stop()
    running=false cancel() node=nil
    if process then process:kill() process=nil end
    e.status:set("off") e.error:set("")
  end
  local apply
  apply=function()
    if not running or not node or command then return end
    local generation,revision=token,latest
    command=morf.run({"pw-cli","set-param",tostring(node),"Props",morf.json.encode(M.controls(revision))},
      {timeout_ms=2000,max_output=65536},function(result)
        if generation~=token or not running then return end
        command=nil
        if not result.ok then fail("Could not update the equalizer: "..tostring(result.error or result.stderr or result.code))
        elseif revision~=latest then e.update(latest) end
      end)
  end
  function e.update(curve)
    latest=curve
    if not running or not node then return end
    if pending then pending:cancel() end
    pending=morf.timer(90,function() pending=nil apply() end,false)
  end
  function e.start(curve)
    latest=curve
    if running then e.update(curve) return end
    running=true token=token+1 attempts=0 e.status:set("starting") e.error:set("")
    local generation=token
    -- Smart filters are a WirePlumber policy. Refuse unsupported setups
    -- rather than presenting a non-functional virtual output as enabled.
    query=morf.run({"wireplumber","--version"},{timeout_ms=2000,max_output=4096},function(result)
      if generation~=token or not running then return end
      query=nil
      local major,minor=(result.stdout or ""):match("(%d+)%.(%d+)")
      if not result.ok or not major or tonumber(major)==0 and tonumber(minor)<5 then
        fail("Equalizer requires PipeWire and WirePlumber 0.5 or newer.") return
      end
      local ok,why=morf.fs.write(path,morf.json.encode(M.graph(latest,owner),true))
      if not ok then fail("Cannot write equalizer configuration: "..tostring(why)) return end
      process,why=morf.spawn {command={"pipewire","-c",path},on_stderr=function() end,
        on_exit=function()
          if generation~=token or not running then return end
          process=nil fail("Equalizer process stopped. Turn it off and on to retry.")
        end}
      if not process then fail("Cannot start PipeWire equalizer: "..tostring(why)) return end
      discover()
    end)
  end
  return e
end
return M
