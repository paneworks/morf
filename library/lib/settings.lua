-- A shell's preferences: defaults in the configuration, what the user
-- changed in a JSON file, and every value a signal.
--
-- The defaults are a nested table, as a shell's settings file usually is
-- (`appearance.rounding.scale`, `bar.workspaces.shown`). The file holds only
-- what differs from them, so a default changed in the configuration reaches
-- everyone who never touched it. A read inside a binding follows that one
-- key; a write saves the file a moment later, once, however many keys a
-- handler changed. The file is watched: another screen's runtime, or a
-- person with an editor, changes it and only the keys that moved re-run
-- what follows them.
--
--   local settings = require("lib.settings")
--   local config = settings.open {
--     path = morf.config_path("shell.json"),   -- or anywhere
--     defaults = { appearance = { rounding = { scale = 1 } }, bar = { persistent = true } },
--   }
--   config.get("appearance.rounding.scale")      -- tracked
--   config.set("bar.persistent", false)          -- saved shortly
--   config.values.appearance.rounding.scale      -- the same, as a table
--   config.values.bar.persistent = true
--
-- A value must have its default's type: a number for a number, a list for
-- a list. One that does not (in the file, or given to `set`) is dropped for
-- the default with a warning. A key the defaults do not have is kept in the
-- file untouched and otherwise ignored, so a newer shell's settings survive
-- an older one.

local morf = require("morf")

local settings = {}

local function kind(value)
  if type(value) ~= "table" then return type(value) end
  if next(value) == nil or value[1] ~= nil then return "list" end
  return "table"
end

local function copy(value)
  if type(value) ~= "table" then return value end
  local out = {}
  for k, v in pairs(value) do out[k] = copy(v) end
  return out
end

local function equal(a, b)
  if type(a) ~= type(b) then return false end
  if type(a) ~= "table" then return a == b end
  for k, v in pairs(a) do if not equal(v, b[k]) then return false end end
  for k in pairs(b) do if a[k] == nil then return false end end
  return true
end

-- Every leaf of the defaults, by dotted key. A list is a leaf; so is an
-- empty table.
local function leaves(tree, prefix, out)
  for k, v in pairs(tree) do
    local key = prefix and (prefix .. "." .. k) or tostring(k)
    if kind(v) == "table" then leaves(v, key, out) else out[key] = v end
  end
  return out
end

local function lookup(tree, key)
  local node = tree
  for part in key:gmatch("[^.]+") do
    if type(node) ~= "table" then return nil end
    node = node[part]
  end
  return node
end

local function place(tree, key, value)
  local node, last = tree, nil
  for part in key:gmatch("[^.]+") do
    if last then
      if type(node[last]) ~= "table" then node[last] = {} end
      node = node[last]
    end
    last = part
  end
  node[last] = value
end

