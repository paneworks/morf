-- Where an authentication somewhere on the machine has got to -- sudo
-- looking for a face, waiting at the fingerprint reader, asking for the
-- password, through -- for a shell that wants to show it.
--
--   local authsteps = require("lib.util.authsteps")
--   local steps = authsteps.new()
--   steps.watch()           -- what the markers write, in the runtime dir
--   morf.ipc["auth-step"] = steps.mark      -- or said over IPC
--   steps.state.step        -- "idle", "face", "finger", "password", "ok", "failed"
--   steps.state.service     -- "sudo", ...
--   steps.state.failures    -- wrong tries in this run, for a shake
--
-- The steps come from markers in the PAM stacks themselves
-- (tools/pam/morf-auth-step, one `pam_exec` line before each step), because
-- a program like sudo talks to its own terminal and nothing else can see
-- its conversation. A stack that starts over at its first step after the
-- password -- sudo asking again -- was a wrong password. A step that is
-- followed by nothing for a while has been given up on, and it goes quiet.

local morf = require("morf")

local authsteps = {}

local ORDER = { face = 1, finger = 2, password = 3, ok = 4 }
-- How long each step is shown with nothing after it: a face and a finger
-- time out by themselves, a password is typed at the terminal's pace.
local QUIET = { face = 20000, finger = 35000, password = 60000, ok = 2500, failed = 2200 }

function authsteps.new()
  local state = morf.state { step = "idle", service = "", failures = 0, since = 0, pid = "" }
  local steps = { state = state }
  local quiet

  local function go(step)
    state.step = step
    state.since = morf.time.now_ms()
    if quiet then quiet:cancel() end
    quiet = morf.timer(QUIET[step] or 1500, function()
      quiet = nil
      if step == "ok" or step == "failed" then
        state.step = "idle"
        state.failures = 0
      else
        state.step = "failed"
        quiet = morf.timer(QUIET.failed, function()
          quiet = nil
          state.step = "idle"
          state.failures = 0
        end, false)
      end
    end, false)
  end

  --- A marker: `step` is face, finger, password or ok; `service` the stack's;
  --- `pid` the process asking, when the marker says.
  function steps.mark(step, service, pid)
    -- Asked with no step: how it stands, and where it listens.
    if step == nil then
      return { step = state.step, service = state.service, watching = steps.dir or "" }
    end
    if not ORDER[step] then return nil end
    local was = state.step
    pid = tostring(pid or "")
    local running = not (was == "idle" or was == "ok" or was == "failed")
    -- One run at a time: while one is on screen, another process's steps
    -- (a prompt checking sudo elsewhere) are not this one's.
    if running and pid ~= "" and state.pid ~= "" and pid ~= state.pid then return nil end
    -- Through without a step shown first: sudo with a login it still
    -- remembers, a rule that needs no password, a prompt checking whether
    -- sudo would ask (`sudo -n true`, every few seconds) -- nothing was
    -- asked of anybody, and there is nothing to show.
    if step == "ok" and (was == "idle" or was == "ok" or was == "failed") then return nil end
    if was == "idle" or was == "ok" or was == "failed" then
      state.failures = 0
    elseif ORDER[step] < (ORDER[was] or 0) then
      -- Back to an earlier step: the stack started over, the last try
      -- was wrong.
      state.failures = state.failures + 1
    end
    state.service = tostring(service or "")
    if not running then state.pid = pid end
    go(step)
    return nil
  end

  --- Watches `dir` (the runtime directory's `morf`) for the markers'
  --- line: `authstep`, "STEP SERVICE". A marker runs as root inside sudo and
  --- cannot wait on anything, so it writes and leaves; this hears it.
  function steps.watch(dir)
    dir = dir or ((morf.env("XDG_RUNTIME_DIR") or "") .. "/morf")
    if dir == "/morf" or not morf.fs.exists(dir) then return nil end
    local ok, handle = pcall(morf.fs.watch, dir, function(event)
      if event.name ~= "authstep" or event.kind == "deleted" then return end
      local okr, text = pcall(morf.fs.read, dir .. "/authstep")
      if not okr or type(text) ~= "string" then return end
      local step, service, pid = text:match("^(%a+)%s*(%S*)%s*(%S*)")
      if step then steps.mark(step, service, pid) end
    end)
    steps.watcher = ok and handle or nil
    steps.dir = steps.watcher and dir or nil
    return steps.watcher
  end

  return steps
end

return authsteps
