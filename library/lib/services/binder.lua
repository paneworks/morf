-- The registry's binder: one descriptor's objects, subscriptions, debounce
-- and teardown, kept as reactive state. Reached through
-- `registry.connect(name)` or `morf.service(name)`; the descriptor format
-- is described in registry.lua and docs/IO.md.
--
-- What the binder holds for a binding:
--
-- * a fixed source (an object at a known path, or the first owned of
--   `any_of` alternatives): its raw properties and which bus name answered;
-- * a set: its members by key (an object path, or a bus name), each with
--   its raw properties per source, its coerced reading and the handles of
--   its own subscriptions;
-- * the handles of every subscription, timer and closer, all ended by
--   `close()`.

local morf = require("morf")
local dbus_client = require("lib.services.dbus_client")

local binder = {}

local GENERIC = {
  state = true, client = true, descriptor = true, options = true, available = true,
  refresh = true, close = true, closed = true, members = true, rows = true, on_read = true,
  on_publish = true, timer = true, cancel = true, own = true, reread = true,
}

local function merged(base, extra)
  local out = {}
  for key, value in pairs(base or {}) do out[key] = value end
  for key, value in pairs(extra or {}) do out[key] = value end
  return out
end

local function close_handle(handle)
  if handle and handle.close then pcall(handle.close, handle) end
end

-- The descriptor with the connect options' overrides applied.
local function resolve(descriptor, options)
  local service = options.service or descriptor.service
  local sources, sets = {}, {}
  for name, spec in pairs(descriptor.objects or {}) do
    local set = merged(spec, (options.objects or {})[name])
    set.name, set.sources, set.members = name, {}, {}
    if set.names then
      set.ignored = {}
      for _, each in ipairs(set.names.ignore or {}) do set.ignored[set.names.prefix .. each] = true end
    end
    for field, each in pairs(set.fields) do
      if each.meta == "key" then set.key_field = field end
    end
    sets[name] = set
  end
  for alias, spec in pairs(descriptor.interfaces) do
    local source = merged(spec, (options.interfaces or {})[alias])
    source.alias = alias
    if source.any_of then
      local alternatives = {}
      for index, each in ipairs(source.any_of) do
        alternatives[index] = {
          service = each.service or each.name, path = each.path, interface = each.interface,
        }
      end
      source.any_of = alternatives
    elseif source.set then
      source.service = source.service or service
      table.insert(sets[source.set].sources, source)
    else
      source.service = source.service or service
    end
    sources[alias] = source
  end
  for _, set in pairs(sets) do
    table.sort(set.sources, function(a, b) return a.alias < b.alias end)
  end
  return { name = descriptor.name, service = service, sources = sources, sets = sets }
end

