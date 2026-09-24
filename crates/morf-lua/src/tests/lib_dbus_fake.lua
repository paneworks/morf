-- A stand-in for `morf.dbus`, answering from Lua tables.
--
-- The service libraries in `examples/lib` take their bus through a `dbus`
-- option, and this is what a test hands them: a registry of objects with
-- properties and methods, replying in the shapes the engine's decoder
-- produces (a method's outputs as a list, a lone scalar bare, variants
-- unwrapped), and recording every call with its arguments exactly as the
-- library typed them — which is the point: a `u` sent as a Lua integer goes
-- out as `x` and a real service rejects it, and only a record of what was
-- sent can catch that without the real service.
--
-- Nothing here touches a real bus, so these tests can press every button a
-- real network or radio would mind being pressed.

local fake = { objects = {}, methods = {}, owners = {}, calls = {}, subscriptions = {}, fds = {} }

local function key(...) return table.concat({ ... }, "|") end

--- Strips `{ signature, value }` wrappers, as the far end of a bus would.
local function plain(value)
  if type(value) ~= "table" then return value end
  if value.signature ~= nil and value.value ~= nil then return plain(value.value) end
  local out = {}
  for k, v in pairs(value) do out[k] = plain(v) end
  return out
end
fake.plain = plain

local function copy(value)
  if type(value) ~= "table" then return value end
  local out = {}
  for k, v in pairs(value) do out[k] = copy(v) end
  return out
end

--- Puts an object on the fake bus: `props` its properties (plain values),
--- `methods` a table of `name = function(args, call) return reply end`.
function fake.object(bus, dest, path, iface, props, methods)
  fake.owners[key(bus, dest)] = true
  fake.objects[bus] = fake.objects[bus] or {}
  fake.objects[bus][dest] = fake.objects[bus][dest] or {}
  fake.objects[bus][dest][path] = fake.objects[bus][dest][path] or {}
  fake.objects[bus][dest][path][iface] = props or {}
  fake.methods[key(bus, dest, path, iface)] = methods or {}
  return fake.objects[bus][dest][path][iface]
end

function fake.remove(bus, dest, path)
  if fake.objects[bus] and fake.objects[bus][dest] then fake.objects[bus][dest][path] = nil end
end

function fake.props(bus, dest, path, iface)
  local d = fake.objects[bus] and fake.objects[bus][dest]
  return d and d[path] and d[path][iface]
end

--- Whether a name is owned; `false` makes every call to it fail as the bus
--- would for a service that is not running.
function fake.own(bus, dest, owned)
  fake.owners[key(bus, dest)] = owned and true or nil
end

--- The unique name a fake service speaks with: what a real signal's
--- `info.sender` carries.
function fake.unique(dest)
  if dest == "org.freedesktop.DBus" then return dest end
  return ":fake." .. dest
end

--- Delivers a signal to whoever subscribed to exactly this address, as
--- the engine does: the body, then `{ sender, path, interface, member,
--- arguments }`.
function fake.emit(bus, dest, path, iface, member, body)
  local list = fake.subscriptions[key(bus, dest, path, iface, member)] or {}
  -- A copy of the list: a handler may close its own subscription.
  local snapshot = {}
  for index, entry in ipairs(list) do snapshot[index] = entry end
  for _, entry in ipairs(snapshot) do
    if not entry.closed then
      entry.callback(copy(body), {
        sender = fake.unique(dest), path = path, interface = iface, member = member,
        arguments = copy(body),
      })
    end
  end
end

