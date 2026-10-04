local test=morf.test
test.it("keyring client receives the answer entered in morf",function()
  test.load("../../examples/shells/caelestia/shell/init.lua",{
    env={CAELESTIA_STYLE="material",CAELESTIA_DRY_RUN="0",MORF_KEYRING_HELPER=morf.env("MORF_KEYRING_HELPER"),
      DBUS_SESSION_BUS_ADDRESS=morf.env("KEYRING_TEST_BUS")},source=[[
      require("init")
      local keyring=require("keyring")
      morf.ipc.status=function() return {agent=keyring.registered:get(),open=keyring.drawer.open:get()} end
    ]]})
  local ok,why=pcall(function() test.wait(function() return test.ipc("status").agent end,5000) end)
  if not ok then test.note(morf.json.encode(test.logs("info"))) error(why) end
  test.wait(function() return test.ipc("status").open end,5000)
  test.advance(1600)
  test.type("Disposable-only-bridge-test")
  test.key("Return")
  test.wait(function() return not test.ipc("status").open end,5000)
  test.eq(#test.logs("error"),0)
end)
