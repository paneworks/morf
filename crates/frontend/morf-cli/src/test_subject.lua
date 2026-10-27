-- The configuration side of `morf test`, run before the configuration in
-- its own runtime. It stands in for two things a spec needs to control:
-- what `morf.process.run` answers (`test.stub_run`), and what `morf.env` reads
-- (`test.load(path, { env = ... })`). Everything else is the real engine.
--
-- `__MORF_TEST_DATA__` is replaced with `{ stubs = {...}, env = {...} }` as
-- JSON before this runs.

local data = morf.json.decode(__MORF_TEST_DATA__)
local stubs = data.stubs or {}
local env = data.env or {}
local runs = {}

local real_run = morf.process.run
local real_env = morf.env

local function stub_for(argv)
  local name = type(argv) == "table" and argv[1] or nil
  if type(name) ~= "string" then return nil end
  return stubs[name] or stubs[name:match("[^/]+$") or name]
end

local function run(argv, ...)
  local recorded = {}
  if type(argv) == "table" then
    for index, word in ipairs(argv) do recorded[index] = tostring(word) end
  end
  runs[#runs + 1] = recorded
  local stub = stub_for(argv)
  if stub == nil then return real_run(argv, ...) end
  local options, callback = ...
  if type(options) == "function" then callback = options end
  local result = { ok = true, code = 0, stdout = "", stderr = "", timed_out = false, truncated = false }
  for key, value in pairs(stub) do result[key] = value end
  if stub.ok == nil and result.code ~= 0 then result.ok = false end
  -- Answered on a later turn of the loop, as a real run is: a spec sees it
  -- after `test.advance` or `test.settle`, never inside the call.
  local done = false
  morf.time.timer(1, function()
    done = true
    if callback then callback(result) end
  end, false)
  return {
    kill = function() return not done end,
    pid = function() return 0 end,
    running = function() return not done end,
    close = function() callback = nil end,
    write = function() return false, "stubbed" end,
    close_stdin = function() end,
  }
end

local function read_env(name)
  local value = env[name]
  if value == false then return nil end
  if value ~= nil then return tostring(value) end
  return real_env(name)
end

-- What the configuration still waits on from outside (`test.io`): request
-- answers and connects under way, and the last `__morf_test.sync>>` line
-- each connection read, by path. Sync lines are the spec's, never passed on.
local outside = { pending = 0, synced = {} }
local SYNC = "__morf_test.sync>>"

-- A handle whose `close` also ends the wait, as the callback would.
local function counted(handle, finish)
  if type(handle) ~= "userdata" and type(handle) ~= "table" then return handle end
  return setmetatable({}, { __index = function(_, key)
    local value = handle[key]
    if key == "close" then return function(_, ...) finish() return value(handle, ...) end end
    if type(value) == "function" then return function(_, ...) return value(handle, ...) end end
    return value
  end })
end

local function waiting()
  outside.pending = outside.pending + 1
  local done = false
  return function()
    if done then return end
    done = true
    outside.pending = outside.pending - 1
  end
end

local real_request = morf.socket.request
local function request_socket(path, data, callback, options)
  local finish = waiting()
  local ok, handle = pcall(real_request, path, data, function(...)
    finish()
    if callback then return callback(...) end
  end, options)
  if not ok then finish() error(handle, 0) end
  return counted(handle, finish)
end

local real_connect = morf.socket.connect
local function connect(options)
  if type(options) ~= "table" then return real_connect(options) end
  local given, finish = {}, waiting()
  for key, value in pairs(options) do given[key] = value end
  given.on_connect = function(...)
    finish()
    if options.on_connect then return options.on_connect(...) end
  end
  given.on_close = function(...)
    finish()
    if options.on_close then return options.on_close(...) end
  end
  if options.on_line then
    local path = tostring(options.path or options.host or "")
    given.on_line = function(line, ...)
      if type(line) == "string" and line:sub(1, #SYNC) == SYNC then
        outside.synced[path] = line
        return
      end
      return options.on_line(line, ...)
    end
  end
  local ok, handle = pcall(real_connect, given)
  if not ok then finish() error(handle, 0) end
  return counted(handle, finish)
end

morf.process.run = run
morf.env = read_env
morf.socket.request = request_socket
morf.socket.connect = connect
morf.ipc["__morf_test.io"] = function() return outside end

morf.ipc["__morf_test.runs"] = function() return runs end
morf.ipc["__morf_test.stub"] = function(name, text)
  if text == nil or text == "" then
    stubs[name] = nil
  else
    stubs[name] = morf.json.decode(text)
  end
  return true
end
morf.ipc["__morf_test.clear_stubs"] = function()
  for name in pairs(stubs) do stubs[name] = nil end
  return true
end
