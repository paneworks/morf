-- The lock: locking the session over the blurred desk, and opening it again.
--
-- Port of LockService.qml. In Quickshell the lock is a surface of the shell's
-- own process. In morf a session lock is a whole run of its own: a
-- configuration that says `morf.surface.session_lock = true` is taken by
-- morf as one lock client with a surface on every output, held until the
-- file clears that flag, and the process ends with the lock. So this file has
-- two halves, one per process:
--
--   the shell     `lock()` closes the island, photographs the desk, blurs a
--                 copy, and starts `morf init.lua -- lock`, then watches that
--                 process for as long as it lives. `locked()` and `secure()`
--                 are what the rest of the shell reads.
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
-- The screenshot goes to the runtime directory (tmpfs, user-only) and the
-- lock deletes it as it lets go. The password is a plain local handed to PAM
-- and dropped; it is never a signal.

local settings = require("services.settings")
local theme = require("theme")
local account = require("services.account")
local session = require("services.session")

local M = {}

local fs = morf.fs

M.dir = fs.join(morf.env("XDG_RUNTIME_DIR") or "/tmp", "impasto-morf")
M.desk_path = fs.join(M.dir, "lock-desk.jpg")
M.blur_path = fs.join(M.dir, "lock-blur.jpg")
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
local function blur_desk()
  local shot = morf.image.info(M.desk_path)
  if not shot then return launch() end
  local w = math.max(1, math.floor(shot.width / 4))
  local h = math.max(1, math.floor(shot.height / 4))
  -- `lockBlur` is the original's blurMax, a radius in screen pixels; a
  -- Gaussian's sigma is about half its radius, then quartered with the size.
  local sigma = math.max(0.5, math.min(100, (settings.lockBlur or 32) / 6))
  local queued = morf.image.process {
    source = M.desk_path, output = M.blur_path, quality = 85,
    ops = { { "resize", w, h, "exact" }, { "blur", sigma } },
    on_done = function() launch() end,
  }
  if not queued then launch() end
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
  fs.remove(M.desk_path)
  fs.remove(M.blur_path)
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
  -- closed island to be what is on screen.
  morf.timer(settle, function()
    local screen = (morf.screens or {})[1]
    local queued = pcall(morf.screencopy.save, {
      path = M.desk_path, quality = 90,
      output = screen and screen.name or nil,
      on_done = function(done) if done then blur_desk() else launch() end end,
    })
    if not queued then launch() end
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
    fs.remove(M.desk_path)
    fs.remove(M.blur_path)
    fs.remove(M.held_path)
    morf.surface.session_lock = false
    if M.on_released then M.on_released() end
  end, false)
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
    local reason = tostring(why or ""):lower()
    st.message = (reason:find("maximum") or reason:find("too many")) and "Too many attempts"
      or "Wrong password"
    st.refusals = st.refusals + 1
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

--- A scan starts only on an awake screen, never on its own: the camera
--- would otherwise find the face that has just locked the screen.
function M.wake()
  if not st.awake or st.face_misses >= FACE_TRIES or now_ms() < face_quiet_until then return end
  M.scan()
end

function M.scan()
  if not st.face_ready or st.leaving or st.face_matched or face_session then return end
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
  if not hold then return end
  -- There is no event for "the compositor says locked" in a configuration
  -- yet; the lock's loop is running by the time a timer fires, and the
  -- compositor answers the lock request within a round trip of it.
  fs.mkdir(M.dir)
  morf.timer(400, function()
    fs.write(M.held_path, tostring(morf.process_id or ""))
  end, false)
end

--- What the pictures are, for the lock surface: "" where there is none.
function M.pictures()
  return exists(M.desk_path) and M.desk_path or "",
         exists(M.blur_path) and M.blur_path or ""
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
