-- System services described as data, and the one binder that runs them.
--
-- A descriptor names a service by the capability a shell uses ("power",
-- "media") and says, as data, which D-Bus objects it reads, which signals
-- change what, and how each raw property becomes a state field:
--
--   registry.define {
--     name = "power", bus = "system", service = "org.freedesktop.UPower",
--     debounce_ms = 60,
--     interfaces = {
--       root = { path = "/org/freedesktop/UPower", interface = "org.freedesktop.UPower" },
--     },
--     fields = {
--       available = { from = "root", meta = "available" },
--       on_battery = { from = "root", prop = "OnBattery", as = "bool" },
--     },
--   }
--   local power = registry.connect("power")   -- or morf.service("power")
--   ui.Text { text = function() return power.state.on_battery and "battery" or "ac" end }
--
-- The binder owns what every hand-written service used to redo: it reads
-- each object once, merges `PropertiesChanged` in place, re-reads only what
-- a signal invalidated (debounced), follows names coming and going, keeps
-- dynamic object sets (objects a method lists, or every bus name under a
-- prefix) with a subscription per member, and closes all of it on
-- `close()`. Nothing in a descriptor is procedural except `bind`, the one
-- escape hatch, for what the format cannot say. The full format is in
-- docs/IO.md, "System services".
--
-- Descriptor names are the capability vocabulary a permissions manifest
-- lists (`services = { "media", "power" }`); `registry.names()` is that list.

local registry = {}

--- The services the library ships, by name: the module that defines each,
--- from `lib.services.catalog`. `registry.connect` requires the module on
--- first use.
registry.catalog = {}
for name, entry in pairs(require("lib.services.catalog")) do
  registry.catalog[name] = entry.module
end

registry.definitions = {}

------------------------------------------------------------------ coercion

local coerce = {}
registry.coerce = coerce

local function fallback(spec, neutral)
  if spec.default ~= nil then return spec.default end
  return neutral
end

function coerce.raw(value, spec)
  if value == nil then return spec.default end
  return value
end

function coerce.bool(value, spec)
  if value == nil then return fallback(spec, false) end
  return value == true
end

function coerce.number(value, spec)
  return tonumber(value) or fallback(spec, 0)
end

function coerce.percent(value, spec)
  local number = tonumber(value)
  if number == nil then return fallback(spec, 0) end
  return math.max(0, math.min(100, number))
end

--- `value / by`: microseconds to seconds is `{ as = "divide", by = 1e6 }`.
function coerce.divide(value, spec)
  local number = tonumber(value)
  if number == nil then return fallback(spec, 0) end
  return number / spec.by
end

function coerce.string(value, spec)
  if value == nil then return fallback(spec, "") end
  return tostring(value)
end

function coerce.lower(value, spec)
  return string.lower(coerce.string(value, spec))
end

--- `values[value]`: a number on the wire as a word.
function coerce.enum(value, spec)
  local word = value ~= nil and spec.values[value] or nil
  if word == nil then return fallback(spec, "") end
  return word
end

--- Whether `value` is one of `values`.
function coerce.one_of(value, spec)
  for _, each in ipairs(spec.values) do
    if each == value then return true end
  end
  return false
end

