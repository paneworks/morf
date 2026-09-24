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

local fake = { objects = {}, methods = {}, owners = {}, calls = {}, subscriptions = {} }

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

--- Delivers a signal to whoever subscribed to exactly this address.
function fake.emit(bus, dest, path, iface, member, body)
  for _, callback in ipairs(fake.subscriptions[key(bus, dest, path, iface, member)] or {}) do
    callback(copy(body))
  end
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

function proxy_methods:call(method)
  return dispatch(self.bus, self.dest, self.path, self.iface, method, {})
end

function proxy_methods:call_with(method, args)
  -- The engine's rule: a list is the positional arguments, anything else is
  -- the one argument.
  if type(args) ~= "table" or args.signature ~= nil then args = { args } end
  return dispatch(self.bus, self.dest, self.path, self.iface, method, args)
end

function proxy_methods:subscribe(member, callback)
  local k = key(self.bus, self.dest, self.path, self.iface, member)
  fake.subscriptions[k] = fake.subscriptions[k] or {}
  table.insert(fake.subscriptions[k], callback)
end

fake.dbus = {
  proxy = function(bus, dest, path, iface, timeout)
    fake.last_timeout = timeout
    return setmetatable({ bus = bus, dest = dest, path = path, iface = iface, timeout = timeout },
      proxy_methods)
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
