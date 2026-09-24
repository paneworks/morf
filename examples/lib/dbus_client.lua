-- The shared half of every system-service library: calling, reading
-- properties, and hearing about changes, over `morf.dbus`.
--
-- NetworkManager, BlueZ, UPower, MPRIS and logind are five services with one
-- shape: an object or a tree of them, properties that change, and signals
-- saying so. Each library in this folder is the part that is *about* its
-- service; this is the part that is about the bus, written once so the five do
-- not drift into five slightly different answers to the same questions.
--
-- Four facts about the engine's D-Bus binding decide how this is written:
--
-- * Every `morf.dbus.proxy` is its own bus connection. A proxy per object in a
--   tree of a hundred access points is a hundred sockets, so proxies are cached
--   by address and the libraries read trees through `GetManagedObjects` — one
--   proxy, one call — rather than an object at a time.
-- * A blocking call holds the thread that draws. Reads of a running service
--   are quick and stay blocking; anything that may wait on a radio, polkit or
--   a person goes through `call_async`, which answers on a later turn from the
--   process's one shared connection.
-- * Each subscription is a match rule on the bus, which caps them (512 per
--   connection on a stock system bus). A subscription is made once per address
--   and shared by every handler on it, and `on_signal` returns a handle whose
--   `close()` ends it when the last handler rides off — so a library can
--   follow an object for exactly as long as the object exists.
-- * A signal handler is given the body and then `info`, with the sender's
--   unique name. Two services emitting on the same path (every MPRIS player
--   lives at `/org/mpris/MediaPlayer2`) are told apart by the engine, which
--   routes a signal only to subscriptions that named its sender.
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

  --- Calls a method without waiting. `done(reply, err)` runs on a later
  --- turn with what `call` would have returned: the reply (`true` for a
  --- reply with no body), or nil and the error. Returns true once the call
  --- is on its way, or nil and why it could not be sent. `timeout_ms` is
  --- how long to wait for the answer; nothing is blocked meanwhile.
  function client.call_async(destination, path, interface, method, arguments, timeout_ms, done)
    done = done or function() end
    local ok, failure = pcall(client.dbus.call_async, client.bus, destination, path, interface,
      method, arguments or {}, { timeout_ms = timeout_ms }, function(answered, reply)
        if not answered then return done(nil, tostring(reply)) end
        if reply == nil then return done(true) end
        done(reply)
      end)
    if not ok then return nil, tostring(failure) end
    return true
  end

  --- `call_async` for a method with exactly one output.
  function client.call1_async(destination, path, interface, method, arguments, timeout_ms, done)
    done = done or function() end
    return client.call_async(destination, path, interface, method, arguments, timeout_ms,
      function(reply, err)
        if reply == nil then return done(nil, err) end
        done(dbus_client.first(reply))
      end)
  end

  --- Writes one property without waiting: `done(true)` or `done(nil, err)`.
  --- `value` may be typed (`dbus_client.d(0.5)`); it is sent inside the
  --- variant `Set` wants either way.
  function client.set_async(destination, path, interface, property, value, timeout_ms, done)
    return client.call_async(destination, path, "org.freedesktop.DBus.Properties", "Set",
      { interface, property, typed("v", value) }, timeout_ms, done)
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

  --- A handle on one handler's place in a route. `close()` takes the
  --- handler off, and the last one off ends the engine's subscription and
  --- its match rule with it. True the first time, false after.
  local function handle_for(key, route, handler)
    local handle = { closed = false }
    function handle.close()
      if handle.closed then return false end
      handle.closed = true
      for index, each in ipairs(route.handlers) do
        if each == handler then
          table.remove(route.handlers, index)
          break
        end
      end
      if #route.handlers == 0 and client.routes[key] == route then
        client.routes[key] = nil
        pcall(route.subscription.close, route.subscription)
      end
      return true
    end
    return handle
  end

  --- Hears one signal: `handler(body, info)`, `info` carrying the sender's
  --- unique name. Subscribes the address once; later handlers ride the same
  --- subscription. Returns a handle with `close()`, or nil and why.
  function client.on_signal(destination, path, interface, member, handler)
    local key = table.concat({ destination, path, interface, member }, " ")
    local route = client.routes[key]
    if route then
      route.handlers[#route.handlers + 1] = handler
      return handle_for(key, route, handler)
    end
    local proxy, err = client.proxy(destination, path, interface)
    if not proxy then return nil, err end
    route = { handlers = { handler } }
    local ok, subscription = pcall(proxy.subscribe, proxy, member, function(body, info)
      -- A copy: a handler may close its own subscription while this runs.
      local handlers = {}
      for index, each in ipairs(route.handlers) do handlers[index] = each end
      for _, each in ipairs(handlers) do each(body, info) end
    end)
    if not ok then return nil, tostring(subscription) end
    route.subscription = subscription
    client.routes[key] = route
    return handle_for(key, route, handler)
  end

  --- `PropertiesChanged` on one path:
  --- `handler(interface, changed, invalidated, info)`. Returns a handle.
  function client.on_properties(destination, path, handler)
    return client.on_signal(destination, path, "org.freedesktop.DBus.Properties",
      "PropertiesChanged", function(body, info)
        if type(body) ~= "table" then return end
        handler(body[1], body[2] or {}, body[3] or {}, info)
      end)
  end

  --- Whether a name has an owner right now. Asked of the bus, which never
  --- starts a service to answer.
  function client.has_owner(name)
    local ok, owned = pcall(client.dbus.name_has_owner, client.bus, name)
    return ok and owned == true
  end

  --- The unique name that owns `name` now, or nil.
  function client.owner_of(name)
    if not client.dbus.name_owner then return nil end
    local ok, owner = pcall(client.dbus.name_owner, client.bus, name)
    if ok and type(owner) == "string" then return owner end
  end

  --- Every name on the bus, or an empty list.
  function client.list_names()
    local ok, names = pcall(client.dbus.list_names, client.bus)
    if not ok or type(names) ~= "table" then return {} end
    return names
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

  --- Watches one name: `handler(true, new_owner)` when it appears,
  --- `handler(false, "")` when it goes. Not called for the state at the time
  --- of watching; the caller reads that itself. The bus filters by name, so
  --- this hears nothing about any other. Returns a handle, or nil and why.
  function client.watch_name(name, handler)
    local ok, subscription = pcall(client.dbus.on_name_owner_changed, client.bus, name,
      function(_, new_owner) handler(new_owner ~= "", new_owner) end)
    if not ok then return nil, tostring(subscription) end
    return subscription
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
