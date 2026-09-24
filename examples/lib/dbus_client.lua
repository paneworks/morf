-- The shared half of every system-service library: calling, reading
-- properties, and hearing about changes, over `morf.dbus`.
--
-- NetworkManager, BlueZ, UPower, MPRIS and logind are five services with one
-- shape: an object or a tree of them, properties that change, and signals
-- saying so. Each library in this folder is the part that is *about* its
-- service; this is the part that is about the bus, written once so the five do
-- not drift into five slightly different answers to the same questions.
--
-- Three facts about the engine's D-Bus binding decide how this is written:
--
-- * Every `morf.dbus.proxy` is its own bus connection. A proxy per object in a
--   tree of a hundred access points is a hundred sockets, so proxies are cached
--   by address and the libraries read trees through `GetManagedObjects` — one
--   proxy, one call — rather than an object at a time.
-- * A subscription cannot be taken back, and each is a match rule on the bus,
--   which caps them (512 per connection on a stock system bus). So a
--   subscription is made once per address and remembered, and the libraries
--   subscribe per *stable* object only — a device, an adapter — never per
--   access point or per discovered Bluetooth stranger, whose paths never end.
-- * A signal handler is given the body and not the sender, and signals are
--   told apart by path, interface and member. Two services emitting on the
--   same path (every MPRIS player lives at `/org/mpris/MediaPlayer2`) cannot be
--   told apart by the handler; the MPRIS library re-reads rather than guesses.
--
-- The `dbus` field is a seam: it defaults to `morf.dbus`, and a test hands in
-- a table with the same `proxy` function answering from Lua. Nothing else in
-- the libraries touches the engine's bus directly.

local morf = require("morf")

local dbus_client = {}

--- A value with its D-Bus type stated.
---
--- A Lua integer goes on the wire as `x` and a string as `s`; a service that
--- declared `u` or `o` rejects the call. The engine cannot guess which one a
--- number meant, so the library says.
function dbus_client.typed(signature, value)
  return { signature = signature, value = value }
end

local typed = dbus_client.typed
function dbus_client.u(value) return typed("u", value) end
function dbus_client.o(value) return typed("o", value) end
function dbus_client.x(value) return typed("x", value) end
function dbus_client.i(value) return typed("i", value) end
function dbus_client.d(value) return typed("d", value) end

--- A byte string as the list of bytes an `ay` decodes to, and back.
---
--- SSIDs are bytes, not text: the protocol allows any octets and some routers
--- use them. They are shown as text when they are text.
function dbus_client.bytes_to_string(bytes)
  if type(bytes) == "string" then return bytes end
  if type(bytes) ~= "table" then return "" end
  local chars = {}
  for index, byte in ipairs(bytes) do
    if type(byte) ~= "number" or byte < 0 or byte > 255 then return "" end
    chars[index] = string.char(byte)
  end
  return table.concat(chars)
end

function dbus_client.string_to_bytes(text)
  local bytes = {}
  for index = 1, #text do bytes[index] = text:byte(index) end
  return bytes
end

--- Calls `fn` once, `ms` after the last time the returned function was
--- called. A burst of signals — a scan finishing touches every access point —
--- is one re-read rather than forty.
function dbus_client.debounce(ms, fn)
  local pending = false
  return function()
    if pending then return end
    pending = true
    morf.timer(ms, function()
      pending = false
      fn()
    end, false)
  end
end

