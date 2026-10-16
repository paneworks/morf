-- The service registry: descriptors as data, the generic binder, the
-- refusal of an unknown name, and teardown that leaves nothing behind,
-- against the shared stand-in for `morf.dbus` (dbus_fake.lua), whose
-- tokens close only as methods, like the engine's.
--
--     morf test --no-dbus library/tests/registry_spec.lua

local test = morf.test

local HOST = [[
  local ui = require("morf.ui")
  local fake = require("tests.dbus_fake")
  local registry = require("lib.services.registry")
  local PROPS = "org.freedesktop.DBus.Properties"
  local DEMO, ROOT, IFACE, ITEM = "org.example.Demo", "/org/example/Demo", "org.example.Demo",
    "org.example.Demo.Item"
  local A, B = ROOT .. "/item/a", ROOT .. "/item/b"
  local listed = { A }
  local root = fake.object("system", DEMO, ROOT, IFACE, { OnBattery = true, Level = 50, Mode = 2 },
    { ListItems = function() return { listed } end })
  fake.object("system", DEMO, A, ITEM, { Label = "a", Percent = 140 })
  -- A second owner of the same shape, for the bindings made and let go of.
  local CYCLE = "org.example.Cycle"
  fake.object("system", CYCLE, ROOT, IFACE, { Level = 1 }, { ListItems = function() return { { A } } end })
  fake.object("system", CYCLE, A, ITEM, { Label = "a" })
  local holder = ui.Item { width = 20, height = 20 }

  registry.define {
    name = "demo", bus = "system", service = DEMO, debounce_ms = 5,
    interfaces = {
      root = { path = ROOT, interface = IFACE },
      item = { set = "items", interface = ITEM },
    },
    objects = {
      items = { list = { from = "root", method = "ListItems" }, from = "item", fields = {
        path = { meta = "key" },
        label = { prop = "Label", as = "string" },
        percent = { prop = "Percent", as = "percent" },
      } },
    },
    signals = {
      { from = "root", member = "ItemAdded", adds = "items" },
      { from = "root", member = "ItemRemoved", removes = "items" },
    },
    fields = {
      available = { from = "root", meta = "available" },
      on_battery = { from = "root", prop = "OnBattery", as = "bool" },
      level = { from = "root", prop = "Level", as = "number" },
      mode = { from = "root", prop = "Mode", as = "enum", values = { "off", "on" }, default = "unknown" },
      items = { from = "items" },
      count = { from = "items", meta = "count" },
    },
  }

  -- UPower and one MPRIS player, for binding the shipped services.
  local U, UROOT = "org.freedesktop.UPower", "/org/freedesktop/UPower"
  local DEV = "org.freedesktop.UPower.Device"
  local BAT = UROOT .. "/devices/battery_BAT0"
  fake.object("system", U, UROOT, U, { OnBattery = true }, {
    EnumerateDevices = function() return { { BAT } } end,
  })
  fake.object("system", U, UROOT .. "/devices/DisplayDevice", DEV, { Percentage = 50, State = 2 })
  fake.object("system", U, BAT, DEV, { Percentage = 50, State = 2, PowerSupply = true, IsPresent = true })
  local PLAYER_NAME, MPATH = "org.mpris.MediaPlayer2.alpha", "/org/mpris/MediaPlayer2"
  fake.object("session", PLAYER_NAME, MPATH, "org.mpris.MediaPlayer2", { Identity = "Alpha" })
  fake.object("session", PLAYER_NAME, MPATH, "org.mpris.MediaPlayer2.Player", {
    PlaybackStatus = "Playing", Position = 0, Rate = 1, Metadata = { ["xesam:title"] = "A" },
  })

  local function gets()
    local count = 0
    for _, call in ipairs(fake.calls) do
      if call.method == "GetAll" then count = count + 1 end
    end
    return count
  end
  local function owner(dest, present)
    fake.own("system", dest, present)
    fake.emit("system", "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
      "NameOwnerChanged", { dest, present and "" or ":1.7", present and ":1.8" or "" })
  end

  local upower_required = package.loaded["lib.services.upower"] ~= nil
  morf.ipc.standalone = function()
    local ok, power = pcall(registry.connect, "power", { dbus = fake.dbus })
    local out = { required_before = upower_required, ok = ok, err = ok and "" or tostring(power) }
    if ok then
      out.available, out.on_battery = power.state.available, power.state.on_battery
      power.close()
    end
    return out
  end
  morf.ipc.tokens = function()
    local raw = fake.dbus.on_name_owner_changed("system", DEMO, function() end)
    local bare = pcall(raw.close)
    raw:close()
    local client = require("lib.services.dbus_client").new({ dbus = fake.dbus })
    local watch = client.watch_name(DEMO, function() end)
    return { bare = bare, closed = watch.close(), again = watch.close(), left = fake.subscription_total() }
  end

  local demo = registry.connect("demo", { dbus = fake.dbus })
  morf.ipc.read = function()
    local s = demo.state
    local labels = {}
    for _, row in ipairs(demo.rows("items")) do labels[#labels + 1] = row.label .. ":" .. row.percent end
    return { available = s.available, on_battery = s.on_battery, level = s.level, mode = s.mode,
             count = s.count, items = table.concat(labels, ","), gets = gets() }
  end
  morf.ipc.level = function(level)
    -- Announced without changing what a re-read would answer: only a
    -- merge in place can show it.
    fake.emit("system", DEMO, ROOT, PROPS, "PropertiesChanged", { IFACE, { Level = level }, {} })
  end
  morf.ipc.invalidate = function(level)
    root.Level = level
    fake.emit("system", DEMO, ROOT, PROPS, "PropertiesChanged", { IFACE, {}, { "Level" } })
  end
  morf.ipc.add = function()
    fake.object("system", DEMO, B, ITEM, { Label = "b", Percent = 20 })
    listed[2] = B
    fake.emit("system", DEMO, ROOT, IFACE, "ItemAdded", B)
  end
  morf.ipc.remove = function()
    listed[2] = nil
    fake.emit("system", DEMO, ROOT, IFACE, "ItemRemoved", B)
  end
  morf.ipc.vanish = function() owner(DEMO, false) end
  morf.ipc.restart = function(level)
    root.Level = level
    owner(DEMO, true)
  end
  morf.ipc.subscribed = function() return fake.subscription_total() end
  morf.ipc.unknown = function(name)
    local ok, err = pcall(registry.connect, name)
    local ok_service, err_service = pcall(morf.service, name)
    return { connected = ok or ok_service, registry = tostring(err), service = tostring(err_service),
             names = table.concat(registry.names(), ", ") }
  end
  morf.ipc.malformed = function()
    local ok, err = pcall(registry.define, { name = "broken", bus = "system", service = DEMO,
      interfaces = { root = { path = ROOT, interface = IFACE } },
      fields = { level = { from = "root", prop = "Level", as = "numbr" } } })
    return ok and "defined" or tostring(err)
  end

  -- Bindings made and let go of, by owner node or by close(), each left
  -- with a re-read pending so its debounce timer is live when it goes.
  local held = {}
  morf.ipc.bind_all = function()
    for _, name in ipairs({ "demo", "power", "media" }) do
      local node = ui.Item {}
      ui.reparent(node, holder)
      local binding = assert(morf.service(name, { dbus = fake.dbus, owner = node, debounce_ms = 5,
        service = name == "demo" and CYCLE or nil }))
      held[#held + 1] = { node = node, binding = binding }
    end
    fake.emit("system", CYCLE, ROOT, PROPS, "PropertiesChanged", { IFACE, {}, { "Level" } })
    return #held
  end
  morf.ipc.close_all = function()
    for _, each in ipairs(held) do assert(each.binding.close(), "closed once") end
    for _, each in ipairs(held) do assert(each.binding.close() == false, "and only once") end
  end
  morf.ipc.destroy_all = function()
    for _, each in ipairs(held) do ui.destroy(each.node, true) end
    local closed = 0
    for _, each in ipairs(held) do if each.binding.closed() then closed = closed + 1 end end
    held = {}
    return closed
  end
]]

local function counts()
  local resources = test.resources()
  resources.subscribed = test.ipc("subscribed")
  return resources
end

local function same(actual, expected, when)
  for key, value in pairs(expected) do
    test.eq(actual[key], value, when .. ": " .. key)
  end
end

test.describe("service registry", function()
  test.before_each(function()
    test.load("registry-fixture.lua", { source = HOST, size = { 20, 20 } })
    test.settle()
  end)

  test.it("coerces a descriptor's fields and keeps a set by its signals", function()
    local read = test.ipc("read")
    test.eq(read.available, true)
    test.eq(read.on_battery, true)
    test.eq(read.level, 50)
    test.eq(read.mode, "on", "an enum, by its values")
    test.eq(read.items, "a:100", "a percent is clamped")
    test.eq(read.count, 1)

    test.ipc("add")
    test.settle()
    test.eq(test.ipc("read").items, "a:100,b:20", "an added object is read and listed")
    test.ipc("remove")
    test.settle()
    test.eq(test.ipc("read").items, "a:100", "a removed one goes")
  end)

  test.it("merges a change in place and re-reads only what was invalidated", function()
    local before = test.ipc("read").gets
    test.ipc("level", 7)
    local merged = test.ipc("read")
    test.eq(merged.level, 7, "merged at once, from the signal")
    test.eq(merged.gets, before, "without asking the bus again")
    test.ipc("invalidate", 9)
    test.eq(test.ipc("read").level, 7, "an invalidation waits for the debounce")
    test.advance(20)
    local reread = test.ipc("read")
    test.eq(reread.level, 9, "then re-reads")
    test.eq(reread.gets, before + 1, "that one object only")
  end)

  test.it("forgets what it heard when the service goes, and reads afresh when it returns", function()
    test.ipc("level", 80)
    test.eq(test.ipc("read").level, 80)
    test.ipc("vanish")
    local gone = test.ipc("read")
    test.eq(gone.available, false)
    test.eq(gone.on_battery, false, "fields fall back with the service")
    test.eq(gone.level, 0)
    test.eq(gone.count, 0, "and the set empties")
    test.ipc("restart", 30)
    local back = test.ipc("read")
    test.eq(back.available, true)
    test.eq(back.level, 30, "what the new owner says, not what the old one did")
    test.eq(back.items, "a:100")
  end)

  test.it("binds a shipped service by name before anything required its module", function()
    local bound = test.ipc("standalone")
    test.eq(bound.required_before, false)
    test.eq(bound.ok, true, bound.err)
    test.eq(bound.available, true)
    test.eq(bound.on_battery, true)
  end)

  test.it("closes a name watch the way the engine's tokens close", function()
    local before = test.ipc("subscribed")
    local tokens = test.ipc("tokens")
    test.eq(tokens.bare, false, "a token closed without itself fails, as the engine's does")
    test.eq(tokens.closed, true, "the client's handle closes either way")
    test.eq(tokens.again, false, "once")
    test.eq(tokens.left, before)
  end)

  test.it("refuses an unregistered name, listing every registered one", function()
    local refusal = test.ipc("unknown", "nonesuch")
    test.eq(refusal.connected, false)
    local expected = "unknown service `nonesuch` (registered: " .. refusal.names .. ")"
    test.eq(refusal.registry, expected)
    test.eq(refusal.service, expected, "morf.service says the same")
    test.eq(refusal.names, "demo, media, power", "the catalog and what the configuration defined")
    test.matches(test.ipc("malformed"), "service `broken` field `level` uses an unknown coercion `numbr`")
  end)

  test.it("creates and destroys bindings repeatedly and leaves nothing behind", function()
    local baseline = counts()
    test.eq(baseline.processes, 0)
    for round = 1, 40 do
      test.eq(test.ipc("bind_all"), 3)
      local bound = counts()
      test.eq(bound.owned_hooks, baseline.owned_hooks + 3, "round " .. round .. ": each owned by its node")
      test.truthy(bound.subscribed > baseline.subscribed, "round " .. round .. ": subscribed")
      test.truthy(bound.timers > baseline.timers, "round " .. round .. ": a re-read and a tick pending")
      if round % 2 == 0 then
        test.eq(test.ipc("destroy_all"), 3, "round " .. round .. ": destroying the owner closes")
      else
        test.ipc("close_all")
        same(counts(), baseline, "round " .. round .. " closed")
        test.eq(test.ipc("destroy_all"), 3)
      end
      same(counts(), baseline, "round " .. round)
    end
    test.advance(50)
    same(counts(), baseline, "after every timer would have fired")
    test.eq(#test.logs("error"), 0, "nothing ran after its binding closed")
  end)
end)