--- A list as "a, b"; a scalar as text.
function coerce.list_text(value, spec)
  if type(value) == "table" then
    local parts = {}
    for _, each in ipairs(value) do parts[#parts + 1] = tostring(each) end
    return table.concat(parts, ", ")
  end
  if value == nil then return fallback(spec, "") end
  return tostring(value)
end

function coerce.object_paths(value)
  local paths = {}
  if type(value) ~= "table" then return paths end
  for _, each in ipairs(value) do
    if type(each) == "string" then paths[#paths + 1] = each end
  end
  return paths
end

local coerce_fields

--- A list of dictionaries as rows of `fields`; an entry without the
--- `key` field's property is skipped.
function coerce.records(value, spec)
  local rows = {}
  if type(value) ~= "table" then return rows end
  local key_prop = spec.fields[spec.key].prop
  for _, entry in ipairs(value) do
    if type(entry) == "table" and entry[key_prop] ~= nil then
      rows[#rows + 1] = coerce_fields(spec.fields, function() return entry end)
    end
  end
  return rows
end

local function raw_of(props, prop)
  if props == nil or prop == nil then return nil end
  if type(prop) ~= "table" then return props[prop] end
  local value = props
  for _, key in ipairs(prop) do
    if type(value) ~= "table" then return nil end
    value = value[key]
  end
  return value
end

local function coerced(spec, raw)
  return coerce[spec.as or "raw"](raw, spec)
end

-- `props_for(alias)` answers the raw properties a field reads; `meta` the
-- binder's own facts (`available`, `service`, `count`, `key`).
function coerce_fields(fields, props_for, meta)
  local out = {}
  for name, spec in pairs(fields) do
    if spec.meta then
      out[name] = meta and meta(spec.meta, spec.from)
    else
      out[name] = coerced(spec, raw_of(props_for(spec.from), spec.prop))
    end
  end
  return out
end

---------------------------------------------------------------- definition

local METAS = { available = true, service = true, count = true, key = true }

local function inherit(fields, from)
  for _, spec in pairs(fields) do
    if spec.from == nil then spec.from = from end
    if spec.fields and spec.as ~= "records" then inherit(spec.fields, spec.from) end
  end
end

local function check_fields(name, fields, known, where)
  for field, spec in pairs(fields) do
    local at = ("service `%s` %s `%s`"):format(name, where, field)
    assert(type(spec) == "table", at .. " is not a table")
    if spec.meta then
      assert(METAS[spec.meta], at .. " names an unknown meta `" .. tostring(spec.meta) .. "`")
    elseif spec.fields and spec.as ~= "records" then
      check_fields(name, spec.fields, known, "field")
    elseif spec.as == "records" then
      assert(spec.key and spec.fields[spec.key], at .. " records need a `key` among their fields")
      for _, each in pairs(spec.fields) do
        assert(coerce[each.as or "raw"], at .. " uses an unknown coercion")
      end
    elseif spec.prop == nil and not spec.only and not spec.where then
      assert(known.sets[spec.from], at .. " needs a `prop`, a `meta` or `fields`")
    end
    if not spec.meta and spec.as then
      assert(coerce[spec.as], at .. " uses an unknown coercion `" .. tostring(spec.as) .. "`")
    end
    if spec.from ~= nil then
      assert(known.sources[spec.from] or known.sets[spec.from] or spec.as == "records",
        at .. " reads from unknown `" .. tostring(spec.from) .. "`")
    end
  end
end

--- Registers a descriptor under its name and returns it. Errors name the
--- descriptor and the part of it that is wrong.
function registry.define(descriptor)
  assert(type(descriptor) == "table", "a service descriptor is a table")
  local name = descriptor.name
  assert(type(name) == "string" and name:match("^[a-z][a-z0-9_]*$"),
    "a service needs a name: a short lowercase capability noun")
  assert(descriptor.bus == "system" or descriptor.bus == "session",
    ("service `%s` needs bus = \"system\" or \"session\""):format(name))
  assert(type(descriptor.interfaces) == "table", ("service `%s` needs interfaces"):format(name))
  assert(type(descriptor.fields) == "table", ("service `%s` needs fields"):format(name))
  local known = { sources = descriptor.interfaces, sets = descriptor.objects or {} }
  for alias, source in pairs(descriptor.interfaces) do
    local at = ("service `%s` interface `%s`"):format(name, alias)
    if source.any_of then
      assert(type(source.any_of) == "table", at .. " lists its alternatives in any_of")
    else
      assert(type(source.interface) == "string", at .. " needs an interface")
      assert(source.set == nil or known.sets[source.set], at .. " belongs to an unknown set")
      assert(source.set or type(source.path) == "string", at .. " needs a path")
    end
  end
  for set, spec in pairs(known.sets) do
    local at = ("service `%s` set `%s`"):format(name, set)
    assert(spec.names or spec.list, at .. " needs `names` or `list`")
    assert(spec.names == nil or type(spec.names.prefix) == "string", at .. " needs a name prefix")
    assert(spec.list == nil or known.sources[spec.list.from], at .. " lists from an unknown interface")
    assert(type(spec.fields) == "table", at .. " needs fields")
    local keyed = false
    for _, field in pairs(spec.fields) do keyed = keyed or field.meta == "key" end
    assert(keyed, at .. " needs a field with meta = \"key\"")
    inherit(spec.fields, spec.from)
    check_fields(name, spec.fields, known, "set field")
  end
  inherit(descriptor.fields, nil)
  check_fields(name, descriptor.fields, known, "field")
  for _, signal in ipairs(descriptor.signals or {}) do
    assert(known.sources[signal.from], ("service `%s` signal `%s` is on an unknown interface")
      :format(name, tostring(signal.member)))
  end
  for action, spec in pairs(descriptor.actions or {}) do
    local at = ("service `%s` action `%s`"):format(name, action)
    if spec.write then
      local source = known.sources[spec.on]
      assert(source and not source.set, at .. " writes to an unknown interface")
    else
      assert(spec.rows and descriptor.fields[spec.rows], at .. " needs `write` or `rows`")
    end
  end
  assert(descriptor.bind == nil or type(descriptor.bind) == "function",
    ("service `%s`: bind is a function"):format(name))
  registry.definitions[name] = descriptor
  return descriptor
end

--- Every service name the registry can bind, sorted: the catalog and
--- anything defined since.
function registry.names()
  local seen, names = {}, {}
  for name in pairs(registry.catalog) do seen[name] = true end
  for name in pairs(registry.definitions) do seen[name] = true end
  for name in pairs(seen) do names[#names + 1] = name end
  table.sort(names)
  return names
end

--- The descriptor for `name`, or nil and an error naming every service
--- that is registered.
function registry.find(name)
  local descriptor = registry.definitions[name]
  if not descriptor and type(name) == "string" and registry.catalog[name] then
    require(registry.catalog[name])
    descriptor = registry.definitions[name]
  end
  if descriptor then return descriptor end
  return nil, ("unknown service `%s` (registered: %s)"):format(
    tostring(name), table.concat(registry.names(), ", "))
end

--- Binds a service by name. Options: `dbus` (the test seam), `bus`,
--- `service` (the main bus name), `debounce_ms`, `interfaces` and
--- `objects` (partial overrides by alias), and whatever the descriptor's
--- `bind` reads. An unknown name raises, listing the registered ones.
function registry.connect(name, options)
  local descriptor, err = registry.find(name)
  if not descriptor then error(err, 2) end
  return require("lib.services.binder").bind(descriptor, options or {}, {
    coerce_fields = coerce_fields, coerced = coerced, raw_of = raw_of,
  })
end

return registry
