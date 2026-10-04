-- The configuration side of `morf test`, run before the configuration in
-- its own runtime. It stands in for two things a spec needs to control:
-- what `morf.run` answers (`test.stub_run`), and what `morf.env` reads
-- (`test.load(path, { env = ... })`). Everything else is the real engine.
--
-- `__MORF_TEST_DATA__` is replaced with `{ stubs = {...}, env = {...} }` as
-- JSON before this runs.

local data = morf.json.decode(__MORF_TEST_DATA__)
local stubs = data.stubs or {}
local env = data.env or {}
local runs = {}

local real_run = morf.run
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
  morf.timer(1, function()
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

morf.run = run
morf.env = read_env
for _, module in ipairs { "morf.io", "morf.core" } do
  local loaded = package.loaded[module]
  if type(loaded) == "table" then
    if loaded.run ~= nil then loaded.run = run end
    if loaded.env ~= nil then loaded.env = read_env end
  end
end

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
