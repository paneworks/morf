-- The service registry's generic binder (PLAN item 7), against the
-- same fake-client seam the other service specs use.
--
--     morf test library/tests/registry_spec.lua

local test = morf.test

local HOST = [[
  local db = require("lib.services.dbus_client")
  local owned, props = true, { OnBattery = true, Percentage = 42 }
  local display = { Percentage = 42, State = 1 }
  local changes, watches = {}, {}
  local live = 0
  local client = {}
  client.has_owner = function() return owned end
  client.get_all = function(_, path)
    if path == "/org/example/Demo/Display" then return display end
    return props
  end
  local function handle()
    live = live + 1
    return { close = function() live = live - 1 end }
  end
  client.watch_name = function(_, fn) watches[#watches + 1] = fn return handle() end
  client.on_properties = function(_, path, fn)
    changes[path] = fn
    return handle()
  end
  client.call1 = function() return "ok" end
  db.new = function() return client end

  local registry = require("lib.services.registry")
  local demo = registry.define {
    name = "demo", bus = "system",
    service = "org.example.Demo", path = "/org/example/Demo",
    interface = "org.example.Demo",
    properties = {
      on_battery = "OnBattery",
      percentage = { prop = "Percentage", default = 0 },
    },
    groups = {
      display = {
        path = "/org/example/Demo/Display", interface = "org.example.Demo.Device",
        properties = {
          percentage = { prop = "Percentage", default = 0 },
          charging = { prop = "State", map = function(v) return v == 1 end },
        },
      },
    },
    methods = function(client, svc)
      return { ping = function() return client.call1() end }
    end,
  }
  local bound = demo.connect({ debounce_ms = 1 })
  morf.ipc.read = function()
    return { available = bound.state.available, on_battery = bound.state.on_battery,
             percentage = bound.state.percentage,
             display_percentage = bound.state.display.percentage,
             display_charging = bound.state.display.charging,
             display_available = bound.state.display.available }
  end
  morf.ipc.ping = function() return bound.ping() end
  morf.ipc.change = function()
    props = { OnBattery = false, Percentage = 7 }
    changes["/org/example/Demo"]()
  end
  morf.ipc.vanish = function() owned = false watches[1]() end
  morf.ipc.close = function() bound.close() end
  morf.ipc.live = function() return live end
  morf.ipc.cycle = function()
    bound.close()
    bound = registry.connect("demo", { debounce_ms = 1 })
    return live
  end
  morf.ipc.unknown = function()
    local ok, err = pcall(registry.connect, "demoo")
    return ok and "connected" or tostring(err)
  end
]]

test.describe("service registry", function()
  test.it("binds properties, follows changes, and closes", function()
    test.load("registry-fixture.lua", { source = HOST, size = { 20, 20 } })
    test.settle()
    local first = test.ipc("read")
    test.eq(first.available, true)
    test.eq(first.on_battery, true)
    test.eq(first.percentage, 42)
    test.eq(first.display_available, true)
    test.eq(first.display_percentage, 42)
    test.eq(first.display_charging, true)
    test.eq(test.ipc("ping"), "ok")

    test.ipc("change")
    test.settle()
    local second = test.ipc("read")
    test.eq(second.on_battery, false)
    test.eq(second.percentage, 7)

    test.ipc("vanish")
    test.settle()
    test.eq(test.ipc("read").available, false)
    test.ipc("close")
  end)

  test.it("connects by name, refuses unknown names, and leaks no watches", function()
    test.load("registry-fixture.lua", { source = HOST, size = { 20, 20 } })
    test.settle()
    local before = test.ipc("live")
    test.eq(test.ipc("cycle"), before, "close and reconnect returns to the same watch count")
    local refusal = test.ipc("unknown")
    test.eq(refusal:find("unknown service `demoo`", 1, true) ~= nil, true, refusal)
    test.eq(refusal:find("demo", 1, true) ~= nil, true, refusal)
    test.ipc("close")
    test.eq(test.ipc("live"), 0)
  end)
end)
