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

--- Starts a program by argv, never through a shell. Returns the process or
--- nil. `mutates` marks a program that changes the machine, which a dry
--- run does not start.
function M.spawn(what, program, argv, mutates)
  if mutates and M.dry then
    morf.log("info", "impasto: dry run, not " .. tostring(what))
    return nil
  end
  local ok, process = pcall(morf.process, program, argv or {})
  if not ok then return nil end
  return process
end

--- Whether `program` is on PATH, found without a shell.
function M.which(program)
  for dir in tostring(morf.env("PATH") or ""):gmatch("[^:]+") do
    local path = dir .. "/" .. program
    if morf.fs.exists(path) then return path end
  end
  return nil
end

--- Runs a program to its end and hands `on_done(stdout, success)` what it
--- printed. The process is drained from a timer: the engine does not watch
--- a child for readiness. Gives up after `timeout_ms` (default 8000).
function M.collect(what, program, argv, on_done, options)
  options = options or {}
  local process = M.spawn(what, program, argv, options.mutates)
  if not process then
    if on_done then on_done("", false) end
    return false
  end
  local out, waited, timer = {}, 0, nil
  timer = morf.timer(options.poll_ms or 50, function()
    waited = waited + (options.poll_ms or 50)
    while true do
      local ok, event = pcall(process.next, process)
      if not ok or not event then break end
      if event.kind == "stdout" then
        out[#out + 1] = event.data
      elseif event.kind == "exit" then
        timer:cancel()
        if on_done then on_done(table.concat(out), event.success) end
        return
      end
    end
    if waited >= (options.timeout_ms or 8000) then
      timer:cancel()
      pcall(process.kill, process)
      if on_done then on_done(table.concat(out), false) end
    end
  end)
  return true
end

return M