--- Opens a settings file. Options: `path` (required), `defaults`
--- (required, nested), `name` (the signals' prefix, "settings"),
--- `debounce_ms` (150), `watch` (true), `legacy` (a function given the
--- decoded file, for a file in an older shape, returning it in the
--- current one), `write_when` (optional predicate for a single writer
--- across runtimes; other runtimes can still update their local signals).
function settings.open(opts)
  assert(type(opts) == "table" and opts.path and opts.defaults, "settings.open needs path and defaults")
  local name = opts.name or "settings"
  local defaults = leaves(opts.defaults, nil, {})
  local signals = {}
  local unknown = {} -- what the file had that the defaults do not
  local last_text = nil
  local pending = nil
  local s = { path = opts.path, defaults = opts.defaults }

  local function accepts(key, value)
    local default = defaults[key]
    if default == nil then return false end
    local want, got = kind(default), kind(value)
    return want == got or (want == "list" and got == "table" and next(value) == nil)
  end
  s.accepts = accepts

  for key, default in pairs(defaults) do
    signals[key] = morf.signal(name .. "." .. key, copy(default))
  end

  local function read_file()
    local ok, text = pcall(morf.fs.read, opts.path)
    if not ok or type(text) ~= "string" then return nil, nil end
    local fine, decoded = pcall(morf.json.decode, text)
    if not fine or type(decoded) ~= "table" then
      morf.log("warn", "settings: " .. opts.path .. " is not a JSON object; the defaults stand")
      return nil, text
    end
    if opts.legacy then decoded = opts.legacy(decoded) or decoded end
    return decoded, text
  end

  -- The file's values applied: each key takes the file's value, or its
  -- default where the file leaves it out or has the wrong type.
  local function apply(stored)
    unknown = {}
    local present = leaves(stored, nil, {})
    for key, value in pairs(present) do
      if defaults[key] == nil then
        -- A table the defaults have as a leaf is taken whole below.
        local parent = key
        local found = false
        while parent:find("%.") do
          parent = parent:match("^(.*)%.[^.]+$")
          if defaults[parent] ~= nil then found = true break end
        end
        if not found then unknown[key] = value end
      end
    end
    for key, default in pairs(defaults) do
      local value = lookup(stored, key)
      if value == nil or value == morf.json.null then
        value = default
      elseif not accepts(key, value) then
        morf.log("warn", string.format("settings: %s should be a %s; the default stands", key, kind(default)))
        value = default
      end
      if not equal(signals[key]:get(), value) then signals[key]:set(copy(value)) end
    end
  end

  local function save()
    pending = nil
    if opts.write_when and not opts.write_when() then return end
    local out = {}
    for key, value in pairs(unknown) do place(out, key, value) end
    for key, default in pairs(defaults) do
      local value = signals[key]:get()
      if not equal(value, default) then place(out, key, value) end
    end
    local text = next(out) and morf.json.encode(out, true) or "{}"
    if text == last_text then return end
    last_text = text
    local ok, err = morf.fs.write(opts.path, text)
    if not ok then morf.log("warn", "settings: could not save " .. opts.path .. ": " .. tostring(err)) end
  end

  --- A value, by dotted key; tracked in a binding.
  function s.get(key)
    local signal = signals[key]
    if signal then return signal:get() end
    -- A branch: its leaves as a table.
    local out, any = {}, false
    local prefix = key .. "."
    for k, sig in pairs(signals) do
      if k:sub(1, #prefix) == prefix then
        place(out, k:sub(#prefix + 1), sig:get())
        any = true
      end
    end
    if any then return out end
    error("settings: no setting " .. tostring(key), 2)
  end

  --- Sets a value; returns true, or false and why.
  function s.set(key, value)
    if defaults[key] == nil then return false, "no setting " .. tostring(key) end
    if not accepts(key, value) then return false, key .. " should be a " .. kind(defaults[key]) end
    if equal(signals[key]:get(), value) then return true end
    signals[key]:set(copy(value))
    if not pending then pending = morf.timer(opts.debounce_ms or 150, save, false) end
    return true
  end

  --- Back to the default.
  function s.reset(key) return s.set(key, copy(defaults[key])) end

  --- Writes any change now, instead of a moment later.
  function s.flush()
    if pending then pending:cancel() save() end
  end

  --- Every dotted key.
  function s.keys()
    local out = {}
    for key in pairs(defaults) do out[#out + 1] = key end
    table.sort(out)
    return out
  end

  -- `values`: the settings as nested tables, read and written through.
  local function proxy(prefix)
    return setmetatable({}, {
      __index = function(_, k)
        local key = prefix and (prefix .. "." .. k) or k
        if signals[key] then return signals[key]:get() end
        return proxy(key)
      end,
      __newindex = function(_, k, v)
        local key = prefix and (prefix .. "." .. k) or k
        local ok, why = s.set(key, v)
        if not ok then error("settings: " .. why, 2) end
      end,
    })
  end
  s.values = proxy(nil)

  local stored, text = read_file()
  last_text = text
  if stored then apply(stored) end

  if opts.watch ~= false then
    s._watch = morf.fs.watch(opts.path, function()
      local again, new_text = read_file()
      if new_text == last_text then return end
      last_text = new_text
      apply(again or {})
    end)
  end

  return s
end

return settings