-- Every state field as a plan entry, with the initial shape beside it.
local function plan(fields, sets, help, path, entries, initial)
  for name, spec in pairs(fields) do
    local at = {}
    for index, each in ipairs(path) do at[index] = each end
    at[#at + 1] = name
    local kind
    if spec.meta then
      kind = "meta"
      if spec.meta == "count" then initial[name] = 0
      elseif spec.meta == "available" then initial[name] = false
      else initial[name] = "" end
    elseif spec.as == "records" then
      kind, initial[name] = "records", {}
    elseif spec.fields then
      initial[name] = {}
      plan(spec.fields, sets, help, at, entries, initial[name])
    elseif sets[spec.from] then
      kind, initial[name] = "list", {}
    else
      kind, initial[name] = "value", help.coerced(spec, nil)
      assert(initial[name] ~= nil, ("field `%s` coerces nothing to nil: give it a default")
        :format(table.concat(at, ".")))
    end
    if kind then entries[#entries + 1] = { path = at, spec = spec, kind = kind } end
  end
end

local function matches(where, reading)
  for field, wanted in pairs(where or {}) do
    if type(wanted) == "table" and wanted.except ~= nil then
      if reading[field] == wanted.except then return false end
    elseif reading[field] ~= wanted then
      return false
    end
  end
  return true
end

local function project(reading, columns)
  local row = {}
  for _, column in ipairs(columns) do row[column] = reading[column] end
  return row
end

--- Binds `descriptor`: see registry.connect.
function binder.bind(descriptor, options, help)
  local plan_of = resolve(descriptor, options)
  local sources, sets = plan_of.sources, plan_of.sets
  local client = dbus_client.new({ dbus = options.dbus, bus = options.bus or descriptor.bus })
  local debounce_ms = options.debounce_ms or descriptor.debounce_ms or 60

  local entries, initial = {}, {}
  plan(descriptor.fields, sets, help, {}, entries, initial)
  for field, value in pairs(descriptor.state or {}) do initial[field] = value end
  local by_from = {}
  for _, entry in ipairs(entries) do
    local from = entry.spec.from
    by_from[from] = by_from[from] or {}
    table.insert(by_from[from], entry)
  end
  local state = morf.state(initial)

  local binding = { state = state, client = client, descriptor = plan_of, options = options }
  local closed = false
  local handles, timers, closers = {}, {}, {}
  local read_hooks, publish_hooks = {}, {}
  local fixed, published = {}, {}
  for alias, source in pairs(sources) do
    if not source.set then fixed[alias] = {} end
  end

  local function write(entry, value)
    local target = state
    for index = 1, #entry.path - 1 do target = target[entry.path[index]] end
    local name = entry.path[#entry.path]
    if entry.kind == "list" or entry.kind == "records" then
      target[name]:replace(value, entry.kind == "list" and sets[entry.spec.from].key_field
        or entry.spec.key)
    else
      target[name] = value
    end
  end

  ------------------------------------------------------------------ owners

  local memo -- owners asked within one pass
  local function owned(name)
    if memo and memo[name] ~= nil then return memo[name] end
    local answer = client.has_owner(name)
    if memo then memo[name] = answer end
    return answer
  end

  local touched -- sets whose members changed within one pass
  local function pass(fn)
    local outer = memo ~= nil
    if not outer then memo, touched = {}, {} end
    fn()
    if outer then return end
    local sets_touched = touched
    memo, touched = nil, nil
    for name in pairs(sets_touched) do
      local set = sets[name]
      local readings = {}
      for _, member in pairs(set.members) do
        if member.reading then readings[#readings + 1] = member.reading end
      end
      table.sort(readings, function(a, b)
        return tostring(a[set.key_field]) < tostring(b[set.key_field])
      end)
      for _, entry in ipairs(by_from[name] or {}) do
        if entry.kind == "list" then
          local columns = entry.spec.only
          if not columns then
            columns = {}
            for column in pairs(set.fields) do columns[#columns + 1] = column end
          end
          local rows = {}
          for _, reading in ipairs(readings) do
            if matches(entry.spec.where, reading) then rows[#rows + 1] = project(reading, columns) end
          end
          published[entry.path[#entry.path]] = rows
          write(entry, rows)
        elseif entry.spec.meta == "count" then
          write(entry, #readings)
        elseif entry.spec.meta == "available" then
          write(entry, #readings > 0)
        end
      end
    end
    for _, hook in ipairs(publish_hooks) do hook() end
  end

  ----------------------------------------------------------- fixed sources

  local function apply_source(alias)
    local held = fixed[alias]
    for _, entry in ipairs(by_from[alias] or {}) do
      local spec = entry.spec
      if entry.kind == "meta" then
        if spec.meta == "available" then write(entry, held.props ~= nil)
        elseif spec.meta == "service" then write(entry, held.answered and held.answered.service or "") end
      else
        write(entry, help.coerced(spec, help.raw_of(held.props, spec.prop)))
      end
    end
  end

  local function read_source(alias)
    local source, held = sources[alias], fixed[alias]
    held.props, held.answered = nil, nil
    for _, each in ipairs(source.any_of or { source }) do
      -- Never by activation: a name nobody owns is absent, not started.
      if owned(each.service) then
        local props = client.get_all(each.service, each.path, each.interface)
        if props then
          held.props, held.answered = props, each
          break
        end
      end
    end
    apply_source(alias)
  end

  ---------------------------------------------------------------- members

  local function member_address(set, source, key)
    if set.names then return key, source.path end
    return source.service, key
  end

  -- A member's reading from its raw properties, amended by the read hooks.
  local function coerce_member(set, member, previous)
    local reading = help.coerce_fields(set.fields,
      function(alias) return member.props[alias] end,
      function(meta) if meta == "key" then return member.key end end)
    for _, hook in ipairs(read_hooks[set.name] or {}) do hook(member.key, reading, previous) end
    member.reading = reading
    touched[set.name] = true
  end

  local function read_member(set, member)
    local previous = member.reading
    member.reading = nil
    for _, source in ipairs(set.sources) do
      local dest, path = member_address(set, source, member.key)
      local props = client.get_all(dest, path, source.interface)
      if not props and not source.optional then
        member.props = {}
        touched[set.name] = true
        return
      end
      member.props[source.alias] = props or nil
    end
    coerce_member(set, member, previous)
  end

  local schedule
  local dirty = { sources = {}, members = {}, lists = {} }

  local function mark_member(set, key)
    dirty.members[set.name] = dirty.members[set.name] or {}
    dirty.members[set.name][key] = true
    schedule()
  end

  local function member_changed(set, key, interface, changed, invalidated)
    local member = set.members[key]
    if closed or not member then return end
    for _, source in ipairs(set.sources) do
      if source.interface == interface then
        local props = member.props[source.alias]
        if source.changes == "reread" or next(invalidated) or not props or not member.reading then
          return mark_member(set, key)
        end
        dbus_client.merge(props, changed)
        pass(function() coerce_member(set, member, member.reading) end)
        return
      end
    end
  end

  local function add_member(set, key)
    local member = set.members[key]
    if member then return read_member(set, member) end
    member = { key = key, props = {}, handles = {} }
    set.members[key] = member
    local seen = {}
    for _, source in ipairs(set.sources) do
      local dest, path = member_address(set, source, key)
      local address = dest .. " " .. path
      if not seen[address] then
        seen[address] = true
        member.handles[#member.handles + 1] = client.on_properties(dest, path,
          function(interface, changed, invalidated)
            member_changed(set, key, interface, changed, invalidated)
          end)
      end
      for _, signal in ipairs(descriptor.signals or {}) do
        if signal.from == source.alias then
          member.handles[#member.handles + 1] = client.on_signal(dest, path, source.interface,
            signal.member, function() if not closed then mark_member(set, key) end end)
        end
      end
    end
    read_member(set, member)
  end

  local function remove_member(set, key)
    local member = set.members[key]
    if not member then return end
    for _, handle in ipairs(member.handles) do close_handle(handle) end
    set.members[key] = nil
    touched[set.name] = true
    for _, source in ipairs(set.sources) do
      local dest, path = member_address(set, source, key)
      if client.forget then client.forget(path, dest) end
    end
  end

  local function wanted(set, name)
    return name:sub(1, #set.names.prefix) == set.names.prefix and not set.ignored[name]
  end

  -- Lists a set's members afresh; `reread` re-reads those already held.
  local function list_set(set, reread)
    local keys = {}
    if set.names then
      for _, name in ipairs(client.list_names()) do
        if wanted(set, name) then keys[#keys + 1] = name end
      end
    else
      local source = sources[set.list.from]
      if owned(source.service) then
        keys = help.coerced({ as = "object_paths" },
          client.call1(source.service, source.path, source.interface, set.list.method))
      end
    end
    local present = {}
    for _, key in ipairs(keys) do
      present[key] = true
      if not set.members[key] then add_member(set, key)
      elseif reread then read_member(set, set.members[key]) end
    end
    local gone = {}
    for key in pairs(set.members) do
      if not present[key] then gone[#gone + 1] = key end
    end
    for _, key in ipairs(gone) do remove_member(set, key) end
    touched[set.name] = true
  end

  ------------------------------------------------------------ refreshing

  local function refresh_all()
    pass(function()
      for alias in pairs(fixed) do read_source(alias) end
      for _, set in pairs(sets) do list_set(set, true) end
    end)
  end

  local function flush()
    if closed then return end
    local now = dirty
    dirty = { sources = {}, members = {}, lists = {} }
    pass(function()
      for alias in pairs(now.sources) do read_source(alias) end
      for name in pairs(now.lists) do list_set(sets[name], false) end
      for name, keys in pairs(now.members) do
        for key in pairs(keys) do
          local member = sets[name].members[key]
          if member then read_member(sets[name], member) end
        end
      end
    end)
  end

  function binding.timer(ms, fn, repeating)
    if closed then return nil end
    local handle
    handle = morf.timer(ms, function()
      if not repeating then timers[handle] = nil end
      if not closed then fn() end
    end, repeating == true)
    timers[handle] = true
    return handle
  end

  function binding.cancel(handle)
    if not handle then return end
    timers[handle] = nil
    handle:cancel()
  end

  local pending
  schedule = function()
    if pending or closed then return end
    pending = binding.timer(debounce_ms, function()
      pending = nil
      flush()
    end, false)
  end

  -------------------------------------------------------------- the API

  --- Re-reads everything now: every source, every set and member.
  function binding.refresh()
    if closed then return end
    dirty = { sources = {}, members = {}, lists = {} }
    refresh_all()
  end

  if initial.available ~= nil then
    function binding.available() return state.available end
  end

  function binding.closed() return closed end

  --- A set's members, by key: the coerced readings.
  function binding.members(name)
    local out = {}
    for key, member in pairs(sets[name].members) do
      if member.reading then out[key] = member.reading end
    end
    return out
  end

  --- A list field's rows as last published, as plain tables.
  function binding.rows(field) return published[field] or {} end

  --- `fn(key, reading, previous)` after each member of `set` is read, before
  --- it is published: the place to add what a reading cannot coerce.
  function binding.on_read(set, fn)
    read_hooks[set] = read_hooks[set] or {}
    table.insert(read_hooks[set], fn)
  end

  --- `fn()` after each change has been published.
  function binding.on_publish(fn) table.insert(publish_hooks, fn) end

  --- `fn()` when the binding closes.
  function binding.own(fn) table.insert(closers, fn) end

  --- Re-reads one member, debounced.
  function binding.reread(set, key)
    if not closed and sets[set].members[key] then mark_member(sets[set], key) end
  end

  --- Ends every subscription, timer and closer. True the first time.
  function binding.close()
    if closed then return false end
    closed = true
    for _, handle in ipairs(handles) do close_handle(handle) end
    for _, set in pairs(sets) do
      for _, member in pairs(set.members) do
        for _, handle in ipairs(member.handles) do close_handle(handle) end
      end
      set.members = {}
    end
    for handle in pairs(timers) do pcall(handle.cancel, handle) end
    timers = {}
    for _, fn in ipairs(closers) do
      local ok, err = pcall(fn)
      if not ok then morf.log.error(("service `%s` closer: %s"):format(plan_of.name, tostring(err))) end
    end
    if client.release then client.release() end
    return true
  end

  for name, action in pairs(descriptor.actions or {}) do
    assert(not GENERIC[name], ("service `%s` action `%s` hides the binder's own"):format(
      plan_of.name, name))
    if action.write then
      binding[name] = function(value)
        local answered = fixed[action.on].answered
        if closed or not answered then return nil, action.missing or ("no " .. action.on) end
        local ok, err = client.set(answered.service, answered.path, answered.interface,
          action.write, help.coerced({ as = action.as or "raw" }, value))
        pass(function() read_source(action.on) end)
        return ok, err
      end
    elseif action.rows then
      binding[name] = function() return published[action.rows] or {} end
    end
  end

  ----------------------------------------------------------- subscribing

  local watched, listened = {}, {}
  local function watch(name, fn)
    if watched[name] then return table.insert(watched[name], fn) end
    watched[name] = { fn }
    handles[#handles + 1] = client.watch_name(name, function()
      if closed then return end
      pass(function() for _, each in ipairs(watched[name]) do each() end end)
    end)
  end

  local function source_changed(alias, each, interface, changed, invalidated)
    local source, held = sources[alias], fixed[alias]
    if closed or interface ~= each.interface then return end
    if held.answered ~= each or source.changes == "reread" or next(invalidated) then
      dirty.sources[alias] = true
      return schedule()
    end
    dbus_client.merge(held.props, changed)
    pass(function() apply_source(alias) end)
  end

  for alias, source in pairs(sources) do
    if not source.set then
      for _, each in ipairs(source.any_of or { source }) do
        watch(each.service, function() read_source(alias) end)
        local address = each.service .. " " .. each.path
        listened[address] = listened[address] or {}
        table.insert(listened[address], function(interface, changed, invalidated)
          source_changed(alias, each, interface, changed, invalidated)
        end)
      end
    end
  end
  for address, listeners in pairs(listened) do
    local dest, path = address:match("^(%S+) (%S+)$")
    handles[#handles + 1] = client.on_properties(dest, path, function(...)
      for _, listener in ipairs(listeners) do listener(...) end
    end)
  end
  for _, set in pairs(sets) do
    if set.list then
      watch(sources[set.list.from].service, function() list_set(set, true) end)
    end
  end

  for _, signal in ipairs(descriptor.signals or {}) do
    local source = sources[signal.from]
    if not source.set then
      handles[#handles + 1] = client.on_signal(source.service, source.path, source.interface,
        signal.member, function(body)
          if closed then return end
          local set, key = sets[signal.adds or signal.removes], dbus_client.first(body)
          if set and type(key) == "string" then
            pass(function()
              if signal.adds then add_member(set, key) else remove_member(set, key) end
            end)
          elseif set then
            dirty.lists[set.name] = true
            schedule()
          else
            dirty.sources[signal.from] = true
            schedule()
          end
        end)
    end
  end

  local named = false
  for _, set in pairs(sets) do named = named or set.names ~= nil end
  if named then
    handles[#handles + 1] = client.on_owner_changed(function(name, _, new_owner)
      if closed then return end
      for _, set in pairs(sets) do
        if set.names and wanted(set, name) then
          pass(function()
            if new_owner ~= "" then add_member(set, name) else remove_member(set, name) end
          end)
        end
      end
    end)
  end

  if descriptor.bind then
    local own = {}
    for name in pairs(GENERIC) do own[name] = binding[name] end
    local ok, err = pcall(descriptor.bind, binding)
    for name, value in pairs(own) do
      if ok and binding[name] ~= value then
        ok, err = false, ("service `%s`: bind replaced the binder's own `%s`"):format(plan_of.name, name)
      end
    end
    if not ok then
      own.close()
      error(err, 0)
    end
  end
  refresh_all()
  return binding
end

return binder
