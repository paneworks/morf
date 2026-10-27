local test=morf.test
local HOST=[[
  local db=require("lib.services.dbus")
  local NM="org.freedesktop.NetworkManager"
  local ROOT="/org/freedesktop/NetworkManager"
  local MM="org.freedesktop.ModemManager1"
  local MR="/org/freedesktop/ModemManager1"
  local MP=MR.."/Modem/0"
  local DEV=ROOT.."/Devices/1"
  local SP=ROOT.."/Settings/1"
  local owned=true
  local saved,device=false,true
  local calls,watches,properties={},{},{}
  local modem_props={State=11,Sim=MR.."/SIM/0",SignalQuality={72,true},AccessTechnologies=1<<14}
  local gpp={OperatorName="Test provider",RegistrationState=5}
  local client={}
  client.has_owner=function() return owned end
  client.call1=function(name,path,interface,method)
    if method=="GetManagedObjects" then
      if morf.env("MOBILE_LEGACY")=="1" then return nil,"D-Bus dictionary keys must be strings" end
      return {[MP]={[MM..".Modem"]=modem_props,[MM..".Modem.Modem3gpp"]=gpp}}
    end
    if method=="Introspect" then return device and '<node><node name="0"/></node>' or '<node/>' end
    if method=="ListConnections" then return saved and {SP} or {} end
    if method=="GetSettings" then return {connection={id="Mobile data",type="gsm",uuid="mobile"},
      gsm={apn="internet.example",["auto-config"]=true}} end
  end
  client.managed_objects=function()
    return {[ROOT]={[NM]={AllDevices=device and {DEV} or {},ActiveConnections={}}},
      [DEV]={[NM..".Device"]={Interface="wwan0",DeviceType=8,Managed=true,State=30}}}
  end
  client.get_all=function() return {WwanEnabled=true} end
  client.get=function(_,_,interface,property)
    return (interface==MM..".Modem" and modem_props or gpp)[property]
  end
  client.watch_name=function(name,fn) watches[name]=fn end
  client.on_signal=function() end
  client.on_properties=function(_,path,fn) properties[path]=fn return {close=function() end} end
  client.call_async=function(_,_,_,method,args,_,done)
    calls[#calls+1]={method=method,args=args}
    if method=="AddAndActivateConnection" then done({SP,ROOT.."/ActiveConnection/1"})
    else done({ROOT.."/ActiveConnection/1"}) end
    return true
  end
  db.new=function() return client end
  local net=require("lib.services.networkmanager").connect()
  local answer
  morf.ipc.create=function()
    local ok,err=net.connect_mobile(function(value) answer=value end)
    return {ok=ok==true,error=err or "",answer=answer or "",calls=calls}
  end
  morf.ipc.change=function(what)
    if what=="saved" then saved=true net.refresh()
    elseif what=="no-device" then device=false net.refresh()
    elseif what=="device-back" then device=true net.refresh() end
  end
  morf.ipc.read=function()
    return {profile=net.state.known_connections:get(1)}
  end
]]
test.it("mobile setup types its D-Bus arguments and reuses an existing GSM profile",function()
  test.load("mobile-service-fixture.lua",{source=HOST,size={20,20}})
  local result=test.ipc("create")
  test.truthy(result.ok)
  local call=result.calls[1]
  test.eq(call.method,"AddAndActivateConnection")
  test.eq(call.args[1].signature,"a{sa{sv}}")
  test.eq(call.args[1].value.gsm["auto-config"],true)
  test.eq(call.args[1].value.connection.type,"gsm")
  test.eq(call.args[2].signature,"o")
  test.eq(call.args[2].value,"/org/freedesktop/NetworkManager/Devices/1")
  test.eq(result.answer,"/org/freedesktop/NetworkManager/ActiveConnection/1")
  test.ipc("change","saved")
  test.eq(test.ipc("read").profile.apn,"internet.example")
  test.truthy(test.ipc("read").profile.auto_apn)
  result=test.ipc("create")
  test.eq(result.calls[2].method,"ActivateConnection")
  test.eq(result.calls[2].args[1].value,"/org/freedesktop/NetworkManager/Settings/1")
end)
test.it("mobile setup refuses absent hardware",function()
  test.load("mobile-service-fixture.lua",{source=HOST,size={20,20}})
  test.ipc("change","no-device")
  local result=test.ipc("create")
  test.falsy(result.ok) test.eq(result.error,"no managed modem") test.eq(#result.calls,0)
end)