--- A client for one bus. `options.dbus` replaces `morf.dbus` (the test seam);
--- `options.bus` is "system" or "session".
function dbus_client.new(options)
  options = options or {}
  local client = {
    dbus = options.dbus or morf.dbus,
    bus = options.bus or "system",
    -- Proxies by "destination path interface timeout". A proxy is a bus
    -- connection; caching one per address is the difference between a socket
    -- per call and a socket per object.
    proxies = {},
    -- Subscriptions by "destination path interface member", each with the
    -- handlers riding on it. One real subscription per address, however many
    -- parts of a library want to hear it.
    routes = {},
  }

  --- A proxy, or nil and the reason. Never raises: an absent service is a
  --- state a shell draws, not an error it crashes on.
  function client.proxy(destination, path, interface, timeout_ms)
    local key = table.concat({ destination, path, interface, tostring(timeout_ms or "") }, " ")
    local cached = client.proxies[key]
    if cached then return cached end
    local ok, proxy = pcall(client.dbus.proxy, client.bus, destination, path, interface, timeout_ms)
    if not ok then return nil, tostring(proxy) end
    client.proxies[key] = proxy
    return proxy
  end

  --- Forgets the cached proxies for a path whose object has gone (only
  --- those to `destination`, if given), so their connections can close.
  function client.forget(path, destination)
    for key in pairs(client.proxies) do
      local _, _, d, p = key:find("^(%S+) (%S+) ")
      if p == path and (destination == nil or d == destination) then
        client.proxies[key] = nil
      end
    end
  end

  --- Calls a method. `arguments` is a list of positional arguments (typed
  --- where the type is not the obvious one); nil for none. Returns the reply,
  --- or nil and the error.
  function client.call(destination, path, interface, method, arguments, timeout_ms)
    local proxy, err = client.proxy(destination, path, interface, timeout_ms)
    if not proxy then return nil, err end
    local ok, reply
    if arguments == nil or #arguments == 0 then
      ok, reply = pcall(proxy.call, proxy, method)
    else
      ok, reply = pcall(proxy.call_with, proxy, method, arguments)
    end
    if not ok then return nil, tostring(reply) end
    -- A successful call with no reply decodes to nil, which callers read as
    -- failure; `true` says it went through.
    if reply == nil then return true end
    return reply
  end

  --- Calls a method with exactly one output and returns that output.
  ---
  --- A reply arrives as the list of its outputs — `GetAll` answers
  --- `{ properties }`, not `properties` — except that a lone scalar comes
  --- bare. Which of the two a reply is depends on the engine's decoder, not
  --- on anything a caller can see, so this accepts both.
  function client.call1(destination, path, interface, method, arguments, timeout_ms)
    local reply, err = client.call(destination, path, interface, method, arguments, timeout_ms)
    if reply == nil then return nil, err end
    return dbus_client.first(reply)
  end

  --- Every property of one interface, as a table; nil and why on failure.
  function client.get_all(destination, path, interface)
    local reply, err = client.call1(destination, path, "org.freedesktop.DBus.Properties",
      "GetAll", { interface })
    if type(reply) ~= "table" then return nil, err or "no properties" end
    return reply
  end

  --- One property, or nil and why.
  function client.get(destination, path, interface, property)
    local proxy, err = client.proxy(destination, path, interface)
    if not proxy then return nil, err end
    local ok, value = pcall(proxy.get, proxy, property)
    if not ok then return nil, tostring(value) end
    return value
  end

  --- Writes one property. True, or nil and why.
  function client.set(destination, path, interface, property, value, timeout_ms)
    local proxy, err = client.proxy(destination, path, interface, timeout_ms)
    if not proxy then return nil, err end
    local ok, failure = pcall(proxy.set, proxy, property, value)
    if not ok then return nil, tostring(failure) end
    return true
  end

  --- Hears one signal. Subscribes the address once; later handlers ride the
  --- same subscription. Returns true, or nil and why.
  function client.on_signal(destination, path, interface, member, handler)
    local key = table.concat({ destination, path, interface, member }, " ")
    local route = client.routes[key]
    if route then
      route[#route + 1] = handler
      return true
    end
    local proxy, err = client.proxy(destination, path, interface)
    if not proxy then return nil, err end
    route = { handler }
    local ok, failure = pcall(proxy.subscribe, proxy, member, function(body)
      for _, each in ipairs(route) do each(body) end
    end)
    if not ok then return nil, tostring(failure) end
    client.routes[key] = route
    return true
  end

  --- `PropertiesChanged` on one path: `handler(interface, changed, invalidated)`.
  function client.on_properties(destination, path, handler)
    return client.on_signal(destination, path, "org.freedesktop.DBus.Properties",
      "PropertiesChanged", function(body)
        if type(body) ~= "table" then return end
        handler(body[1], body[2] or {}, body[3] or {})
      end)
  end

  --- Whether a name has an owner right now.
  function client.has_owner(name)
    local reply = client.call1("org.freedesktop.DBus", "/org/freedesktop/DBus",
      "org.freedesktop.DBus", "NameHasOwner", { name })
    return reply == true
  end

  --- Every name on the bus, or an empty list.
  function client.list_names()
    local reply = client.call1("org.freedesktop.DBus", "/org/freedesktop/DBus",
      "org.freedesktop.DBus", "ListNames")
    if type(reply) ~= "table" then return {} end
    return reply
  end

  --- `handler(name, old_owner, new_owner)` whenever any name changes hands.
  --- The bus sends it for every name; the caller filters. It is the one
  --- signal that says a service started or went away.
  function client.on_owner_changed(handler)
    return client.on_signal("org.freedesktop.DBus", "/org/freedesktop/DBus",
      "org.freedesktop.DBus", "NameOwnerChanged", function(body)
        if type(body) ~= "table" then return end
        handler(body[1], body[2] or "", body[3] or "")
      end)
  end

  --- Watches one name: `handler(true)` when it appears, `handler(false)` when
  --- it goes. Not called for the state at the time of watching; the caller
  --- reads that itself.
  function client.watch_name(name, handler)
    return client.on_owner_changed(function(changed, _, new_owner)
      if changed == name then handler(new_owner ~= "") end
    end)
  end

  --- The whole tree under an ObjectManager: path -> interface -> properties.
  function client.managed_objects(destination, root)
    local reply, err = client.call1(destination, root, "org.freedesktop.DBus.ObjectManager",
      "GetManagedObjects")
    if type(reply) ~= "table" then return nil, err or "no objects" end
    return reply
  end

  return client
end

--- The one output of a reply or a signal body: `{ value }` is `value`, a
--- bare scalar is itself, and a successful call with no output (`true`) stays
--- `true`.
function dbus_client.first(reply)
  if type(reply) == "table" and reply.signature == nil then return reply[1] end
  return reply
end

--- Applies a `PropertiesChanged` to a property table in place.
function dbus_client.merge(properties, changed, invalidated)
  for key, value in pairs(changed or {}) do properties[key] = value end
  for _, key in ipairs(invalidated or {}) do properties[key] = nil end
  return properties
end

--- A list sorted by a comparison, without touching the original.
function dbus_client.sorted(list, less)
  local copy = {}
  for index, value in ipairs(list) do copy[index] = value end
  table.sort(copy, less)
  return copy
end

return dbus_client
