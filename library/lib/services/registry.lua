-- A registry of system services, and the generic binder most of them
-- were hand-rolling (PLAN item 7).
--
-- A service is data plus a short list of what is genuinely its own:
--
--   local upower = registry.define {
--     name = "upower", bus = "system",
--     service = "org.freedesktop.UPower",
--     path = "/org/freedesktop/UPower",
--     interface = "org.freedesktop.UPower",
--     properties = {
--       on_battery = "OnBattery",
--       lid_closed = { prop = "LidIsClosed", default = false },
--     },
--     objects = {
--       field = "devices", key = "path",
--       list = function(client, svc) return svc.enumerate() end,
--       interface = "org.freedesktop.UPower.Device",
--       map = function(props, path) return { path = path, pct = props.Percentage } end,
--     },
--     methods = function(client, svc, state)
--       return { refresh = function() ... end }
--     end,
--   }
--   local power = upower.connect()
--   ui.Text { text = function() return power.state.on_battery and "battery" or "ac" end }
--
-- The binder owns the pattern every D-Bus service repeated: create the
-- client (through dbus_client, so the test seam is the same), watch
-- the name, read once, subscribe to PropertiesChanged (debounced),
-- re-read on change, follow the service away and back, and close
-- everything on `close()`. Property values land in `state` under their
-- descriptor names; `available` says whether the name is owned.
-- Derived values are ordinary bindings in the descriptor's `state`
-- table, computed by the reactive graph like anywhere else.
--
-- Descriptor names are the permission vocabulary of
-- `crates/engine/morf-lua/src/vm/permissions.rs`: a service a shell
-- binds is a capability it uses, named the same in both places.

local dbus_client = require("lib.services.dbus_client")

local registry = {}
registry.definitions = {}
registry.services = {}

