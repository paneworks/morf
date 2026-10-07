local test = morf.test
local HOST = [[
  local mobile = {state=morf.state {
    available=false, data=true, locked=false, registered=true,
    signal=72, technology="LTE", operator="Test network"
  }}
  local calls = 0
  mobile.set_data = function(on) calls=calls+1 mobile.state.data=on end
  package.loaded["lib.services.modem"] = {connect=function() return mobile end}
  local services = require("services")
  local model = require("settings_model")
  model.opened:set(true)
  local toggle
  for _, item in ipairs(model.TOGGLES) do
    if item.id == "mobile" then toggle=item end
  end
  local bar = require("bar_model").new {
    on=function() return true end, vertical=function() return false end
  }
  local status = morf.signal("test.mobile.status", "")
  morf.effect("test.mobile.binding",function() status:set(toggle.status()) end)
  morf.ipc.available = function(value) mobile.state.available=value end
  morf.ipc.locked = function(value) mobile.state.locked=value end
  morf.ipc.disable = function() toggle.set(false) end
  morf.ipc.read = function()
    return {observed=services.modem==mobile, status=status:get(), on=toggle.on(), icon=toggle.icon(),
      technology=bar.reading:get().technology, bars=bar.reading:get().mobile_icon,
      calls=calls}
  end
]]

test.it("mobile settings and bar follow late hardware and removal without reloading", function()
  test.load("../shell/init.lua", {source=HOST,size={320,240},env={
    CAELESTIA_DRY_RUN="0", CAELESTIA_FAKE_MODEM="0"
  }})
  test.advance(50)
  test.truthy(test.ipc("read").observed)
  test.eq(test.ipc("read").status,"No modem")
  test.falsy(test.ipc("read").on)
  test.eq(test.ipc("read").technology,"")
  test.ipc("disable")
  test.eq(test.ipc("read").calls,0)

  test.ipc("available",true) test.advance(50)
  test.eq(test.ipc("read").status,"LTE · Test network")
  test.truthy(test.ipc("read").on)
  test.eq(test.ipc("read").technology,"LTE")
  test.eq(test.ipc("read").bars,"signal_cellular_3_bar")
  test.ipc("locked",true) test.advance(50)
  test.eq(test.ipc("read").status,"SIM locked")
  test.ipc("locked",false)

  test.ipc("available",false) test.advance(50)
  test.eq(test.ipc("read").status,"No modem")
  test.eq(test.ipc("read").technology,"")
  test.eq(test.ipc("read").bars,"signal_cellular_nodata")
  test.ipc("available",true) test.ipc("disable") test.advance(50)
  test.eq(test.ipc("read").calls,1)
  test.eq(test.ipc("read").status,"Off")
  test.eq(#test.logs("error"),0)
end)