--- A stand-in for a file descriptor handle: `close()` and `is_open()`, as
--- the engine's userdata has.
function fake.fd(label)
  local handle = { label = label, open = true }
  function handle:close()
    local was = self.open
    self.open = false
    return was
  end
  function handle:is_open() return self.open end
  fake.fds[#fake.fds + 1] = handle
  return handle
end

--- The calls made to one method, in order.
function fake.calls_to(member)
  local found = {}
  for _, call in ipairs(fake.calls) do
    if call.method == member then found[#found + 1] = call end
  end
  return found
end

local function fail(message) error(message, 0) end

local function dispatch(bus, dest, path, iface, method, args)
  args = args or {}
  local call = { bus = bus, dest = dest, path = path, iface = iface, method = method, args = args }
  fake.calls[#fake.calls + 1] = call
  if dest == "org.freedesktop.DBus" then
    if method == "NameHasOwner" then return fake.owners[key(bus, args[1])] == true end
    if method == "ListNames" then
      local names = {}
      for k in pairs(fake.owners) do
        local b, n = k:match("^([^|]*)|(.*)$")
        if b == bus then names[#names + 1] = n end
      end
      table.sort(names)
      return { names }
    end
    fail("org.freedesktop.DBus.Error.UnknownMethod: " .. method)
  end
  if not fake.owners[key(bus, dest)] then
    fail("org.freedesktop.DBus.Error.ServiceUnknown: The name " .. dest .. " was not provided")
  end
  local objects = fake.objects[bus] and fake.objects[bus][dest] or {}
  if iface == "org.freedesktop.DBus.Properties" then
    local target = objects[path] and objects[path][plain(args[1])]
    if not target then fail("org.freedesktop.DBus.Error.UnknownObject: " .. path) end
    if method == "GetAll" then return { copy(target) } end
    if method == "Get" then return copy(target[plain(args[2])]) end
    if method == "Set" then
      -- Recorded as `proxy:set` records it, so a test reads a property
      -- write the same way whichever route the library took. The variant
      -- is the wire's wrapper, not what the library typed.
      local value = args[3]
      if type(value) == "table" and value.signature == "v" then value = value.value end
      call.property, call.value = plain(args[2]), value
      target[plain(args[2])] = plain(args[3])
      return nil
    end
  end
  if iface == "org.freedesktop.DBus.ObjectManager" and method == "GetManagedObjects" then
    return { copy(objects) }
  end
  local handler = fake.methods[key(bus, dest, path, iface)]
  handler = handler and handler[method]
  if not handler then fail("org.freedesktop.DBus.Error.UnknownMethod: " .. method .. " on " .. path) end
  return handler(args, call)
end

local proxy_methods = {}
proxy_methods.__index = proxy_methods

function proxy_methods:get(property)
  if not fake.owners[key(self.bus, self.dest)] then
    fail("org.freedesktop.DBus.Error.ServiceUnknown: " .. self.dest)
  end
  local props = fake.props(self.bus, self.dest, self.path, self.iface)
  if not props then fail("org.freedesktop.DBus.Error.UnknownObject: " .. self.path) end
  return copy(props[property])
end

function proxy_methods:set(property, value)
  fake.calls[#fake.calls + 1] = {
    bus = self.bus, dest = self.dest, path = self.path, iface = self.iface,
    method = "Set", property = property, value = value,
  }
  if not fake.owners[key(self.bus, self.dest)] then
    fail("org.freedesktop.DBus.Error.ServiceUnknown: " .. self.dest)
  end
  local props = fake.props(self.bus, self.dest, self.path, self.iface)
  if not props then fail("org.freedesktop.DBus.Error.UnknownObject: " .. self.path) end
  props[property] = plain(value)
end

--- The engine's rule for arguments: none; one (a list of them, or the one
--- argument); or several.
local function positional(...)
  local count = select("#", ...)
  if count == 0 then return {} end
  if count > 1 then return { ... } end
  local args = ...
  if args == nil then return {} end
  if type(args) ~= "table" or args.signature ~= nil or getmetatable(args) ~= nil then
    return { args }
  end
  return args
end

function proxy_methods:call(method, ...)
  return dispatch(self.bus, self.dest, self.path, self.iface, method, positional(...))
end

function proxy_methods:call_with(method, ...)
  return dispatch(self.bus, self.dest, self.path, self.iface, method, positional(...))
end

--- Answers on a later turn, as the engine does: the call is recorded now,
--- the callback runs from a timer.
local function answer_later(callback, ok, reply)
  local morf = require("morf")
  morf.timer(1, function() callback(ok, reply) end, false)
end

local function dispatch_async(bus, dest, path, iface, method, args, callback)
  local ok, reply = pcall(dispatch, bus, dest, path, iface, method, args)
  fake.calls[#fake.calls].async = true
  answer_later(callback, ok, reply)
  return true
end

function proxy_methods:call_async(method, ...)
  local all = table.pack(...)
  local callback = all[all.n]
  return dispatch_async(self.bus, self.dest, self.path, self.iface, method,
    positional(table.unpack(all, 1, all.n - 1)), callback)
end

local function subscribe(k, callback)
  fake.subscriptions[k] = fake.subscriptions[k] or {}
  local entry = { callback = callback }
  table.insert(fake.subscriptions[k], entry)
  local handle = {}
  function handle:close()
    if entry.closed then return false end
    entry.closed = true
    for index, each in ipairs(fake.subscriptions[k]) do
      if each == entry then table.remove(fake.subscriptions[k], index) break end
    end
    return true
  end
  handle.unsubscribe = handle.close
  function handle:active() return not entry.closed end
  return handle
end

function proxy_methods:subscribe(member, callback)
  return subscribe(key(self.bus, self.dest, self.path, self.iface, member), callback)
end

fake.dbus = {
  proxy = function(bus, dest, path, iface, timeout)
    fake.last_timeout = timeout
    return setmetatable({ bus = bus, dest = dest, path = path, iface = iface, timeout = timeout },
      proxy_methods)
  end,
  call_async = function(bus, dest, path, iface, method, args, options, callback)
    if type(options) == "function" then options, callback = nil, options end
    fake.last_timeout = options and options.timeout_ms
    return dispatch_async(bus, dest, path, iface, method, positional(args), callback)
  end,
  name_has_owner = function(bus, name)
    return fake.owners[key(bus, name)] == true
  end,
  list_names = function(bus)
    local names = {}
    for k in pairs(fake.owners) do
      local b, n = k:match("^([^|]*)|(.*)$")
      if b == bus then names[#names + 1] = n end
    end
    table.sort(names)
    return names
  end,
  on_name_owner_changed = function(bus, name, callback)
    return subscribe(key(bus, "org.freedesktop.DBus", "/org/freedesktop/DBus",
      "org.freedesktop.DBus", "NameOwnerChanged"), function(body)
      if type(body) == "table" and body[1] == name then
        callback(body[2] or "", body[3] or "", body[1])
      end
    end)
  end,
}

--- Counts the live subscriptions to one address.
function fake.subscribed(bus, dest, path, iface, member)
  return #(fake.subscriptions[key(bus, dest, path, iface, member)] or {})
end

--- Runs the steps one after another, `gap_ms` apart, each in its own timer
--- turn so the libraries' debounced re-reads have fired between them. The
--- verdict ("ok" or the first failure) goes to `done(text)`.
function fake.steps(steps, done, gap_ms)
  local morf = require("morf")
  local index = 0
  local function run()
    index = index + 1
    local step = steps[index]
    if not step then
      done("ok")
      return
    end
    local ok, err = pcall(step)
    if not ok then
      done("FAIL step " .. index .. ": " .. tostring(err))
      return
    end
    morf.timer(gap_ms or 120, run, false)
  end
  morf.timer(1, run, false)
end

--- `assert` with the two values in the message.
function fake.eq(actual, expected, what)
  if actual ~= expected then
    error((what or "value") .. ": expected " .. tostring(expected) .. ", got " .. tostring(actual), 2)
  end
end

return fake
