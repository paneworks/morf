-- The lock: locking the session over the blurred desk, and opening it again.
--
-- Port of LockService.qml. In Quickshell the lock is a surface of the shell's
-- own process. In morf a session lock is a whole run of its own: a
-- configuration that says `morf.surface.session_lock = true` is taken by
-- morf as one lock client with a surface on every output, held until the
-- file clears that flag, and the process ends with the lock. So this file has
-- two halves, one per process:
--
--   the shell     `lock()` closes the island, photographs every screen's
--                 desk, blurs a copy of each, and starts `morf init.lua --
--                 lock`, then watches that process for as long as it lives.
--                 `locked()` and `secure()` are what the rest of the shell
--                 reads. `secure()` is the compositor's word, not a guess:
--                 the lock writes it down only once the compositor confirms
--                 the session is hidden, and a refused lock ends the lock
--                 process, so nothing (a suspend) goes on as if it held.
--
--   the lock      `init.lua -- lock` builds lock/screen.lua over the two
--                 pictures, and everything under "the lock's own state"
--                 below: awake or at rest, the password, PAM, the face.
--
-- A separate process is also what the protocol wants: the compositor keeps
-- the session locked if the lock client dies, so a crash in the lock leaves
-- the machine locked rather than open, and a crash in the bar cannot take the
-- lock with it.
--
-- The screenshots go to the runtime directory (tmpfs, user-only), one per
-- output, and the lock deletes them as it lets go. The password is a plain local handed to PAM
-- and dropped; it is never a signal.

local settings = require("services.settings")
local theme = require("theme")
local account = require("services.account")
local session = require("services.session")

local M = {}

local fs = morf.fs

M.dir = fs.join(morf.env("XDG_RUNTIME_DIR") or "/tmp", "impasto-morf")

local function safe(name)
  name = tostring(name or "")
  if name == "" then name = "screen" end
  return (name:gsub("[^%w%-_.]", "_"))
end

--- The desk of one output, and its blurred copy.
function M.desk_path(output) return fs.join(M.dir, "lock-desk-" .. safe(output) .. ".jpg") end
function M.blur_path(output) return fs.join(M.dir, "lock-blur-" .. safe(output) .. ".jpg") end

local function remove_pictures()
  for _, entry in ipairs(fs.list(M.dir) or {}) do
    if entry.name:match("^lock%-desk%-.*%.jpg$") or entry.name:match("^lock%-blur%-.*%.jpg$") then
      fs.remove(entry.path)
    end
  end
end
-- Written by the lock once it holds the screen; the shell reads it as
-- "secure".
M.held_path = fs.join(M.dir, "lock-held")

-- ================================================================ shell --

local s = {
  locked = morf.signal("impasto.lock.locked", false),
  secure = morf.signal("impasto.lock.secure", false),
  pending = morf.signal("impasto.lock.pending", false),
}

--- A lock process is running (or being started).
function M.locked() return s.locked:get() or s.pending:get() end

--- The lock holds the screen: what a suspend waits for.
function M.secure() return s.secure:get() end

