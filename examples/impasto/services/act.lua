-- The one door every action that changes the machine goes through.
--
-- The quick settings can turn the Wi-Fi off, drop a Bluetooth headset, mute
-- the speakers or dim the screen. Each of those is wired to a click and
-- nothing else, and each is sent through `act.run` so that a shell started
-- with IMPASTO_DRY_RUN=1 (a test bench, a screenshot session) logs what it
-- would have done instead of doing it. The panels still behave: a dry run
-- answers true, and the readings simply do not move.

local M = {}

M.dry = (morf.env("IMPASTO_DRY_RUN") or "") ~= ""

--- Runs `fn(...)` unless this is a dry run. `what` names the action for the
--- log. Returns what `fn` returned, or true on a dry run.
function M.run(what, fn, ...)
  if M.dry then
    morf.log("info", "impasto: dry run, not " .. tostring(what))
    return true
  end
  if not fn then return nil, "unavailable" end
  local ok, a, b = pcall(fn, ...)
  if not ok then
    morf.log("warn", "impasto: " .. tostring(what) .. " failed: " .. tostring(a))
    return nil, a
  end
  if a == nil and b ~= nil then
    morf.log("warn", "impasto: " .. tostring(what) .. ": " .. tostring(b))
  end
  return a, b
end

--- Starts a program by argv, never through a shell. Returns the handle
--- (`:kill`, `:running`, `:pid`) or nil. `mutates` marks a program that
--- changes the machine, which a dry run does not start. `options` goes to
--- `morf.spawn` as it is (`on_exit`, `on_stdout`, `detached`, ...).
function M.spawn(what, program, argv, mutates, options)
  if mutates and M.dry then
    morf.log("info", "impasto: dry run, not " .. tostring(what))
    return nil
  end
  local spec = {}
  for key, value in pairs(options or {}) do spec[key] = value end
  local command = { program }
  for _, arg in ipairs(argv or {}) do command[#command + 1] = arg end
  spec.command = command
  local ok, handle = pcall(morf.spawn, spec)
  if not ok or not handle then return nil end
  return handle
end

--- Whether `program` is on PATH, found without a shell.
function M.which(program)
  for dir in tostring(morf.env("PATH") or ""):gmatch("[^:]+") do
    local path = dir .. "/" .. program
    if morf.fs.exists(path) then return path end
  end
  return nil
end

--- Runs a program to its end and hands `on_done(stdout, success, result)`
--- what it printed, when it exits (`morf.run`). Gives up after `timeout_ms`
--- (default 8000). A `mutates` program is not run on a dry run, and
--- `on_done` hears ("", false) at once.
function M.collect(what, program, argv, on_done, options)
  options = options or {}
  if options.mutates and M.dry then
    morf.log("info", "impasto: dry run, not " .. tostring(what))
    if on_done then on_done("", false) end
    return false
  end
  local command = { program }
  for _, arg in ipairs(argv or {}) do command[#command + 1] = arg end
  local ok, handle = pcall(morf.run, command, {
    timeout_ms = options.timeout_ms or 8000,
    stdin = options.stdin,
    env = options.env,
  }, function(result)
    if on_done then on_done(result.stdout or "", result.ok == true, result) end
  end)
  if not ok or not handle then
    if on_done then on_done("", false) end
    return false
  end
  return true
end

return M