--- Defines a service. The result has `connect(options)`; options are
--- `dbus` (the test seam), `bus`, and `debounce_ms` (60).
function registry.define(descriptor)
  assert(type(descriptor) == "table", "a service descriptor is a table")
  assert(type(descriptor.name) == "string", "a service needs a name")
  assert(type(descriptor.service) == "string", "a service needs a bus name")
  assert(type(descriptor.path) == "string", "a service needs an object path")
  assert(type(descriptor.interface) == "string", "a service needs an interface")
  registry.definitions[descriptor.name] = descriptor

  local service = { descriptor = descriptor }
  registry.services[descriptor.name] = service

  function service.connect(options)
    options = options or {}
    local client = dbus_client.new({ dbus = options.dbus, bus = options.bus or descriptor.bus })
    local svc = descriptor.service

    local initial = { available = false }
    for field, spec in pairs(descriptor.properties or {}) do
      initial[field] = (type(spec) == "table" and spec.default) or false
    end
    -- Fixed-path property groups (a display device, a power-profiles
    -- daemon): each writes into its own sub-table of the state, and
    -- may live on a second bus name.
    for target, group in pairs(descriptor.groups or {}) do
      local sub = { available = false }
      for field, spec in pairs(group.properties or {}) do
        sub[field] = (type(spec) == "table" and spec.default) or false
      end
      initial[target] = sub
    end
    if descriptor.objects then initial[descriptor.objects.field] = {} end
    for field, value in pairs(descriptor.state or {}) do initial[field] = value end
    local state = morf.state(initial)

    local handles = {}
    local object_handles = {}
    local closed = false
    -- Property deltas heard from PropertiesChanged, by path: a
    -- re-read may race the change it answers, so the heard values
    -- overlay the read ones until the next delta replaces them.
    local heard = {}
    local function note_heard(path, changed, invalidated)
      local cache = heard[path]
      if not cache then cache = {} heard[path] = cache end
      for key, value in pairs(changed or {}) do cache[key] = value end
      for _, key in ipairs(invalidated or {}) do cache[key] = nil end
    end
    local function overlay(path, all)
      local cache = heard[path]
      if not cache then return all end
      local merged = {}
      for key, value in pairs(all) do merged[key] = value end
      for key, value in pairs(cache) do merged[key] = value end
      return merged
    end

    local bound = { state = state, client = client }

    local function read_properties()
      if not client.has_owner(svc) then
        state.available = false
        return false
      end
      local all = client.get_all(svc, descriptor.path, descriptor.interface)
      if not all then
        state.available = false
        return false
      end
      all = overlay(descriptor.path, all)
      state.available = true
      for field, spec in pairs(descriptor.properties or {}) do
        local prop, default = spec, nil
        if type(spec) == "table" then prop, default = spec.prop, spec.default end
        local value = all[prop]
        if value == nil then value = default end
        if type(spec) == "table" and spec.map then value = spec.map(value) end
        state[field] = value
      end
      return true
    end

    local read_objects
    if descriptor.objects then
      local objects = descriptor.objects
      read_objects = function()
        local paths = objects.list(client, bound) or {}
        local rows, present = {}, {}
        for _, path in ipairs(paths) do
          present[path] = true
          local props = client.get_all(svc, path, objects.interface)
          if props then rows[#rows + 1] = objects.map(overlay(path, props), path) end
          if not object_handles[path] and not closed then
            object_handles[path] = client.on_properties(svc, path, function(_, changed, invalidated)
              note_heard(path, changed, invalidated)
              bound._schedule_objects()
            end)
          end
        end
        local gone = {}
        for path in pairs(object_handles) do
          if not present[path] then gone[#gone + 1] = path end
        end
        for _, path in ipairs(gone) do
          local handle = object_handles[path]
          if handle and handle.close then handle.close() end
          object_handles[path] = nil
        end
        table.sort(rows, function(a, b) return tostring(a[objects.key or "path"]) < tostring(b[objects.key or "path"]) end)
        bound.rows = rows
        state[objects.field]:replace(rows, objects.key or "path")
      end
    end

    local function read_group(target, group)
      local owner = group.service or svc
      if not client.has_owner(owner) then
        state[target].available = false
        return
      end
      local all = client.get_all(owner, group.path, group.interface)
      if not all then
        state[target].available = false
        return
      end
      all = overlay(group.path, all)
      state[target].available = true
      for field, spec in pairs(group.properties or {}) do
        local prop, default = spec, nil
        if type(spec) == "table" then prop, default = spec.prop, spec.default end
        local value = all[prop]
        if value == nil then value = default end
        if type(spec) == "table" and spec.map then value = spec.map(value) end
        state[target][field] = value
      end
    end

    local function refresh_now()
      if closed then return end
      read_properties()
      for target, group in pairs(descriptor.groups or {}) do read_group(target, group) end
      if read_objects then read_objects() end
      if descriptor.on_refresh then descriptor.on_refresh(bound, client, state) end
    end
    local schedule = dbus_client.debounce(
      options.debounce_ms or descriptor.debounce_ms or 60, refresh_now)
    bound._schedule_objects = schedule

    if descriptor.methods then
      for name, method in pairs(descriptor.methods(client, bound, state)) do
        bound[name] = method
      end
    end

    -- After the descriptor's methods, on purpose: a method named
    -- `refresh` or `close` must not replace the binder's own, or it
    -- would call itself forever.
    function bound.refresh()
      schedule()
    end

    function bound.close()
      closed = true
      for _, handle in ipairs(handles) do
        if handle and handle.close then handle.close() end
      end
      handles = {}
      local gone = {}
      for path in pairs(object_handles) do gone[#gone + 1] = path end
      for _, path in ipairs(gone) do
        local handle = object_handles[path]
        if handle and handle.close then handle.close() end
        object_handles[path] = nil
      end
    end

    handles[#handles + 1] = client.watch_name(svc, function() schedule() end)
    handles[#handles + 1] = client.on_properties(svc, descriptor.path,
      function(_, changed, invalidated)
        note_heard(descriptor.path, changed, invalidated)
        schedule()
      end)
    for _, signal in ipairs(descriptor.signals or {}) do
      handles[#handles + 1] = client.on_signal(
        svc, signal.path or descriptor.path, signal.interface or descriptor.interface,
        signal.member, function(body) signal.fn(bound, body) end)
    end
    for _, group in pairs(descriptor.groups or {}) do
      local owner = group.service or svc
      if owner ~= svc then
        handles[#handles + 1] = client.watch_name(owner, function() schedule() end)
      end
      handles[#handles + 1] = client.on_properties(owner, group.path,
        function(_, changed, invalidated)
          note_heard(group.path, changed, invalidated)
          schedule()
        end)
    end
    -- The first read is not debounced: a consumer that connects and
    -- reads its state in the same turn finds it there.
    refresh_now()
    return bound
  end

  return service
end

--- Connects to a service by its registered name. An unknown name
--- fails naming what is registered, so a typo is answered in place.
function registry.connect(name, options)
  local service = registry.services[name]
  if not service then
    local names = {}
    for registered in pairs(registry.services) do names[#names + 1] = registered end
    table.sort(names)
    error(("unknown service `%s` (registered: %s)"):format(
      tostring(name), table.concat(names, ", ")), 2)
  end
  return service.connect(options)
end

return registry