local listeners = {}
--- `fn(locked)` whenever the lock goes up or comes down.
function M.on_change(fn) listeners[#listeners + 1] = fn end
local function announce(locked)
  for _, fn in ipairs(listeners) do pcall(fn, locked) end
end

local watch_timer

local function exists(path)
  return fs.stat(path) ~= nil
end

-- Every screen runs the shell, and every screen hears `morf ipc call lock`,
-- so the lock is claimed first: a directory made without `parents` is made
-- by exactly one of them. The claim holds "starting <ms>" while the desk is
-- photographed, then the lock process's pid; the others adopt that pid.
local claim_dir = fs.join(M.dir, "lock.claim")
local claim_file = fs.join(claim_dir, "pid")
local STALE_CLAIM = 10000

local function release_claim()
  fs.remove(claim_dir, { recursive = true })
end

local function alive(pid) return pid ~= nil and exists("/proc/" .. pid) end

-- The lock process is watched rather than waited on: `exec_detached` leaves
-- no pipe between the two, so the bar dying never takes the lock with it.
local function watch(pid)
  if watch_timer then watch_timer:cancel() end
  s.locked:set(true)
  announce(true)
  watch_timer = morf.timer(250, function()
    if alive(pid) then
      if not s.secure:get() and exists(M.held_path) then s.secure:set(true) end
      return
    end
    watch_timer:cancel()
    watch_timer = nil
    s.secure:set(false)
    s.locked:set(false)
    fs.remove(M.held_path)
    -- A lock that never came up (refused, or no session lock at all)
    -- leaves the pictures behind; nobody else will take them.
    remove_pictures()
    release_claim()
    announce(false)
  end, true)
end

local function launch()
  if not s.pending:get() then return end
  s.pending:set(false)
  fs.remove(M.held_path)
  local ok, pid = pcall(morf.exec_detached, {
    morf.executable or "morf", morf.shell_path("init.lua"), "--", "lock",
  })
  if not ok then
    morf.log("error", "impasto: could not start the lock: " .. tostring(pid))
    release_claim()
    return
  end
  fs.write(claim_file, tostring(pid))
  watch(tostring(pid))
end

--- Another screen has the claim: follow its lock once its pid is written.
--- True while the claim is live; false when it was stale and is gone.
local function adopt()
  local text = fs.read(claim_file) or ""
  local pid = text:match("^(%d+)$")
  if pid then
    if alive(pid) then watch(pid) return true end
    release_claim()
    return false
  end
  local since = tonumber(text:match("^starting (%d+)$") or "")
  if not since or morf.time.now_ms() - since > STALE_CLAIM then
    release_claim()
    return false
  end
  -- Still photographing: look again shortly.
  local tries = 0
  local timer
  timer = morf.timer(250, function()
    tries = tries + 1
    local now = (fs.read(claim_file) or ""):match("^(%d+)$")
    if now and alive(now) then
      timer:cancel()
      watch(now)
    elseif tries > 40 or not exists(claim_dir) then
      timer:cancel()
    end
  end, true)
  return true
end

--- The blur is done at a quarter of the size: blurred, the lost detail is
--- the point, and a quarter of the pixels is a sixteenth of the work.
--- `done()` runs once the blurred copy is written, or is not coming.
local function blur_desk(output, done)
  local source = M.desk_path(output)
  local shot = morf.image.info(source)
  if not shot then return done() end
  local w = math.max(1, math.floor(shot.width / 4))
  local h = math.max(1, math.floor(shot.height / 4))
  -- `lockBlur` is the original's blurMax, a radius in screen pixels; a
  -- Gaussian's sigma is about half its radius, then quartered with the size.
  local sigma = math.max(0.5, math.min(100, (settings.lockBlur or 32) / 6))
  local ok, queued = pcall(morf.image.process, {
    source = source, output = M.blur_path(output), quality = 85,
    ops = { { "resize", w, h, "exact" }, { "blur", sigma } },
    on_done = function() done() end,
  })
  if not ok or not queued then done() end
end

--- Locks: once the island has closed (or it would be in the picture), the
--- desk is photographed, blurred, and the lock put up over it. Nothing is
--- waited on for long: an untidy picture, or none, is better than an
--- unlocked machine.
function M.lock()
  if M.locked() then return end
  fs.mkdir(M.dir)
  if not fs.mkdir(claim_dir, { parents = false }) then
    if adopt() then return end
    if not fs.mkdir(claim_dir, { parents = false }) then return end
  end
  fs.write(claim_file, "starting " .. tostring(morf.time.now_ms()))
  s.pending:set(true)
  remove_pictures()
  local ok, island = pcall(require, "bar.island")
  local settle = 40
  if ok and island.state.expanded() then
    island.close()
    settle = theme.duration_morph() + 60
  end
  -- Lock anyway if the picture never comes.
  morf.timer(settle + 2500, function()
    if s.pending:get() then
      morf.log("warn", "impasto: the desk did not come back in time; locking without it")
      launch()
    end
  end, false)
  -- The screencopy reads the compositor, not this scene: two frames for the
  -- closed island to be what is on screen. Every output is photographed,
  -- so each lock surface lies over its own desk; the lock goes up once the
  -- last picture is blurred (or failed).
  morf.timer(settle, function()
    local outputs = {}
    for _, screen in ipairs(morf.screens or {}) do
      if screen.name and screen.name ~= "" then outputs[#outputs + 1] = screen.name end
    end
    if #outputs == 0 then return launch() end
    local waiting = #outputs
    local function one_done()
      waiting = waiting - 1
      if waiting == 0 then launch() end
    end
    for _, output in ipairs(outputs) do
      local queued = pcall(morf.screencopy.save, {
        path = M.desk_path(output), quality = 90, output = output,
        on_done = function(done)
          if done then blur_desk(output, one_done) else one_done() end
        end,
      })
      if not queued then one_done() end
    end
  end, false)
end

session.attach_lock(M)

-- =================================================== the lock's own state --

-- Shared by every screen, since every screen draws the one tree.
local st = morf.state {
  -- Awake shows the account, the field and the power buttons; at rest, the
  -- clock alone.
  awake = false,
  authenticating = false,
  failed = false,
  message = "",
  -- From the answer until the surface has let go of the desk.
  leaving = false,
  -- How many characters are in the field: all the screen may know of it.
  typed = 0,
  -- Bumped once per refusal, so a shake can follow it.
  refusals = 0,
  face_ready = false,
  face_scanning = false,
  face_matched = false,
  face_misses = 0,
  -- A fresh counter for the island's shake.
  face_missed = 0,
}
M.state = st

-- How long an awake screen waits untouched before going back to its clock.
M.AWAKE_FOR = 30000

local password = ""
local drowse_generation = 0
local face_session
local face_quiet_until = 0
local FACE_REST, FACE_IDLE, FACE_TRIES, FACE_HOLD = 1500, 5000, 3, 650

-- The password stack: a dedicated lock service where the machine has one,
-- else the one every login shares, as the greeter chooses.
local function password_service()
  for _, name in ipairs { "impasto-lock", "morf-lock", "login", "system-auth" } do
    if exists("/etc/pam.d/" .. name) then return name end
  end
  return "login"
end
M.PASSWORD_SERVICE = "login"

local function now_ms() return morf.time.now_ms() end

local function drowse()
  drowse_generation = drowse_generation + 1
  local mine = drowse_generation
  morf.timer(M.AWAKE_FOR, function()
    if mine ~= drowse_generation then return end
    -- A password being checked is not a screen left alone.
    if st.authenticating then drowse() else M.rest() end
  end, false)
end

local function clear()
  password = ""
  st.typed = 0
end

--- Somebody is there: the screen wakes, and a face is looked for.
function M.rouse()
  if st.leaving then return end
  st.awake = true
  drowse()
  M.wake()
end

--- Back to the clock: the field empties, a scan under way stops.
function M.rest()
  drowse_generation = drowse_generation + 1
  st.awake = false
  clear()
  if face_session then face_session:cancel() face_session = nil end
  st.face_scanning = false
  st.failed = false
  st.message = ""
end

--- A character typed into the field.
function M.type(character)
  if st.authenticating or st.leaving then return end
  if #password >= 256 then return end
  password = password .. character
  st.typed = #password
  -- Typing clears the error.
  if st.failed then st.failed = false end
end

function M.backspace()
  if st.authenticating or st.leaving then return end
  -- One character, not one byte.
  password = password:gsub("[\1-\127\194-\244][\128-\191]*$", "")
  st.typed = #password
end

--- Lets go: the blur relaxes and the type goes, then the lock falls.
function M.release()
  if st.leaving then return end
  st.leaving = true
  clear()
  if face_session then face_session:cancel() face_session = nil end
  morf.timer(theme.duration_morph(), function()
    remove_pictures()
    fs.remove(M.held_path)
    morf.surface.session_lock = false
    if M.on_released then M.on_released() end
  end, false)
end

--- What PAM's answer means for the person at the lock. PAM reports a
--- refusal as "Authentication failure" (PAM_AUTH_ERR, which faillock also
--- uses) and a lockout as the maximum number of retries; anything else is
--- the stack failing to check at all, which must not read as a wrong
--- password.
function M.verdict(why)
  local reason = tostring(why or ""):lower()
  if reason:find("maximum") or reason:find("too many") or reason:find("retries") then
    return "Too many attempts"
  end
  if reason:find("authentication failure") or reason:find("permission denied")
    or reason:find("user not known") or reason:find("insufficient credentials") then
    return "Wrong password"
  end
  return "Authentication is unavailable"
end

--- Enter: the password to PAM, or on an empty field a look for a face.
function M.submit()
  if st.authenticating or st.leaving then return end
  if password == "" then
    M.scan()
    return
  end
  st.failed = false
  st.message = ""
  st.authenticating = true
  local attempt = password
  clear()
  morf.pam.authenticate(M.PASSWORD_SERVICE, account.user, attempt, function(ok, why)
    st.authenticating = false
    if ok then
      M.release()
      return
    end
    -- The face got there first; the lock is already on its way out.
    if st.leaving or st.face_matched then return end
    st.failed = true
    local verdict = M.verdict(why)
    st.message = verdict
    -- Only a refused password shakes the field; a PAM that could not
    -- check anything is said, and logged, but is nobody's mistake.
    if verdict == "Authentication is unavailable" then
      morf.log("warn", "impasto: PAM error while unlocking: " .. tostring(why))
    else
      st.refusals = st.refusals + 1
    end
  end)
  attempt = nil
end

-- --------------------------------------------------------------- face --

-- A PAM service of its own, `impasto-face`, beside the password and never
-- inside it: an unrecognised face does not count against the password's
-- attempts, and the password never waits for the camera. Ready where the
-- original's `./setup system` put the service and `./setup face` put howdy.
-- (`/usr/lib/impasto/face`, run under pkexec, is only for enrolling faces
-- from Settings; the check itself needs no root.)
local function face_available()
  return exists("/etc/pam.d/impasto-face")
    and (exists("/usr/lib/security/pam_howdy.so") or exists("/lib/security/pam_howdy.so"))
end

-- howdy keeps its models where only root reads them, so the faces are
-- listed through the setup's helper under pkexec (its policy lets the
-- active user list without a password). The face is offered once a face is
-- enrolled -- or while the list cannot be read at all, as upstream's
-- `faceReady`, since a refused pkexec says nothing about the faces.
local FACE_HELPER = "/usr/lib/impasto/face"
local FACE_POLICY = "/usr/share/polkit-1/actions/org.impasto.face.policy"

local function list_faces()
  if not exists(FACE_HELPER) or not exists(FACE_POLICY) then return end
  local act = require("services.act")
  if act.dry then return end
  act.collect("listing the enrolled faces", "pkexec", { FACE_HELPER, "list" }, function(text, ok)
    if not ok then return end
    local faces = 0
    for line in (text or ""):gmatch("[^\n]+") do
      if line:match("^%d+,") then faces = faces + 1 end
    end
    st.face_ready = face_available() and faces > 0
  end, { timeout_ms = 10000 })
end

--- A scan starts only on an awake screen, never on its own: the camera
--- would otherwise find the face that has just locked the screen.
function M.wake()
  if not st.awake or st.face_misses >= FACE_TRIES or now_ms() < face_quiet_until then return end
  M.scan()
end

function M.scan()
  if not st.face_ready or st.leaving or st.face_matched or face_session then return end
  -- Only while the compositor says the session is hidden (upstream's
  -- `secure`): the camera is never for a lock that is not up.
  if M.holding and morf.session_lock_state() ~= "locked" then return end
  local conversation = morf.pam.session("impasto-face", account.user)
  face_session = conversation
  local looked = false
  conversation:on_message(function(m)
    if face_session ~= conversation then return end
    if m.kind == "info" or m.kind == "error" then
      -- howdy's first word is the camera coming on.
      looked = true
      st.face_scanning = true
    elseif m.kind == "prompt" then
      -- The face stack has nothing to ask a person; a question means it
      -- fell through to a password, which has a stack of its own.
      conversation:cancel()
    elseif m.kind == "finished" then
      face_session = nil
      st.face_scanning = false
      if st.leaving or not st.awake then return end
      if m.ok then
        st.face_matched = true
        morf.timer(FACE_HOLD, M.release, false)
        return
      end
      -- A refusal with nothing said first is howdy declining to look (no
      -- face enrolled, the lid shut), and shows nothing.
      face_quiet_until = now_ms() + (looked and FACE_REST or FACE_IDLE)
      if looked then
        st.face_misses = st.face_misses + 1
        st.face_missed = st.face_missed + 1
      end
    end
  end)
end

--- Starts the lock's own half. Called once by lock/screen.lua.
function M.begin_lock_process(hold)
  M.PASSWORD_SERVICE = password_service()
  st.face_ready = face_available()
  if st.face_ready then list_faces() end
  M.holding = hold
  if not hold then return end
  fs.mkdir(M.dir)
  -- "secure" is the compositor confirming the lock, written down for the
  -- shell (which suspends only then). A refused lock, or one the
  -- compositor ends by itself, ends this process: the shell sees it go and
  -- knows the session is not hidden.
  morf.on_session_lock_state(function(state)
    if state == "locked" then
      fs.write(M.held_path, tostring(morf.process_id or ""))
    elseif state == "failed" or (state == "unlocked" and not st.leaving) then
      morf.log("error", "impasto: the compositor " ..
        (state == "failed" and "refused the lock" or "ended the lock"))
      fs.remove(M.held_path)
      remove_pictures()
      morf.quit()
    end
  end)
end

--- What the pictures are for one output, for its lock surface: "" where
--- there is none.
function M.pictures(output)
  local desk, blurred = M.desk_path(output), M.blur_path(output)
  return exists(desk) and desk or "", exists(blurred) and blurred or ""
end

--- For previews and tests: put the lock in a state without anybody typing.
function M.preview(values)
  for key, value in pairs(values) do
    if key == "typed" then
      password = string.rep("x", value)
      st.typed = value
    else
      st[key] = value
    end
  end
end

return M
